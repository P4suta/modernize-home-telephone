#![cfg(feature = "plugin-phonebook")]

use phone_server::config::Config;

fn configured(directory: &std::path::Path) -> std::path::PathBuf {
    let config = Config {
        data_dir: directory.join("private-data"),
        ..Config::default()
    };
    let path = directory.join("homephone.toml");
    std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    path
}

fn run(config: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_phone-server"))
        .arg("--config")
        .arg(config)
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn lookups_print_the_stored_label_or_report_an_unlisted_number() {
    let directory = tempfile::tempdir().unwrap();
    let config = configured(directory.path());
    let unlisted = run(&config, &["lookup-phonebook", "0775511234"]);
    assert!(unlisted.status.success());
    assert_eq!(String::from_utf8(unlisted.stdout).unwrap(), "未登録\n");
    let number = phone_core::directory::NationalNumber::from_dial(
        phone_core::DialNumber::parse(b"0775511234").unwrap(),
    )
    .unwrap();
    phone_plugin_phonebook::install(
        &directory.path().join("private-data"),
        &"1".repeat(64),
        &"2".repeat(64),
        1,
        &[(number, "さくら病院（病院・診療所）".into())],
    )
    .unwrap();
    for caller in ["0775511234", "+81775511234"] {
        let listed = run(&config, &["lookup-phonebook", caller]);
        assert!(listed.status.success());
        assert_eq!(
            String::from_utf8(listed.stdout).unwrap(),
            "さくら病院（病院・診療所）\n"
        );
    }
    let incomplete = run(&config, &["lookup-phonebook", "110"]);
    assert_eq!(String::from_utf8(incomplete.stdout).unwrap(), "未登録\n");
}

#[tokio::test]
async fn a_failed_sync_exits_unsuccessfully_without_creating_an_index() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = axum::Router::new().route(
        "/api/datasets",
        axum::routing::get(|| async { "not a listing" }),
    );
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let directory = tempfile::tempdir().unwrap();
    let config = configured(directory.path());
    let failed =
        tokio::task::spawn_blocking(move || run(&config, &["sync-phonebook", "--origin", &origin]))
            .await
            .unwrap();
    task.abort();
    assert!(!failed.status.success());
    assert!(
        !directory
            .path()
            .join("private-data")
            .join(phone_plugin_phonebook::INDEX_FILE)
            .exists()
    );
}
