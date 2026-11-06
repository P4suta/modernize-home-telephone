use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use phone_server::{
    config::Config,
    model::Call,
    notifications::{Notifier, signature},
    secrets::Secrets,
    store::Store,
};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn failed_webhooks_retry_with_the_same_event_id_and_an_authenticated_body() {
    type Captured = Arc<Mutex<Vec<(HeaderMap, String)>>>;
    async fn hook(
        State(captured): State<Captured>,
        headers: HeaderMap,
        body: String,
    ) -> StatusCode {
        let mut captured = captured.lock().unwrap();
        captured.push((headers, body));
        if captured.len() == 1 {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::NO_CONTENT
        }
    }
    let captured = Captured::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/hook", post(hook))
        .with_state(captured.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let call = Call::offered("123".into(), "inbound".into(), 10);
    let event = store.save(&call, "incoming", 10).await.unwrap();
    let mut config = Config::default();
    config.notifications.webhook_url = Some(format!("http://{address}/hook"));
    let secrets = Secrets {
        ari_password: "unused".into(),
        sip_password: "unused".into(),
        api_token: None,
        openai_key: None,
        webhook_secret: Some("test-signing-secret".into()),
        vapid_pem: None,
    };
    let notifier = Notifier::new(config, store.clone(), secrets).unwrap();
    notifier.pump(10).await.unwrap();
    assert_eq!(captured.lock().unwrap().len(), 1);
    assert!(store.due(14).await.unwrap().is_empty());
    notifier.pump(15).await.unwrap();
    assert!(store.due(100).await.unwrap().is_empty());
    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2);
    for (index, (headers, body)) in captured.iter().enumerate() {
        assert_eq!(headers["x-homephone-event-id"], event.id.to_string());
        let time = if index == 0 { 10 } else { 15 };
        assert_eq!(headers["x-homephone-timestamp"], time.to_string());
        assert_eq!(
            headers["x-homephone-signature"],
            signature("test-signing-secret", event.id, time, body).unwrap()
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(body).unwrap()["id"],
            event.id
        );
    }
    task.abort();
}
