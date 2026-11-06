use phone_core::Classification;
use phone_server::media::{Purpose, parse_route, session_configuration};

#[test]
fn ai_tools_can_only_select_the_declared_call_routes() {
    for (name, expected) in [
        ("continue", Classification::Continue),
        ("spam", Classification::Spam),
        ("suspected_spam", Classification::SuspectedSpam),
        ("human", Classification::Human),
        ("message", Classification::Message),
    ] {
        let arguments =
            serde_json::json!({"classification":name,"reason":"The caller stated a purpose."})
                .to_string();
        assert_eq!(parse_route("route_call", &arguments).unwrap(), expected);
    }
    for (name, arguments) in [
        ("originate", "{}"),
        (
            "route_call",
            "{\"classification\":\"unknown\",\"reason\":\"x\"}",
        ),
        (
            "route_call",
            "{\"classification\":\"spam\",\"reason\":\"\"}",
        ),
        (
            "route_call",
            "{\"classification\":\"human\",\"reason\":\"x\",\"number\":\"123\"}",
        ),
    ] {
        assert!(parse_route(name, arguments).is_err());
    }
}

#[test]
fn session_configuration_uses_pcmu_and_constrains_read_aloud_permissions() {
    let screening = session_configuration(Purpose::Screening, "gpt-realtime-2.1", "configured-asr");
    assert_eq!(
        screening["session"]["audio"]["input"]["transcription"]["model"],
        "configured-asr"
    );
    assert_eq!(screening["session"]["type"], "realtime");
    assert_eq!(
        screening["session"]["audio"]["input"]["format"]["type"],
        "audio/pcmu"
    );
    assert_eq!(
        screening["session"]["audio"]["output"]["format"]["type"],
        "audio/pcmu"
    );
    assert_eq!(screening["session"]["tools"][0]["name"], "route_call");
    let read_aloud =
        session_configuration(Purpose::ReadAloud, "gpt-realtime-2.1", "configured-asr");
    assert_eq!(read_aloud["session"]["tools"].as_array().unwrap().len(), 0);
    assert!(read_aloud["session"]["audio"]["input"]["turn_detection"].is_null());
}

#[tokio::test(start_paused = true)]
async fn websocket_bridge_transfers_audio_and_truncates_only_unplayed_output() {
    use base64::Engine as _;
    use futures_util::{SinkExt as _, StreamExt as _};
    use phone_server::media::{BridgeContext, Notice, bridge};
    use serde_json::json;
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::{Message, protocol::Role},
    };
    let (ast_client, ast_server) = tokio::io::duplex(65536);
    let (ai_client, ai_server) = tokio::io::duplex(65536);
    let ast_client = WebSocketStream::from_raw_socket(ast_client, Role::Client, None).await;
    let mut ast = WebSocketStream::from_raw_socket(ast_server, Role::Server, None).await;
    let ai_client = WebSocketStream::from_raw_socket(ai_client, Role::Client, None).await;
    let mut ai = WebSocketStream::from_raw_socket(ai_server, Role::Server, None).await;
    let (_commands, receiver) = tokio::sync::mpsc::channel(4);
    let (reports, mut reported) = tokio::sync::mpsc::channel(8);
    let stop = tokio_util::sync::CancellationToken::new();
    let context = BridgeContext {
        call_id: uuid::Uuid::new_v4(),
        generation: uuid::Uuid::new_v4(),
        purpose: Purpose::Screening,
        model: "gpt-realtime-2.1".into(),
        transcription_model: "configured-asr".into(),
    };
    let task = tokio::spawn(bridge(
        ast_client,
        ai_client,
        context,
        receiver,
        reports,
        tokio::sync::watch::channel(vec![]).0,
        stop.clone(),
    ));
    ast.send(Message::Text(
        json!({"event":"MEDIA_START","format":"ulaw","optimal_frame_size":160,"ptime":20})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let configuration: serde_json::Value =
        serde_json::from_str(ai.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(configuration["type"], "session.update");
    ai.send(Message::Text(
        json!({"type":"session.updated"}).to_string().into(),
    ))
    .await
    .unwrap();
    let greeting: serde_json::Value =
        serde_json::from_str(ai.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(greeting["type"], "response.create");
    ast.send(Message::Binary(vec![255; 160].into()))
        .await
        .unwrap();
    let input: serde_json::Value =
        serde_json::from_str(ai.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(input["type"], "input_audio_buffer.append");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(input["audio"].as_str().unwrap())
            .unwrap(),
        vec![255; 160]
    );
    ai.send(Message::Text(json!({"type":"response.output_audio.delta","response_id":"r1","item_id":"i1","content_index":0,"delta":base64::engine::general_purpose::STANDARD.encode(vec![159;3200])}).to_string().into())).await.unwrap();
    let mut marks = vec![];
    let mut audio = vec![];
    while marks.len() < 2 {
        match ast.next().await.unwrap().unwrap() {
            Message::Binary(bytes) => audio.extend_from_slice(&bytes),
            Message::Text(text) => {
                let mark: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(mark["command"], "MARK_MEDIA");
                marks.push(mark["correlation_id"].as_str().unwrap().to_owned());
            }
            other => panic!("unexpected media frame: {other:?}"),
        }
    }
    assert_eq!(audio, vec![159; 3200]);
    ast.send(Message::Text(
        json!({"event":"MEDIA_MARK_PROCESSED","correlation_id":marks[0]})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    ast.send(Message::Binary(vec![255; 160].into()))
        .await
        .unwrap();
    let acknowledged: serde_json::Value =
        serde_json::from_str(ai.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(acknowledged["type"], "input_audio_buffer.append");
    ai.send(Message::Text(
        json!({"type":"input_audio_buffer.speech_started"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let cancel: serde_json::Value =
        serde_json::from_str(ai.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(cancel["type"], "response.cancel");
    let truncate: serde_json::Value =
        serde_json::from_str(ai.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(truncate["type"], "conversation.item.truncate");
    assert_eq!(truncate["item_id"], "i1");
    assert_eq!(truncate["audio_end_ms"], 200);
    let flush: serde_json::Value =
        serde_json::from_str(ast.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(flush["command"], "FLUSH_MEDIA");
    assert!(
        matches!(reported.recv().await.unwrap().notice, Notice::Truncate { item_id } if item_id == "i1")
    );
    ai.send(Message::Text(json!({"type":"response.function_call_arguments.done","response_id":"r2","name":"route_call","call_id":"tool1","arguments":"{\"classification\":\"suspected_spam\",\"reason\":\"Sales pitch.\"}"}).to_string().into())).await.unwrap();
    assert!(matches!(
        reported.recv().await.unwrap().notice,
        Notice::Route(Classification::SuspectedSpam)
    ));
    stop.cancel();
    assert!(task.await.unwrap().is_ok());
}
