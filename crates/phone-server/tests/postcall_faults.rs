use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use futures_util::StreamExt;
use phone_server::{
    config::Config,
    controller::now,
    gateway::Ari,
    model::{Call, Phase, Recording},
    postcall::Processor,
    store::Store,
};
use std::sync::Arc;
use tokio::sync::Notify;

fn wav() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(vec![]);
    let mut writer = hound::WavWriter::new(
        &mut bytes,
        hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..160 {
        writer.write_sample(100_i16).unwrap();
    }
    writer.finalize().unwrap();
    bytes.into_inner()
}
fn finished() -> Call {
    let mut call = Call::offered("123".into(), "inbound".into(), now());
    call.phase = Phase::Ended;
    call.ended_at = Some(now());
    call.recording = Recording::Pending;
    call
}

async fn fixture(
    app: Router,
    openai: bool,
) -> (
    Processor,
    Store,
    tempfile::TempDir,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(&root.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut config = Config {
        data_dir: root.path().into(),
        ..Config::default()
    };
    config.ai.provider = if openai { "openai" } else { "mock" }.into();
    config.ari.url = format!("http://{address}/ari");
    let ari = Ari::new(config.ari.clone(), "test-ari-secret".into()).unwrap();
    let processor = Processor::new_for_loopback(
        config,
        ari,
        store.clone(),
        Some("test-provider-key".into()),
        address,
    )
    .unwrap();
    (processor, store, root, server)
}

#[tokio::test]
async fn successful_recording_jobs_preserve_call_faults_and_clear_only_their_own_errors() {
    for (error, expected) in [
        ("ai_connection_failed", Some("ai_connection_failed")),
        ("recording_capture_failed", None),
        ("post_call_processing_failed", None),
        (
            "unrecognized_call_failure",
            Some("unrecognized_call_failure"),
        ),
    ] {
        let app = Router::new().route(
            "/ari/recordings/stored/{name}/file",
            get(|| async { wav() }),
        );
        let (processor, store, _root, server) = fixture(app, false).await;
        let mut call = finished();
        call.last_error = Some(error.into());
        store.save(&call, "ended", now()).await.unwrap();
        processor.process(call.id).await.unwrap();
        let saved = store.get(call.id).await.unwrap().unwrap();
        server.abort();
        assert_eq!(saved.last_error.as_deref(), expected);
        assert_eq!(saved.processing, "complete");
    }
}

#[tokio::test]
async fn capture_revalidates_crash_leftovers_before_committing_or_removing_the_source() {
    let expected = wav();
    let response = expected.clone();
    let (processor, store, root, server) = fixture(
        Router::new().route(
            "/ari/recordings/stored/{name}/file",
            get(move || {
                let bytes = response.clone();
                async move { bytes }
            }),
        ),
        false,
    )
    .await;
    let call = finished();
    store.save(&call, "ended", now()).await.unwrap();
    let directory = root.path().join("recordings");
    tokio::fs::create_dir_all(&directory).await.unwrap();
    let path = directory.join(format!("{}.wav", call.id));
    tokio::fs::write(&path, b"incomplete cached recording")
        .await
        .unwrap();
    let result = processor.capture(call.id).await;
    let actual = tokio::fs::read(&path).await.unwrap();
    let saved = store.get(call.id).await.unwrap().unwrap();
    server.abort();
    result.unwrap();
    assert_eq!(
        actual, expected,
        "a cached filename is not evidence of a complete recording"
    );
    assert_eq!(
        saved.recording,
        Recording::Available {
            bytes: expected.len() as u64
        }
    );
}

#[tokio::test]
async fn malformed_and_oversized_cloud_responses_preserve_the_recording_and_refuse_completion() {
    for (status, payload, chunked) in [
        (StatusCode::UNAUTHORIZED, "{}".into(), false),
        (StatusCode::TOO_MANY_REQUESTS, "{}".into(), false),
        (StatusCode::SERVICE_UNAVAILABLE, "{}".into(), false),
        (StatusCode::OK, "not-json".into(), false),
        (
            StatusCode::OK,
            serde_json::json!({"text":"用件","padding":"x".repeat(300000)}).to_string(),
            false,
        ),
        (
            StatusCode::OK,
            serde_json::json!({"text":"用件","padding":"x".repeat(300000)}).to_string(),
            true,
        ),
    ] {
        let bytes = wav();
        let app=Router::new()
            .route("/ari/recordings/stored/{name}/file",get(move ||{let bytes=bytes.clone();async move{bytes}}))
            .route("/v1/audio/transcriptions",post(move ||{let payload=payload.clone();async move{
                let body=if chunked {axum::body::Body::from_stream(futures_util::stream::iter(payload.into_bytes().chunks(8192).map(|bytes| Ok::<_,std::io::Error>(bytes.to_vec())).collect::<Vec<_>>()))}else{axum::body::Body::from(payload)};
                axum::http::Response::builder().status(status).header("content-type","application/json").body(body).unwrap()
            }}))
            .route("/v1/responses",post(||async{Json(serde_json::json!({"output":[{"content":[{"type":"output_text","text":"{\"summary\":\"用件\"}"}]}]}))}));
        let (processor, store, root, server) = fixture(app, true).await;
        let call = finished();
        store.save(&call, "ended", now()).await.unwrap();
        let result = processor.process(call.id).await;
        let saved = store.get(call.id).await.unwrap().unwrap();
        server.abort();
        assert!(result.is_err(), "status={status}, chunked={chunked}");
        assert!(matches!(saved.recording, Recording::Available { .. }));
        assert_ne!(saved.processing, "complete");
        assert!(
            root.path()
                .join("recordings")
                .join(format!("{}.wav", call.id))
                .is_file()
        );
    }
}

#[tokio::test]
async fn interrupted_downloads_remove_partial_files_and_can_be_retried() {
    let stall = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let injected = stall.clone();
    let app = Router::new().route(
        "/ari/recordings/stored/{name}/file",
        get(move || {
            let stall = injected.clone();
            async move {
                let chunks: std::pin::Pin<
                    Box<dyn futures_util::Stream<Item = Result<Vec<u8>, std::io::Error>> + Send>,
                > = if stall.load(std::sync::atomic::Ordering::SeqCst) {
                    Box::pin(
                        futures_util::stream::once(async { Ok(wav()[..44].to_vec()) })
                            .chain(futures_util::stream::pending()),
                    )
                } else {
                    Box::pin(futures_util::stream::once(async { Ok(wav()) }))
                };
                axum::body::Body::from_stream(chunks)
            }
        }),
    );
    let (processor, store, root, server) = fixture(app, false).await;
    let call = finished();
    store.save(&call, "ended", now()).await.unwrap();
    let downloading = processor.clone();
    let id = call.id;
    let task = tokio::spawn(async move { downloading.capture(id).await });
    let partial = root.path().join("recordings").join(format!("{id}.partial"));
    let final_path = partial.with_extension("wav");
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while tokio::fs::metadata(&partial).await.is_err() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!partial.exists());
    assert!(!final_path.exists());
    assert_eq!(
        store.get(id).await.unwrap().unwrap().recording,
        Recording::Pending
    );
    stall.store(false, std::sync::atomic::Ordering::SeqCst);
    processor.capture(id).await.unwrap();
    assert_eq!(tokio::fs::read(final_path).await.unwrap(), wav());
    server.abort();
}

#[tokio::test]
async fn stalled_cloud_inference_cannot_block_recording_capture_or_quota_enforcement() {
    let root = tempfile::tempdir().unwrap();
    let entered = Arc::new(Notify::new());
    let released = Arc::new(Notify::new());
    let signal = entered.clone();
    let barrier = released.clone();
    let bytes = wav();
    let app=Router::new()
        .route("/ari/recordings/stored/{name}/file",get(move ||{let bytes=bytes.clone();async move{bytes}}))
        .route("/ari/recordings/stored/{name}",axum::routing::delete(||async{StatusCode::NO_CONTENT}))
        .route("/v1/audio/transcriptions",post(move || {let signal=signal.clone();let barrier=barrier.clone();async move{signal.notify_one();barrier.notified().await;Json(serde_json::json!({"text":"試験の用件です。"}))}}))
        .route("/v1/responses",post(||async{Json(serde_json::json!({"output":[{"content":[{"type":"output_text","text":"{\"summary\":\"試験通話です。\"}"}]}]}))}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let store = Store::open(&root.path().join("phone.sqlite"))
        .await
        .unwrap();
    let first = finished();
    store.save(&first, "ended", now()).await.unwrap();
    let mut config = Config {
        data_dir: root.path().into(),
        ..Config::default()
    };
    config.ai.provider = "openai".into();
    config.ari.url = format!("http://{address}/ari");
    let ari = Ari::new(config.ari.clone(), "test-ari-secret".into()).unwrap();
    let processor = Processor::new_for_loopback(
        config,
        ari,
        store.clone(),
        Some("test-provider-key".into()),
        address,
    )
    .unwrap();
    let stop = tokio_util::sync::CancellationToken::new();
    let running = tokio::spawn(processor.run(stop.clone()));
    let started = tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified()).await;
    let second = finished();
    store.save(&second, "ended", now()).await.unwrap();
    let captured = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if store
                .get(second.id)
                .await
                .unwrap()
                .is_some_and(|call| matches!(call.recording, Recording::Available { .. }))
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    stop.cancel();
    released.notify_waiters();
    running.await.unwrap();
    server.abort();
    assert!(started.is_ok(), "the fixture never reached cloud inference");
    assert!(
        captured.is_ok(),
        "cloud latency blocked local recording capture"
    );
}
