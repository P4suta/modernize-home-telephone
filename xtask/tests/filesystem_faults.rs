#[cfg(unix)]
#[test]
fn atomic_state_creates_its_parent_and_preserves_an_occupied_destination_on_failure() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("nested/assurance/receipt.json");
    xtask::manager::atomic_json(&state, &serde_json::json!({"completed":true})).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&state).unwrap()).unwrap()["completed"],
        true
    );
    let occupied = state.parent().unwrap().join("occupied");
    std::fs::create_dir(&occupied).unwrap();
    std::fs::write(occupied.join("preserved"), b"original").unwrap();
    assert!(xtask::manager::atomic_json(&occupied, &serde_json::json!({"changed":true})).is_err());
    assert_eq!(
        std::fs::read(occupied.join("preserved")).unwrap(),
        b"original"
    );
    assert!(
        std::fs::read_dir(state.parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".partial"))
    );
}

#[test]
fn private_artifacts_preserve_existing_parent_permissions_and_refuse_symlinks() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    xtask::operations::private_directory(root.path()).unwrap();
    assert_eq!(
        std::fs::metadata(root.path()).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let child = root.path().join("owned/child");
    xtask::operations::private_directory(&child).unwrap();
    assert_eq!(
        std::fs::metadata(&child).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let link = root.path().join("link");
    symlink(&child, &link).unwrap();
    assert!(xtask::operations::private_directory(&link).is_err());
}

#[test]
fn deep_empty_directories_and_dangling_restore_links_are_not_accepted_as_bundles() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("data"), "fixture").unwrap();
    let mut nested = source.clone();
    for _ in 0..40 {
        nested = nested.join("a");
        std::fs::create_dir(&nested).unwrap();
    }
    assert!(xtask::bundle::seal(&source, xtask::bundle::Kind::Backup).is_err());
    #[cfg(unix)]
    {
        let clean = root.path().join("clean");
        std::fs::create_dir(&clean).unwrap();
        std::fs::write(clean.join("data"), "fixture").unwrap();
        xtask::bundle::seal(&clean, xtask::bundle::Kind::Backup).unwrap();
        let destination = root.path().join("dangling");
        std::os::unix::fs::symlink(root.path().join("absent"), &destination).unwrap();
        assert!(xtask::bundle::restore(&clean, &destination, xtask::bundle::Kind::Backup).is_err());
    }
}
