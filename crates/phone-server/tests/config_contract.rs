use phone_server::config::{Config, Mode, valid_number};

#[test]
fn browser_calls_use_registered_sip_for_new_and_existing_configurations() {
    use phone_server::config::Sip;
    let default: Sip = toml::from_str("").unwrap();
    assert_eq!(default.browser_endpoint, "PJSIP/browser");
    let saved: Sip = toml::from_str("browser_endpoint = 'Local/browser@home-endpoint'").unwrap();
    assert_eq!(saved.browser_endpoint, default.browser_endpoint);
    let custom: Sip = toml::from_str("browser_endpoint = 'PJSIP/office'").unwrap();
    assert_eq!(custom.browser_endpoint, "PJSIP/office");
}

#[test]
fn production_accepts_explicit_local_calls_with_ai_off() {
    let config: Config = toml::from_str(
        "mode = 'production'\npublic_origin = 'https://phone.example.ts.net'\n[ai]\nprovider = 'disabled'\n",
    ).unwrap();
    config.validate().unwrap();
}

#[test]
fn ntfy_is_optional_and_accepts_the_private_server_configuration() {
    let config: Config = toml::from_str(
        "[notifications.ntfy]\nserver = 'https://notify.example.ts.net:10443'\ntopic = 'home-status'\n",
    )
    .unwrap();
    assert!(config.validate().is_ok());
    assert!(Config::default().validate().is_ok());
}

#[test]
fn ntfy_configuration_refuses_ambiguous_origins_topics_and_secret_names() {
    let baseline: Config = toml::from_str(
        "[notifications.ntfy]\nserver = 'https://notify.example.ts.net:10443'\ntopic = 'home-status'\n",
    )
    .unwrap();
    for server in [
        "http://notify.example.ts.net",
        "http://localhost:8081",
        "https://notify.example.ts.net/topic",
        "https://user:password@notify.example.ts.net",
        "https://notify.example.ts.net/?auth=secret",
        "https://notify.example.ts.net/#topic",
        "file:///tmp/ntfy",
        "not-a-url",
    ] {
        let mut config = baseline.clone();
        config.notifications.ntfy.as_mut().unwrap().server = server.into();
        assert!(config.validate().is_err(), "{server}");
    }
    for topic in ["", "home/status", "home status", "着信", &"a".repeat(65)] {
        let mut config = baseline.clone();
        config.notifications.ntfy.as_mut().unwrap().topic = topic.into();
        assert!(config.validate().is_err());
    }
    for topic in ["a", "Home-status_1", &"a".repeat(64)] {
        let mut config = baseline.clone();
        config.notifications.ntfy.as_mut().unwrap().topic = topic.into();
        assert!(config.validate().is_ok());
    }
    for name in ["", "lowercase", "TOKEN-NAME", "TOKEN\n"] {
        let mut config = baseline.clone();
        config.notifications.ntfy.as_mut().unwrap().token_env = name.into();
        assert!(config.validate().is_err());
    }
    for server in ["http://127.0.0.1:8081", "http://[::1]:8081"] {
        let mut config = baseline.clone();
        config.notifications.ntfy.as_mut().unwrap().server = server.into();
        assert!(config.validate().is_ok());
        config.mode = Mode::Production;
        config.ai.provider = "openai".into();
        config.public_origin = "https://phone.example.ts.net".into();
        assert!(config.validate().is_err());
    }
}

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
    config.ai.provider = if cfg!(feature = "plugin-ai") {
        "openai"
    } else {
        "disabled"
    }
    .into();
    config.public_origin = "https://phone.example.ts.net".into();
    assert!(config.validate().is_ok());
    config.ai.provider = "disabled".into();
    config.listen = "0.0.0.0:8787".parse().unwrap();
    assert!(config.validate().is_err());
}

#[test]
fn configurations_from_earlier_releases_keep_loading_without_user_lists() {
    let config: Config = toml::from_str(
        "mode = 'production'\npublic_origin = 'https://phone.example.ts.net'\nallowed_users = ['person@example.com']\n[ai]\nprovider = 'disabled'\n",
    )
    .unwrap();
    config.validate().unwrap();
    assert!(!toml::to_string(&config).unwrap().contains("allowed_users"));
    assert!(toml::from_str::<Config>("allowed_users = 'person@example.com'").is_err());
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
