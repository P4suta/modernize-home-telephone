use phone_server::{
    config::Config,
    secrets::{Secrets, parse_file},
};

#[cfg(all(feature = "plugin-nas", feature = "plugin-ntfy"))]
#[test]
fn optional_nas_and_notification_credentials_fail_independently() {
    let config: Config = toml::from_str("[notifications.ntfy]\nserver='https://notify.example.ts.net'\ntopic='calls'\n[recording]\nenabled=true\ngateway_spool_dir='/tmp/spool'\n[recording.nas]\narchive_id='36b82de3-b456-44b3-bd31-d7c2fe71f460'\nserver='nas.invalid'\nshare='home'\ndirectory='telephone'\nusername_env='NAS_USERNAME'\npassword_env='NAS_PASSWORD'\n").unwrap();
    for nas_available in [false, true] {
        for ntfy_available in [false, true] {
            let loaded = Secrets::from_lookup_for_calls(
                &config,
                |name| match name {
                    "NAS_USERNAME" if nas_available => Some("recorder".into()),
                    "NAS_PASSWORD" if nas_available => Some("synthetic-private-value".into()),
                    "HOMEPHONE_NTFY_TOKEN" if ntfy_available => {
                        Some("synthetic-publisher-token".into())
                    }
                    _ => None,
                },
                None,
            )
            .unwrap();
            assert_eq!(loaded.nas_credentials.is_some(), nas_available);
            assert_eq!(loaded.ntfy_token.is_some(), ntfy_available);
            assert_eq!(loaded.ari_password, "lab-ari-password");
        }
    }
    for (username, password) in [
        ("", "valid"),
        ("bad;name", "valid"),
        ("recorder", ""),
        ("recorder", "bad\nvalue"),
        ("recorder", "bad\0value"),
    ] {
        let loaded = Secrets::from_lookup_for_calls(
            &config,
            |name| match name {
                "NAS_USERNAME" => Some(username.into()),
                "NAS_PASSWORD" => Some(password.into()),
                "HOMEPHONE_NTFY_TOKEN" => Some("synthetic-publisher-token".into()),
                _ => None,
            },
            None,
        )
        .unwrap();
        assert!(loaded.nas_credentials.is_none());
        assert!(loaded.ntfy_token.is_some());
    }
}

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
fn literal_secret_files_preserve_password_whitespace() {
    let values = parse_file("  # Comment\r\nTOKEN= value with spaces \r\n").unwrap();
    assert_eq!(values["TOKEN"], " value with spaces ");
}

#[cfg(feature = "plugin-nas")]
#[test]
fn nas_credential_export_preserves_call_secrets_and_refuses_colliding_names() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("secrets.env");
    let output = directory.path().join("nas.env");
    std::fs::write(
        &input,
        "HOMEPHONE_SIP_PASSWORD=unchanged\nCUSTOM=literal$(value)=# \n",
    )
    .unwrap();
    let config: Config = toml::from_str("[ai]\nprovider='disabled'\n[storage]\nretention_days=0\n[recording]\nenabled=true\ngateway_spool_dir='/tmp/spool'\n[recording.nas]\narchive_id='36b82de3-b456-44b3-bd31-d7c2fe71f460'\nserver='nas.invalid'\nshare='home'\ndirectory='telephone'\nusername_env='NAS_USERNAME'\npassword_env='NAS_PASSWORD'\n").unwrap();
    let lookup = |name: &str| match name {
        "NAS_USERNAME" => Some("recorder fixture".into()),
        "NAS_PASSWORD" => Some(" literal$()=#'\" ".into()),
        _ => None,
    };
    Secrets::export_nas_credentials(&config, &input, &output, lookup).unwrap();
    let values = parse_file(&std::fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(values["NAS_USERNAME"], "recorder fixture");
    assert_eq!(values["NAS_PASSWORD"], " literal$()=#'\" ");
    assert_eq!(values["HOMEPHONE_SIP_PASSWORD"], "unchanged");
    assert_eq!(values["CUSTOM"], "literal$(value)=# ");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(Secrets::export_nas_credentials(&config, &input, &output, lookup).is_err());
    let absent = directory.path().join("absent.env");
    assert!(Secrets::export_nas_credentials(&config, &input, &absent, |_| None).is_err());
    assert!(!absent.exists());
    let mut collision = config;
    collision
        .recording
        .as_mut()
        .unwrap()
        .nas
        .as_mut()
        .unwrap()
        .username_env = collision.api_token_env.clone();
    assert!(
        Secrets::export_nas_credentials(&collision, &input, &absent, |_| Some("synthetic".into()))
            .is_err()
    );
    assert!(!absent.exists());
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
