use phone_server::{
    config::Config,
    secrets::{Secrets, parse_file},
};

#[test]
fn missing_ntfy_credentials_do_not_prevent_loading_the_call_service() {
    let config: Config = toml::from_str(
        "[notifications.ntfy]\nserver = 'https://notify.example.ts.net'\ntopic = 'home-status'\n",
    )
    .unwrap();
    let loaded = Secrets::from_lookup_for_calls(&config, |_| None, None).unwrap();
    assert_eq!(loaded.ari_password, "lab-ari-password");
    assert!(loaded.ntfy_token.is_none());
    let mut production = config;
    production.mode = phone_server::config::Mode::Production;
    assert!(Secrets::from_lookup_for_calls(&production, |_| None, None).is_err());
}

#[test]
fn ntfy_requires_a_private_header_safe_publisher_token_when_enabled() {
    let config: Config = toml::from_str(
        "[notifications.ntfy]\nserver = 'https://notify.example.ts.net:10443'\ntopic = 'home-status'\n",
    )
    .unwrap();
    assert!(Secrets::from_lookup(&config, |_| None, None).is_err());
    for value in ["", "bad token", "bad\r\nInjected: value", "トークン"] {
        assert!(
            Secrets::from_lookup(
                &config,
                |name| (name == "HOMEPHONE_NTFY_TOKEN").then(|| value.into()),
                None
            )
            .is_err()
        );
    }
    assert!(
        Secrets::from_lookup(
            &config,
            |name| (name == "HOMEPHONE_NTFY_TOKEN").then(|| "a".repeat(4097)),
            None
        )
        .is_err()
    );
    assert!(
        Secrets::from_lookup(
            &config,
            |name| (name == "HOMEPHONE_NTFY_TOKEN").then(|| "a".repeat(4096)),
            None
        )
        .is_ok()
    );
    assert!(
        Secrets::from_lookup(
            &config,
            |name| (name == "HOMEPHONE_NTFY_TOKEN").then(|| "synthetic-ntfy-token".into()),
            None
        )
        .is_ok()
    );
    assert!(Secrets::from_lookup(&Config::default(), |_| None, None).is_ok());
}

#[test]
fn secret_files_are_data_and_reject_ambiguous_assignments() {
    let values = parse_file("# Private values\nTOKEN=$(not-executed)\nKEY=base64==\n").unwrap();
    assert_eq!(values["TOKEN"], "$(not-executed)");
    assert_eq!(values["KEY"], "base64==");
    assert!(parse_file("TOKEN=one\nTOKEN=two\n").is_err());
    assert!(parse_file("export TOKEN=value").is_err());
    assert!(parse_file("INVALID").is_err());
}

#[test]
fn vapid_key_references_resolve_relative_to_the_private_configuration() {
    let directory = tempfile::tempdir().unwrap();
    let group = openssl::ec::EcGroup::from_curve_name(openssl::nid::Nid::X9_62_PRIME256V1).unwrap();
    let key = openssl::ec::EcKey::generate(&group).unwrap();
    std::fs::write(
        directory.path().join("vapid.pem"),
        key.private_key_to_pem().unwrap(),
    )
    .unwrap();
    let config = Config::default();
    let secrets = Secrets::from_lookup(
        &config,
        |name| (name == config.notifications.vapid_private_key_env).then(|| "@vapid.pem".into()),
        Some(directory.path()),
    )
    .unwrap();
    assert!(phone_server::notifications::public_key(secrets.vapid_pem.as_deref()).is_some());
    assert!(
        Secrets::from_lookup(
            &config,
            |name| (name == config.notifications.vapid_private_key_env).then(|| "invalid".into()),
            None
        )
        .is_err()
    );
}
