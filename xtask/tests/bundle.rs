use xtask::bundle::{self, Kind};

#[test]
fn corruption_and_traversal_are_refused_before_restoring_any_file() {
    let directory = tempfile::tempdir().unwrap();
    let backup = directory.path().join("backup");
    std::fs::create_dir(&backup).unwrap();
    std::fs::write(backup.join("phone.sqlite"), b"abc").unwrap();
    bundle::seal(&backup, Kind::Backup).unwrap();
    assert_eq!(
        bundle::verify(&backup, Kind::Backup).unwrap().files["phone.sqlite"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let restored = directory.path().join("restored");
    bundle::restore(&backup, &restored, Kind::Backup).unwrap();
    assert_eq!(
        std::fs::read(restored.join("phone.sqlite")).unwrap(),
        b"abc"
    );
    std::fs::write(backup.join("phone.sqlite"), b"changed").unwrap();
    assert!(bundle::verify(&backup, Kind::Backup).is_err());
    let refused = directory.path().join("refused");
    assert!(bundle::restore(&backup, &refused, Kind::Backup).is_err());
    assert!(!refused.exists());
    std::fs::write(
        backup.join("manifest.json"),
        r#"{"schema":1,"format":"homephone-backup","files":{"../outside":"anything"}}"#,
    )
    .unwrap();
    assert!(bundle::restore(&backup, &refused, Kind::Backup).is_err());
    assert!(!refused.exists());
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_a_backup_or_replace_a_restore_destination() {
    let directory = tempfile::tempdir().unwrap();
    let backup = directory.path().join("backup");
    std::fs::create_dir(&backup).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", backup.join("private")).unwrap();
    assert!(bundle::seal(&backup, Kind::Backup).is_err());
}
