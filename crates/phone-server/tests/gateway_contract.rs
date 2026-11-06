use axum::{
    Json, Router,
    extract::OriginalUri,
    http::{HeaderMap, StatusCode},
    routing::get,
};
use phone_server::{
    config::Config,
    gateway::{Ari, Channel},
};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn gateway_websockets_refuse_messages_larger_than_the_declared_transport_bound() {
    use futures_util::StreamExt;
    for size in [524288, 524289] {
        let app = Router::new().route(
            "/ari/events",
            get(move |socket: axum::extract::WebSocketUpgrade| async move {
                socket.on_upgrade(move |mut socket| async move {
                    let _ = socket
                        .send(axum::extract::ws::Message::Binary(vec![0; size].into()))
                        .await;
                })
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut config = Config::default().ari;
        config.url = format!("http://{address}/ari/");
        let client = Ari::new(config, "test-secret".into()).unwrap();
        let mut socket = client.connect_events().await.unwrap();
        let result = socket.next().await.unwrap();
        server.abort();
        if size == 524288 {
            assert_eq!(result.unwrap().into_data().len(), size);
        } else {
            assert!(matches!(
                result,
                Err(tokio_tungstenite::tungstenite::Error::Capacity(_))
            ));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn channel_readiness_bounds_identifiers_and_live_channel_inventory() {
    use phone_server::gateway::AriEvent;
    let client = Ari::new(Config::default().ari, "test-secret".into()).unwrap();
    let event = |id: String| AriEvent::StasisStart {
        channel: Channel {
            id,
            state: "Up".into(),
            caller: Default::default(),
        },
        args: vec![],
    };
    for id in [String::new(), "x".repeat(257), "bad\nidentity".into()] {
        assert!(client.observe(&event(id)).is_err());
    }
    for id in 0..128 {
        client.observe(&event(format!("channel-{id}"))).unwrap();
    }
    assert!(client.observe(&event("overflow".into())).is_err());
    client.observe(&event("channel-0".into())).unwrap();
    client.reset_events();
    client.observe(&event("after-reset".into())).unwrap();
}

#[tokio::test(start_paused = true)]
async fn stasis_readiness_requires_the_exact_live_channel_and_resets_on_disconnect() {
    use futures_util::FutureExt;
    use phone_server::gateway::AriEvent;
    let client = Ari::new(Config::default().ari, "test-secret".into()).unwrap();
    let event = |id: &str| AriEvent::StasisStart {
        channel: Channel {
            id: id.into(),
            state: "Up".into(),
            caller: Default::default(),
        },
        args: vec![],
    };
    let mut waiting = Box::pin(client.wait_for_stasis("expected"));
    assert!(waiting.as_mut().now_or_never().is_none());
    client.observe(&event("other")).unwrap();
    assert!(waiting.as_mut().now_or_never().is_none());
    client.observe(&event("expected")).unwrap();
    waiting.await.unwrap();
    client
        .observe(&AriEvent::StasisEnd {
            channel: Channel {
                id: "expected".into(),
                state: "Down".into(),
                caller: Default::default(),
            },
        })
        .unwrap();
    let mut ended = Box::pin(client.wait_for_stasis("expected"));
    assert!(ended.as_mut().now_or_never().is_none());
    client.observe(&event("expected")).unwrap();
    client.reset_events();
    let mut disconnected = Box::pin(client.wait_for_stasis("expected"));
    assert!(disconnected.as_mut().now_or_never().is_none());
    assert!(client.wait_for_stasis("absent").await.is_err());
}

#[tokio::test]
async fn ari_uses_basic_auth_and_encodes_resource_segments_and_query_values() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let received = requests.clone();
    let router = Router::new().fallback(get(
        move |OriginalUri(uri): OriginalUri, headers: HeaderMap| {
            let received = received.clone();
            async move {
                received.lock().unwrap().push((uri.to_string(), headers));
                Json(serde_json::json!({"id":"channel-1","state":"Up","caller":{"number":"1"}}))
            }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = Config::default().ari;
    config.url = format!("http://{address}/ari/");
    let client = Ari::new(config, "secret".into()).unwrap();
    let channel: Channel = client
        .read(&["channels", "id/with?reserved"], &[("variable", "a&b")])
        .await
        .unwrap();
    assert_eq!(channel.id, "channel-1");
    let locked = requests.lock().unwrap();
    assert_eq!(locked.len(), 1);
    assert_eq!(
        locked[0].0,
        "/ari/channels/id%2Fwith%3Freserved?variable=a%26b"
    );
    assert_eq!(
        locked[0].1.get("authorization").unwrap(),
        "Basic aG9tZXBob25lOnNlY3JldA=="
    );
    task.abort();
}

#[tokio::test]
async fn ari_distinguishes_not_found_cleanup_from_authentication_failure() {
    let router = Router::new()
        .route(
            "/ari/channels/missing",
            axum::routing::delete(|| async { StatusCode::NOT_FOUND }),
        )
        .route(
            "/ari/channels/denied",
            axum::routing::delete(|| async { StatusCode::UNAUTHORIZED }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = Config::default().ari;
    config.url = format!("http://{address}/ari/");
    let client = Ari::new(config, "secret".into()).unwrap();
    assert!(
        client
            .command(reqwest::Method::DELETE, &["channels", "missing"], &[])
            .await
            .is_ok()
    );
    assert!(
        client
            .command(reqwest::Method::DELETE, &["channels", "denied"], &[])
            .await
            .is_err()
    );
    task.abort();
}
