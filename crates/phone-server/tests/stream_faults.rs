use axum::{body::Body, extract::ConnectInfo, http::Request};
use http_body_util::BodyExt;
use phone_server::{
    app::router,
    config::Config,
    controller::Control,
    gateway::Ari,
    model::{Call, Phase},
    secrets::Secrets,
    store::Store,
};
use tower::ServiceExt;

#[tokio::test]
async fn committed_background_updates_and_deletions_wake_the_stream_before_its_poll_timer() {
    let root = tempfile::tempdir().unwrap();
    let mut config = Config {
        data_dir: root.path().into(),
        ..Config::default()
    };
    config.ari.url = "http://127.0.0.1:0/ari/".into();
    let secrets = Secrets {
        ari_password: "local-test".into(),
        sip_password: "local-test".into(),
        api_token: None,
        openai_key: None,
        webhook_secret: None,
        vapid_pem: None,
    };
    let ari = Ari::new(config.ari.clone(), secrets.ari_password.clone()).unwrap();
    let store = Store::open(&root.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut call = Call::offered("999001".into(), "inbound".into(), 1);
    call.phase = Phase::Ended;
    call.ended_at = Some(2);
    store.save(&call, "ended", 2).await.unwrap();
    let control = Control::start(config.clone(), ari.clone(), store.clone());
    let app = router(config, control.clone(), ari, secrets);
    let mut request = Request::builder()
        .uri("/api/events")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = app.oneshot(request).await.unwrap();
    let mut body = response.into_body();
    let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert!(String::from_utf8_lossy(&first).contains("event: snapshot"));
    let metadata = body.frame().await.unwrap().unwrap().into_data().unwrap();
    assert!(String::from_utf8_lossy(&metadata).contains("event: status"));
    async fn next_call(body: &mut Body) -> phone_server::model::Event {
        tokio::time::timeout(std::time::Duration::from_millis(100), async {
            loop {
                let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
                let text = String::from_utf8(frame.to_vec()).unwrap();
                if text.lines().any(|line| line == "event: call") {
                    let payload = text
                        .lines()
                        .find_map(|line| line.strip_prefix("data: "))
                        .unwrap();
                    return serde_json::from_str(payload).unwrap();
                }
            }
        })
        .await
        .expect("a committed call event must wake SSE without polling")
    }
    let updated = store
        .update_finished(call.id, "complete", 3, |call| {
            call.processing = "complete".into();
        })
        .await
        .unwrap()
        .unwrap();
    tokio::time::pause();
    assert_eq!(next_call(&mut body).await.id, updated.id);
    tokio::time::resume();
    let deleted = store.delete_finished(call.id, 4).await.unwrap().unwrap();
    tokio::time::pause();
    assert_eq!(next_call(&mut body).await.id, deleted.id);
    tokio::time::resume();
    let mut observer = control.events.subscribe();
    let first = Call::offered("1".into(), "inbound".into(), 5);
    let committed = store.save(&first, "incoming", 5).await.unwrap();
    assert_eq!(observer.try_recv().unwrap().id, committed.id);
    let competing = Call::offered("2".into(), "inbound".into(), 6);
    assert!(store.save(&competing, "incoming", 6).await.is_err());
    assert!(matches!(
        observer.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}
#[tokio::test]
async fn an_idle_event_stream_observes_gateway_readiness_changes_without_call_events() {
    use axum::{Json, Router, extract::WebSocketUpgrade, routing::get};
    let gateway = Router::new()
        .route(
            "/ari/channels",
            get(|| async { Json(serde_json::json!([])) }),
        )
        .route(
            "/ari/bridges",
            get(|| async { Json(serde_json::json!([])) }),
        )
        .route(
            "/ari/applications/homephone",
            get(|| async { Json(serde_json::json!({"channel_ids":[]})) }),
        )
        .route(
            "/ari/events",
            get(|socket: WebSocketUpgrade| async {
                socket
                    .on_upgrade(|mut socket| async move { while socket.recv().await.is_some() {} })
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, gateway).await.unwrap() });
    let root = tempfile::tempdir().unwrap();
    let mut config = Config {
        data_dir: root.path().into(),
        ..Config::default()
    };
    config.ari.url = format!("http://{address}/ari/");
    let secrets = Secrets {
        ari_password: "local-test".into(),
        sip_password: "local-test".into(),
        api_token: None,
        openai_key: None,
        webhook_secret: None,
        vapid_pem: None,
    };
    let ari = Ari::new(config.ari.clone(), secrets.ari_password.clone()).unwrap();
    let store = Store::open(&root.path().join("phone.sqlite"))
        .await
        .unwrap();
    let control = Control::start(config.clone(), ari.clone(), store);
    control.wait_ready().await.unwrap();
    let app = router(config, control.clone(), ari, secrets);
    let mut request = Request::builder()
        .uri("/api/events")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let mut body = app.oneshot(request).await.unwrap().into_body();
    assert!(
        String::from_utf8_lossy(&body.frame().await.unwrap().unwrap().into_data().unwrap())
            .contains("event: snapshot")
    );
    tokio::time::pause();
    async fn status(body: &mut Body, ready: bool) {
        let bytes = tokio::time::timeout(std::time::Duration::from_millis(100), body.frame())
            .await
            .expect("readiness must wake the idle stream before polling")
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(text.contains("event: status"));
        let payload = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(value["gateway_ready"], ready);
    }
    status(&mut body, true).await;
    control.maintenance(true).await.unwrap();
    status(&mut body, false).await;
    control.maintenance(false).await.unwrap();
    status(&mut body, true).await;
    control.shutdown().await;
    server.abort();
}
