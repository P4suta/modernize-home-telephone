use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{FutureExt, SinkExt, StreamExt};
use phone_server::media::{BridgeContext, Notice, Purpose, Report, bridge};
use serde_json::{Value, json};
use tokio::io::DuplexStream;
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message, protocol::Role},
};

type Socket = WebSocketStream<DuplexStream>;
struct Lab {
    ast: Socket,
    ai: Socket,
    reports: tokio::sync::mpsc::Receiver<Report>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
    stop: tokio_util::sync::CancellationToken,
    _commands: tokio::sync::mpsc::Sender<String>,
}
async fn send(socket: &mut Socket, event: Value) {
    socket
        .send(Message::Text(event.to_string().into()))
        .await
        .unwrap();
}
async fn read(socket: &mut Socket) -> Value {
    serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap()
}
impl Lab {
    async fn new() -> Self {
        let (ast_client, ast_server) = tokio::io::duplex(262144);
        let (ai_client, ai_server) = tokio::io::duplex(262144);
        let ast_client = WebSocketStream::from_raw_socket(ast_client, Role::Client, None).await;
        let ai_client = WebSocketStream::from_raw_socket(ai_client, Role::Client, None).await;
        let ast = WebSocketStream::from_raw_socket(ast_server, Role::Server, None).await;
        let ai = WebSocketStream::from_raw_socket(ai_server, Role::Server, None).await;
        let (commands, receiver) = tokio::sync::mpsc::channel(4);
        let (reports, reported) = tokio::sync::mpsc::channel(16);
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
        Self {
            ast,
            ai,
            reports: reported,
            task,
            stop,
            _commands: commands,
        }
    }
    async fn start(&mut self) {
        send(
            &mut self.ast,
            json!({"event":"MEDIA_START","format":"ulaw","optimal_frame_size":160,"ptime":20}),
        )
        .await;
        assert_eq!(read(&mut self.ai).await["type"], "session.update");
    }
    async fn ready(&mut self) {
        send(&mut self.ai, json!({"type":"session.updated"})).await;
        assert_eq!(read(&mut self.ai).await["type"], "response.create");
    }
    async fn barrier(&mut self) {
        self.ast
            .send(Message::Binary(vec![255; 160].into()))
            .await
            .unwrap();
        assert_eq!(
            read(&mut self.ai).await["type"],
            "input_audio_buffer.append"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn backpressure_defers_output_until_xon_and_cancellation_discards_late_audio() {
    let mut lab = Lab::new().await;
    lab.start().await;
    lab.ready().await;
    send(&mut lab.ast, json!({"event":"MEDIA_XOFF"})).await;
    lab.barrier().await;
    send(&mut lab.ai,json!({"type":"response.output_audio.delta","response_id":"r","item_id":"i","content_index":0,"delta":STANDARD.encode([159;160])})).await;
    send(&mut lab.ai,json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"barrier","transcript":"用件"})).await;
    assert!(matches!(
        lab.reports.recv().await.unwrap().notice,
        Notice::Transcript { .. }
    ));
    lab.barrier().await;
    assert!(
        lab.ast.next().now_or_never().is_none(),
        "media escaped while XOFF was active"
    );
    send(&mut lab.ast, json!({"event":"MEDIA_XON"})).await;
    assert_eq!(
        lab.ast.next().await.unwrap().unwrap().into_data(),
        vec![159; 160]
    );
    assert_eq!(read(&mut lab.ast).await["command"], "MARK_MEDIA");
    send(
        &mut lab.ai,
        json!({"type":"input_audio_buffer.speech_started"}),
    )
    .await;
    assert_eq!(read(&mut lab.ast).await["command"], "FLUSH_MEDIA");
    assert_eq!(read(&mut lab.ai).await["type"], "response.cancel");
    assert_eq!(
        read(&mut lab.ai).await["type"],
        "conversation.item.truncate"
    );
    send(&mut lab.ai,json!({"type":"response.output_audio.delta","response_id":"r","item_id":"i","content_index":0,"delta":STANDARD.encode([31;160])})).await;
    send(&mut lab.ai,json!({"type":"conversation.item.input_audio_transcription.completed","item_id":"after-cancel","transcript":"用件"})).await;
    assert!(matches!(
        lab.reports.recv().await.unwrap().notice,
        Notice::Truncate { .. }
    ));
    assert!(matches!(
        lab.reports.recv().await.unwrap().notice,
        Notice::Transcript { .. }
    ));
    lab.barrier().await;
    assert!(
        lab.ast.next().now_or_never().is_none(),
        "cancelled audio escaped to the caller"
    );
    lab.stop.cancel();
    lab.task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn unsupported_formats_and_excess_startup_audio_fail_before_unbounded_buffering() {
    for (format, frame, ptime) in [("slin16", 160, 20), ("ulaw", 320, 20), ("ulaw", 160, 40)] {
        let mut lab = Lab::new().await;
        send(
            &mut lab.ast,
            json!({"event":"MEDIA_START","format":format,"optimal_frame_size":frame,"ptime":ptime}),
        )
        .await;
        assert!(
            lab.task
                .await
                .unwrap()
                .unwrap_err()
                .is::<phone_server::media::StartupFault>()
        );
    }
    let mut lab = Lab::new().await;
    lab.start().await;
    lab.ast
        .send(Message::Binary(vec![255; 65500].into()))
        .await
        .unwrap();
    lab.ast
        .send(Message::Binary(vec![255; 37].into()))
        .await
        .unwrap();
    assert!(
        lab.task
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("startup audio queue exceeded")
    );
}
