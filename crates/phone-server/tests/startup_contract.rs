use phone_server::config::Config;

#[test]
fn a_busy_http_port_is_refused_before_database_recovery_or_gateway_control() {
    let directory = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = Config {
        listen: listener.local_addr().unwrap(),
        data_dir: directory.path().join("private-data"),
        ..Config::default()
    };
    config.ari.url = "http://127.0.0.1:9/ari/".into();
    let path = directory.path().join("homephone.toml");
    std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_phone-server"))
        .arg("--config")
        .arg(path)
        .arg("serve")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!config.data_dir.join("phone.sqlite").exists());
}
