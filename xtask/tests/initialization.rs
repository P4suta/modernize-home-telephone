use xtask::operations::initialize_with;

#[test]
fn initialization_publishes_complete_configuration_and_cleans_up_failed_key_generation() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("installation");
    assert!(
        initialize_with(
            &target,
            "127.0.0.1".parse().unwrap(),
            "http://localhost:8787".into(),
            None,
            true,
            |_| anyhow::bail!("injected key generation failure")
        )
        .is_err()
    );
    assert!(!target.join("config").exists());
    initialize_with(
        &target,
        "127.0.0.1".parse().unwrap(),
        "http://localhost:8787".into(),
        None,
        true,
        |keys| {
            for name in ["dtls.key", "dtls.crt", "vapid.pem"] {
                std::fs::write(keys.join(name), "fixture key")?;
            }
            Ok(())
        },
    )
    .unwrap();
    assert!(target.join("config/homephone.toml").is_file());
    assert!(target.join("spool/recording").is_dir());
    assert!(target.join("config/keys/vapid.pem").is_file());
    let secret = std::fs::read(target.join("config/secrets.env")).unwrap();
    assert!(
        initialize_with(
            &target,
            "127.0.0.1".parse().unwrap(),
            "http://localhost:8787".into(),
            None,
            true,
            |_| Ok(())
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(target.join("config/secrets.env")).unwrap(),
        secret
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(target.join("config/secrets.env"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
