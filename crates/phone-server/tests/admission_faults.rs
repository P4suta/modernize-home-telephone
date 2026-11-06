use axum::{
    Json, Router,
    extract::OriginalUri,
    http::{Method, StatusCode},
    routing::get,
};
use phone_server::{
    config::Config,
    controller::Control,
    gateway::{Ari, Caller, Channel},
    store::Store,
};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn a_failed_admission_commit_closes_its_external_channel_without_requiring_a_call_row() {
    let deleted = Arc::new(Mutex::new(Vec::new()));
    let observed = deleted.clone();
    let router = Router::new()
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
            get(|| async { Json(serde_json::json!({"channel_ids":["uncommitted-external"]})) }),
        )
        .route(
            "/ari/events",
            get(|socket: axum::extract::WebSocketUpgrade| async {
                socket
                    .on_upgrade(|mut socket| async move { while socket.recv().await.is_some() {} })
            }),
        )
        .fallback(axum::routing::any(
            move |method: Method, OriginalUri(uri): OriginalUri| {
                let observed = observed.clone();
                async move {
                    if method == Method::DELETE {
                        observed.lock().unwrap().push(uri.path().to_owned());
                    }
                    StatusCode::NO_CONTENT
                }
            },
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(&root.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut config = Config {
        data_dir: root.path().into(),
        ..Config::default()
    };
    config.ari.url = format!("http://{address}/ari");
    let ari = Ari::new(config.ari.clone(), "test-secret".into()).unwrap();
    let control = Control::start(config, ari, store.clone());
    control.wait_ready().await.unwrap();
    deleted.lock().unwrap().clear();
    store.with(|connection|{connection.execute_batch("CREATE TRIGGER reject_admission BEFORE INSERT ON calls BEGIN SELECT RAISE(FAIL,'injected admission failure'); END;")?;Ok(())}).await.unwrap();
    assert!(
        control
            .offer(
                Channel {
                    id: "uncommitted-external".into(),
                    state: "Ring".into(),
                    caller: Caller {
                        number: "123".into()
                    }
                },
                "inbound".into()
            )
            .await
            .is_err()
    );
    let cleaned = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if deleted
                .lock()
                .unwrap()
                .iter()
                .any(|path| path == "/ari/channels/uncommitted-external")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    control.shutdown().await;
    server.abort();
    assert!(store.calls().await.unwrap().is_empty());
    assert!(
        cleaned.is_ok(),
        "a failed database commit leaked an external channel"
    );
}
