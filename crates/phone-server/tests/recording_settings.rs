use axum::{body::Body, http::Request};
use phone_server::{
    app::router, config::Config, controller::Control, gateway::Ari, model::Settings,
    secrets::Secrets, store::Store,
};
use tower::ServiceExt;

#[cfg(unix)]
#[test]
fn recording_space_uses_the_smaller_of_the_data_and_spool_filesystems() {
    const SLACK: u64 = 1 << 30;
    let free = |path: &std::path::Path| {
        let stat = rustix::fs::statvfs(path).unwrap();
        stat.f_bavail * stat.f_frsize
    };
    let data = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let missing = data.path().join("absent-spool");
    let alone = phone_server::recordings::recording_free_space(data.path(), &missing).unwrap();
    assert!(alone.abs_diff(free(data.path())) < SLACK);
    let both = phone_server::recordings::recording_free_space(data.path(), spool.path()).unwrap();
    let expected = free(data.path()).min(free(spool.path()));
    assert!(both.abs_diff(expected) < SLACK, "{both} vs {expected}");
}

#[tokio::test]
async fn nas_settings_migrate_effective_retention_without_applying_legacy_expiry() {
    let directory = tempfile::tempdir().unwrap();
    let mut config: Config = toml::from_str("[storage]\nretention_days=0\n[recording]\nenabled=true\ngateway_spool_dir='/tmp/spool'\n[recording.nas]\narchive_id='36b82de3-b456-44b3-bd31-d7c2fe71f460'\nserver='nas.invalid'\nshare='home'\ndirectory='telephone'\nusername_env='NAS_USERNAME'\npassword_env='NAS_PASSWORD'\n").unwrap();
    config.data_dir = directory.path().into();
    config.ari.url = "http://127.0.0.1:9/ari/".into();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    store
        .set_settings(&Settings {
            retention_days: Some(30),
            ..Settings::default()
        })
        .await
        .unwrap();
    let secrets = Secrets::from_lookup_for_calls(
        &config,
        |name| (name == "HOMEPHONE_API_TOKEN").then(|| "a".repeat(32)),
        None,
    )
    .unwrap();
    let ari = Ari::new(config.ari.clone(), secrets.ari_password.clone()).unwrap();
    let control = Control::start(config.clone(), ari.clone(), store);
    let app = router(config, control.clone(), ari, secrets);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [127, 0, 0, 1],
                    12345,
                ))))
                .header("authorization", format!("Bearer {}", "a".repeat(32)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let settings: Settings = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap(),
    )
    .unwrap();
    control.shutdown().await;
    assert_eq!(settings.retention_days, Some(0));
}
