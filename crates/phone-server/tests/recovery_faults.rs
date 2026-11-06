use phone_server::{config::Config, controller::Control, gateway::Ari, store::Store};
use tower::ServiceExt;

#[tokio::test]
async fn candidate_health_requires_the_event_stream_and_completed_reconciliation() {
    for (websocket, reconciliation, expected) in [
        (false, true, axum::http::StatusCode::SERVICE_UNAVAILABLE),
        (true, false, axum::http::StatusCode::SERVICE_UNAVAILABLE),
        (true, true, axum::http::StatusCode::NO_CONTENT),
    ] {
        let observed = std::sync::Arc::new(tokio::sync::Notify::new());
        let observed_request = observed.clone();
        let mut gateway = axum::Router::new()
            .route(
                "/ari/applications/homephone",
                axum::routing::get(|| async {
                    axum::Json(serde_json::json!({"name":"homephone","channel_ids":[]}))
                }),
            )
            .route(
                "/ari/channels",
                axum::routing::get(move || {
                    let observed = observed_request.clone();
                    async move {
                        observed.notify_one();
                        if reconciliation {
                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!([])),
                            )
                        } else {
                            (
                                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                                axum::Json(serde_json::json!({})),
                            )
                        }
                    }
                }),
            )
            .route(
                "/ari/bridges",
                axum::routing::get(|| async { axum::Json(serde_json::json!([])) }),
            );
        if websocket {
            gateway = gateway.route(
                "/ari/events",
                axum::routing::get(|socket: axum::extract::WebSocketUpgrade| async {
                    socket.on_upgrade(
                        |mut socket| async move { while socket.recv().await.is_some() {} },
                    )
                }),
            );
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, gateway).await.unwrap() });
        let root = tempfile::tempdir().unwrap();
        let mut config = Config {
            data_dir: root.path().into(),
            ..Config::default()
        };
        config.ari.url = format!("http://{address}/ari/");
        let ari = Ari::new(config.ari.clone(), "test-password".into()).unwrap();
        let store = Store::open(&root.path().join("phone.sqlite"))
            .await
            .unwrap();
        let control =
            Control::start_quiesced(config.clone(), ari.clone(), store.clone(), None, true);
        let app = phone_server::app::router(
            config,
            control.clone(),
            ari,
            phone_server::secrets::Secrets {
                ari_password: "test-password".into(),
                sip_password: "test-password".into(),
                api_token: Some("a".repeat(32)),
                openai_key: None,
                webhook_secret: None,
                vapid_pem: None,
            },
        );
        if websocket {
            tokio::time::timeout(std::time::Duration::from_secs(3), observed.notified())
                .await
                .unwrap();
        }
        let request = || {
            let mut request = axum::http::Request::builder()
                .uri("/api/admin/ready")
                .header("authorization", format!("Bearer {}", "a".repeat(32)))
                .body(axum::body::Body::empty())
                .unwrap();
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap(),
            ));
            request
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let status = app.clone().oneshot(request()).await.unwrap().status();
                if expected != axum::http::StatusCode::NO_CONTENT || status == expected {
                    break status;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(!control.ready(), "a paused candidate must refuse new calls");
        assert!(store.background_paused());
        control.shutdown().await;
        server.abort();
        assert_eq!(
            result.unwrap(),
            expected,
            "websocket={websocket}, reconciliation={reconciliation}"
        );
    }
}

#[tokio::test]
async fn startup_freezes_recording_jobs_until_gateway_reconciliation_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(&root.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut config = Config::default();
    config.ari.url = "http://127.0.0.1:0/ari".into();
    let ari = Ari::new(config.ari.clone(), "private-test-secret".into()).unwrap();
    let control = Control::start(config, ari, store.clone());
    assert!(store.background_paused());
    control.shutdown().await;
}
