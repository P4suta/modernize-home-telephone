use phone_server::config::{Config, Mode, valid_number};

#[test]
fn lab_is_local_and_requires_no_cloud_credentials() {
    let config = Config::default();
    assert!(config.validate().is_ok());
    assert_eq!(config.mode, Mode::Lab);
    assert!(config.listen.ip().is_loopback());
    assert_eq!(config.storage.retention_days, 30);
    assert_eq!(config.storage.recording_limit_bytes, 4_294_967_296);
}

#[test]
fn production_rejects_mock_ai_and_untrusted_http_access() {
    let mut config = Config {
        mode: Mode::Production,
        ..Config::default()
    };
    assert!(config.validate().is_err());
    config.ai.provider = "openai".into();
    config.allowed_users = vec!["owner@example.invalid".into()];
    config.public_origin = "https://phone.example.ts.net".into();
    assert!(config.validate().is_ok());
    config.listen = "0.0.0.0:8787".parse().unwrap();
    assert!(config.validate().is_err());
}

#[test]
fn malformed_configuration_is_refused_instead_of_ignored() {
    assert!(toml::from_str::<Config>("retentoin_days = 30").is_err());
    let mut config = Config::default();
    config.timers.ringing_seconds = 0;
    assert!(config.validate().is_err());
    config.timers.ringing_seconds = 30;
    config.ai.provider = "unknown".into();
    assert!(config.validate().is_err());
}

#[test]
fn dialed_numbers_cannot_inject_sip_or_dialplan_syntax() {
    for number in ["0", "0312345678", "+81312345678"] {
        assert!(valid_number(number), "{number}");
    }
    for number in [
        "",
        "+",
        "++81",
        "sip:a@b",
        "123/../../x",
        "123&456",
        "12;system",
        "１２３",
        "123\r\n",
        " 123",
        "123 ",
    ] {
        assert!(!valid_number(number), "{number}");
    }
    assert!(!valid_number(&"1".repeat(33)));
}
