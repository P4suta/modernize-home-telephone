const REALTIME_ENDPOINT: &str = "wss://api.openai.com/v1/realtime";
const COMMAND_QUEUE_CAPACITY: usize = 8;
const MOCK_TRANSPORT_BYTES: usize = 131072;
const MAX_WEBSOCKET_BYTES: usize = 524288;
const CONNECT_TIMEOUT_SECONDS: u64 = 5;
const MOCK_AUDIO_BYTES: usize = 8000;
const MOCK_TONE_PERIOD: usize = 8;
const MOCK_TONE_HALF_PERIOD: usize = 4;
const MOCK_PCMU_POSITIVE: u8 = 159;
const MOCK_PCMU_NEGATIVE: u8 = 31;

use crate::{
    config::Ai,
    gateway::{Ari, Variable},
    media::{self, BridgeContext, Notice, Purpose, Report},
};
use anyhow::{Context, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::HeaderValue,
        protocol::{Role, WebSocketConfig},
    },
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct Handle {
    pub generation: Uuid,
    pub channel: String,
    pub commands: mpsc::Sender<String>,
    pub unplayed: watch::Receiver<Vec<String>>,
    pub stop: CancellationToken,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub struct Start {
    pub call_id: Uuid,
    pub generation: Uuid,
    pub channel: String,
    pub bridge: String,
    pub caller: String,
    pub purpose: Purpose,
}

pub async fn start(
    ari: &Ari,
    ai: &Ai,
    key: Option<String>,
    start: Start,
    reports: mpsc::Sender<Report>,
) -> anyhow::Result<Handle> {
    ensure!(
        ai.provider == "mock" || key.is_some(),
        "AI credentials unavailable"
    );
    ari.command(
        Method::POST,
        &["channels", "externalMedia"],
        &[
            ("app", ari.application()),
            ("external_host", "INCOMING"),
            ("transport", "websocket"),
            ("encapsulation", "none"),
            ("connection_type", "server"),
            ("format", "ulaw"),
            ("direction", "both"),
            ("channelId", &start.channel),
            ("transport_data", "f(json)"),
        ],
    )
    .await
    .context(media::StartupFault::Create)?;
    let variable: Variable = ari
        .read(
            &["channels", &start.channel, "variable"],
            &[("variable", "MEDIA_WEBSOCKET_CONNECTION_ID")],
        )
        .await
        .context(media::StartupFault::Identifier)?;
    let socket = ari
        .connect_media(&variable.value)
        .await
        .context(media::StartupFault::Connect)?;
    ari.wait_for_stasis(&start.channel)
        .await
        .context(media::StartupFault::Stasis)?;
    ari.command(
        Method::POST,
        &["bridges", &start.bridge, "addChannel"],
        &[("channel", &start.channel)],
    )
    .await
    .context(media::StartupFault::Attach)?;
    let (commands, receiver) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
    let (unplayed, snapshot) = watch::channel(vec![]);
    let stop = CancellationToken::new();
    let handle = Handle {
        generation: start.generation,
        channel: start.channel.clone(),
        commands,
        unplayed: snapshot,
        stop: stop.clone(),
    };
    let config = ai.clone();
    tokio::spawn(async move {
        let context = BridgeContext {
            call_id: start.call_id,
            generation: start.generation,
            purpose: start.purpose,
            model: config.realtime_model,
            transcription_model: config.transcription_model,
        };
        let operation = async {
            if config.provider == "mock" {
                let (client, server) = tokio::io::duplex(MOCK_TRANSPORT_BYTES);
                let client = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
                let server = WebSocketStream::from_raw_socket(server, Role::Server, None).await;
                let mock_stop = stop.child_token();
                let mock_task =
                    tokio::spawn(mock(server, start.caller, start.purpose, mock_stop.clone()));
                let result = media::bridge(
                    socket,
                    client,
                    context,
                    receiver,
                    reports.clone(),
                    unplayed,
                    stop.clone(),
                )
                .await;
                mock_stop.cancel();
                if (mock_task.await).is_err() {
                    tracing::warn!(operation = "provider", code = "mock_task_join_failed");
                }
                result
            } else {
                let mut url = url::Url::parse(REALTIME_ENDPOINT)?;
                url.query_pairs_mut().append_pair("model", &context.model);
                let mut request = url.as_str().into_client_request()?;
                let mut authorization =
                    HeaderValue::from_str(&format!("Bearer {}", key.context("missing AI key")?))?;
                authorization.set_sensitive(true);
                request.headers_mut().insert("authorization", authorization);
                let limits = WebSocketConfig::default()
                    .max_message_size(Some(MAX_WEBSOCKET_BYTES))
                    .max_frame_size(Some(MAX_WEBSOCKET_BYTES));
                let (provider, _) = tokio::select! {
                    _ = stop.cancelled() => return Ok(()),
                    connected = tokio::time::timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECONDS), tokio_tungstenite::connect_async_with_config(request, Some(limits), false)) => connected??,
                };
                media::bridge(
                    socket,
                    provider,
                    context,
                    receiver,
                    reports.clone(),
                    unplayed,
                    stop.clone(),
                )
                .await
            }
        };
        if let Err(error) = operation.await
            && !stop.is_cancelled()
        {
            let code = error
                .downcast_ref::<media::StartupFault>()
                .map_or_else(|| "media_operation_failed".into(), ToString::to_string);
            tracing::warn!(operation="media_bridge",call_id=%start.call_id,generation=%start.generation,code);
            if (reports
                .send(Report {
                    call_id: start.call_id,
                    generation: start.generation,
                    notice: Notice::Failed,
                })
                .await)
                .is_err()
            {
                tracing::debug!(operation = "provider", code = "controller_receiver_closed");
            }
        }
    });
    Ok(handle)
}

async fn send<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    socket: &mut WebSocketStream<S>,
    value: Value,
) -> anyhow::Result<()> {
    socket.send(Message::Text(value.to_string().into())).await?;
    Ok(())
}

async fn mock(
    mut socket: WebSocketStream<tokio::io::DuplexStream>,
    caller: String,
    purpose: Purpose,
    stop: CancellationToken,
) -> anyhow::Result<()> {
    let mut input_bytes = 0_u64;
    let mut responses = 0_u32;
    let mut routed = false;
    loop {
        let frame = tokio::select! { _ = stop.cancelled() => return Ok(()), frame = socket.next() => frame.context("mock socket closed")?? };
        let Message::Text(text) = frame else {
            if matches!(frame, Message::Close(_)) {
                return Ok(());
            }
            continue;
        };
        let event: Value = serde_json::from_str(&text)?;
        match event["type"].as_str() {
            Some("session.update") => send(&mut socket, json!({"type":"session.updated"})).await?,
            Some("response.create") => {
                responses += 1;
                let id = format!("mock-response-{responses}");
                let item = format!("mock-item-{responses}");
                send(
                    &mut socket,
                    json!({"type":"response.created","response":{"id":id}}),
                )
                .await?;
                let audio: Vec<u8> = (0..MOCK_AUDIO_BYTES)
                    .map(|index| {
                        if index % MOCK_TONE_PERIOD < MOCK_TONE_HALF_PERIOD {
                            MOCK_PCMU_POSITIVE
                        } else {
                            MOCK_PCMU_NEGATIVE
                        }
                    })
                    .collect();
                send(&mut socket, json!({"type":"response.output_audio.delta","response_id":id,"item_id":item,"content_index":0,"delta":STANDARD.encode(audio)})).await?;
                send(
                    &mut socket,
                    json!({"type":"response.output_audio.done","item_id":item}),
                )
                .await?;
                send(&mut socket, json!({"type":"response.output_audio_transcript.done","response_id":id,"item_id":item,"transcript":"AI受付の試験通話です。通話を録音します。お名前とご用件をお話しください。"})).await?;
                send(
                    &mut socket,
                    json!({"type":"response.done","response":{"id":id}}),
                )
                .await?;
            }
            Some("input_audio_buffer.append") if purpose == Purpose::Screening => {
                let audio = event["audio"]
                    .as_str()
                    .context("missing mock input audio")?;
                input_bytes = input_bytes
                    .checked_add(u64::try_from(STANDARD.decode(audio)?.len())?)
                    .context("mock audio counter overflow")?;
                if input_bytes >= MOCK_AUDIO_BYTES as u64 && !routed {
                    routed = true;
                    let classification = match caller.as_str() {
                        "999001" => "spam",
                        "999002" => "suspected_spam",
                        "999004" => "message",
                        "999005" => continue,
                        _ => "human",
                    };
                    send(&mut socket, json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"mock-caller","transcript":"仮想回線からの試験通話です。"})).await?;
                    send(&mut socket, json!({"type":"response.function_call_arguments.done","response_id":"mock-route","call_id":"mock-tool","name":"route_call","arguments":json!({"classification":classification,"reason":"Deterministic virtual-line scenario."}).to_string()})).await?;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn mock_classification_measures_audio_duration_independently_of_message_partitioning() {
        for chunk_size in [1, 160, 8000] {
            let (client, server) = tokio::io::duplex(131072);
            let mut client = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
            let server = WebSocketStream::from_raw_socket(server, Role::Server, None).await;
            let stop = CancellationToken::new();
            let task = tokio::spawn(mock(
                server,
                "999001".into(),
                Purpose::Screening,
                stop.clone(),
            ));
            for chunk in vec![255_u8; 7999].chunks(chunk_size) {
                send(
                    &mut client,
                    json!({"type":"input_audio_buffer.append","audio":STANDARD.encode(chunk)}),
                )
                .await
                .unwrap();
            }
            send(&mut client, json!({"type":"session.update"}))
                .await
                .unwrap();
            let barrier = client.next().await.unwrap().unwrap();
            let barrier: Value = serde_json::from_str(barrier.to_text().unwrap()).unwrap();
            assert_eq!(barrier["type"], "session.updated");
            send(
                &mut client,
                json!({"type":"input_audio_buffer.append","audio":STANDARD.encode([255])}),
            )
            .await
            .unwrap();
            send(&mut client, json!({"type":"session.update"}))
                .await
                .unwrap();
            let first = client.next().await.unwrap().unwrap();
            let value: Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
            assert_eq!(
                value["type"], "conversation.item.input_audio_transcription.completed",
                "chunk size {chunk_size}"
            );
            let route = client.next().await.unwrap().unwrap();
            let value: Value = serde_json::from_str(route.to_text().unwrap()).unwrap();
            assert_eq!(value["name"], "route_call");
            stop.cancel();
            task.await.unwrap().unwrap();
        }
    }
}
