use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use phone_server::{
    app::router,
    config::{Config, Mode},
    controller::Control,
    gateway::Ari,
    secrets::Secrets,
    store::Store,
};
use tower::ServiceExt;

async fn app(production: bool) -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config {
        data_dir: dir.path().into(),
        ..Config::default()
    };
    config.ari.url = "http://127.0.0.1:0/ari".into();
    if production {
        config.mode = Mode::Production;
        config.ai.provider = "openai".into();
        config.allowed_users = vec!["owner@example.invalid".into()];
        config.public_origin = "https://phone.example.ts.net".into();
    }
    let secrets = Secrets {
        ari_password: "password".into(),
        sip_password: "sip-password".into(),
        api_token: Some("a".repeat(32)),
        openai_key: None,
        webhook_secret: None,
        vapid_pem: None,
    };
    let ari = Ari::new(config.ari.clone(), secrets.ari_password.clone()).unwrap();
    let store = Store::open(&dir.path().join("phone.sqlite")).await.unwrap();
    let control = Control::start(config.clone(), ari.clone(), store);
    (router(config, control, ari, secrets), dir)
}

fn request(path: &str, user: Option<&str>, peer: &str) -> Request<Body> {
    let mut request = Request::builder().uri(path).body(Body::empty()).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(peer.parse::<std::net::SocketAddr>().unwrap()));
    if let Some(user) = user {
        request
            .headers_mut()
            .insert("tailscale-user-login", user.parse().unwrap());
    }
    request
}

#[tokio::test]
async fn status_distinguishes_provider_configuration_from_a_live_external_connection() {
    for (production, expected) in [(false, true), (true, false)] {
        let (app, _root) = app(production).await;
        let response = app
            .oneshot(request(
                "/api/status",
                Some("owner@example.invalid"),
                "127.0.0.1:1234",
            ))
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let status: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status["ai_configured"], expected);
        assert!(status.get("ai_ready").is_none());
    }
}

#[tokio::test]
async fn lab_serves_calls_and_health_without_cloud_credentials() {
    let (app, _dir) = app(false).await;
    for path in [
        "/healthz",
        "/api/status",
        "/api/calls",
        "/api/settings",
        "/api/openapi.json",
    ] {
        let response = app
            .clone()
            .oneshot(request(path, None, "127.0.0.1:1234"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
    }
}

#[tokio::test]
async fn identity_headers_require_loopback_and_an_explicit_allowed_user() {
    let (app, _dir) = app(true).await;
    for (user, peer, expected) in [
        (None, "127.0.0.1:1234", StatusCode::UNAUTHORIZED),
        (
            Some("other@example.invalid"),
            "127.0.0.1:1234",
            StatusCode::UNAUTHORIZED,
        ),
        (
            Some("owner@example.invalid"),
            "192.0.2.1:1234",
            StatusCode::UNAUTHORIZED,
        ),
        (
            Some("owner@example.invalid"),
            "127.0.0.1:1234",
            StatusCode::OK,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(request("/api/calls", user, peer))
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn mutating_requests_refuse_foreign_origins_even_with_a_valid_identity() {
    let (app, _dir) = app(true).await;
    let mut request = request(
        "/api/calls",
        Some("owner@example.invalid"),
        "127.0.0.1:1234",
    );
    *request.method_mut() = axum::http::Method::POST;
    request
        .headers_mut()
        .insert("origin", "https://attacker.example".parse().unwrap());
    request
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    *request.body_mut() = Body::from("{\"number\":\"1\"}");
    assert_eq!(
        app.oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}
