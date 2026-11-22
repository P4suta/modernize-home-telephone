use std::{fs, path::Path};
use xtask::{operations::initialize_with, rusterisk::prepare_existing};

fn installation(root: &Path) -> std::path::PathBuf {
    let directory = root.join("telephone");
    initialize_with(
        &directory,
        "192.0.2.10".parse().unwrap(),
        "https://phone.example.net".into(),
        Some("fixture@example.net".into()),
        false,
        |keys| {
            for name in ["vapid.pem", "dtls.crt", "dtls.key"] {
                fs::write(keys.join(name), "fixture")?;
            }
            Ok(())
        },
    )
    .unwrap();
    directory
}

#[test]
fn migrating_an_asterisk_installation_preserves_routing_credentials_and_audio() {
    let root = tempfile::tempdir().unwrap();
    let directory = installation(root.path());
    let config = directory.join("config");
    let native = config.join("rusterisk");
    let sip = fs::read(native.join("pjsip.conf")).unwrap();
    let ari = fs::read(native.join("ari.conf")).unwrap();
    fs::remove_dir_all(&native).unwrap();
    let original = fs::read(config.join("pjsip.conf")).unwrap();
    let dialplan = "[home-outbound]\nexten => 100,1,Stasis(homephone,outbound,100)\n";
    fs::write(config.join("extensions.conf"), dialplan).unwrap();
    let recording = directory.join("spool/recording/previous.wav");
    fs::write(&recording, b"previous audio").unwrap();
    prepare_existing(&directory).unwrap();
    prepare_existing(&directory).unwrap();
    assert_eq!(fs::read(native.join("pjsip.conf")).unwrap(), sip);
    assert_eq!(fs::read(native.join("ari.conf")).unwrap(), ari);
    assert_eq!(
        fs::read_to_string(native.join("extensions.conf")).unwrap(),
        dialplan
    );
    assert_eq!(fs::read(config.join("pjsip.conf")).unwrap(), original);
    assert_eq!(fs::read(recording).unwrap(), b"previous audio");
}

#[test]
fn the_published_asterisk_configuration_upgrades_with_its_original_credentials() {
    let root = tempfile::tempdir().unwrap();
    let directory = installation(root.path());
    let config = directory.join("config");
    let native = config.join("rusterisk");
    let expected = fs::read(native.join("pjsip.conf")).unwrap();
    fs::remove_dir_all(&native).unwrap();
    let path = config.join("pjsip.conf");
    let published = fs::read_to_string(&path)
        .unwrap()
        .replace("identify_by=auth_username,username\n", "");
    fs::write(&path, &published).unwrap();
    prepare_existing(&directory).unwrap();
    assert_eq!(fs::read(native.join("pjsip.conf")).unwrap(), expected);
    assert_eq!(fs::read_to_string(path).unwrap(), published);
}

#[test]
fn customized_or_incomplete_configuration_keeps_the_previous_gateway_recoverable() {
    let root = tempfile::tempdir().unwrap();
    let directory = installation(root.path());
    let config = directory.join("config");
    let native = config.join("rusterisk");
    fs::remove_dir_all(&native).unwrap();
    let sip = config.join("pjsip.conf");
    let original = fs::read_to_string(&sip).unwrap();
    let custom = format!("{original}\n[custom]\ntype=aor\nmax_contacts=1\n");
    fs::write(&sip, &custom).unwrap();
    assert!(prepare_existing(&directory).is_err());
    assert!(!native.exists());
    assert_eq!(fs::read_to_string(&sip).unwrap(), custom);
    fs::create_dir(&native).unwrap();
    assert!(prepare_existing(&directory).is_err());
}
