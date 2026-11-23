#[tokio::test]
async fn ai_off_refuses_media_before_any_request_even_with_credentials() {
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = requests.clone();
    let router = axum::Router::new().fallback(axum::routing::any(move || {
        let observed = observed.clone();
        async move {
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            axum::http::StatusCode::NO_CONTENT
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = phone_server::config::Config::default();
    config.ai.provider = "disabled".into();
    config.ari.url = format!("http://{address}/ari/");
    let ari = phone_server::gateway::Ari::new(config.ari, "synthetic-secret".into()).unwrap();
    let (reports, _receiver) = tokio::sync::mpsc::channel(1);
    let result = phone_server::provider::start(
        &ari,
        &config.ai,
        Some("synthetic-key".into()),
        phone_server::provider::Start {
            call_id: uuid::Uuid::new_v4(),
            generation: uuid::Uuid::new_v4(),
            channel: "synthetic-media".into(),
            bridge: "synthetic-bridge".into(),
            caller: "123".into(),
            purpose: phone_server::media::Purpose::Screening,
        },
        reports,
    )
    .await;
    task.abort();
    assert!(result.is_err());
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 0);
}
