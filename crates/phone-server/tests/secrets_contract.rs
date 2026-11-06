use phone_server::{
    config::Config,
    secrets::{Secrets, parse_file},
};

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
