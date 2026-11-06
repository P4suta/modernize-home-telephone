use xtask::bundle::{self, Kind};

fn sample(root: &std::path::Path) {
    for name in [
        "homephone",
        "app.tar",
        "gateway.tar",
        "gateway-source/asterisk.tar.gz",
        "source/Cargo.toml",
        "source/Cargo.lock",
        "source/LICENSE-MIT",
        "source/LICENSE-APACHE",
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "fixture").unwrap();
    }
    let source = xtask::package::source_identity(&root.join("source")).unwrap();
    let release = serde_json::json!({"schema":1,"architecture":"arm64","source":source,"app":{"reference":"homephone-app:fixture","fingerprint":"1".repeat(64)},"gateway":{"reference":"homephone-gateway:fixture","fingerprint":"2".repeat(64)}});
    std::fs::write(
        root.join("release.json"),
        serde_json::to_vec(&release).unwrap(),
    )
    .unwrap();
    bundle::seal(root, Kind::Package).unwrap();
}
#[test]
fn packages_require_the_binary_images_and_matching_corresponding_sources_before_docker_effects() {
    let directory = tempfile::tempdir().unwrap();
    sample(directory.path());
    xtask::package::validate(directory.path()).unwrap();
    for name in [
        "homephone",
        "app.tar",
        "gateway.tar",
        "gateway-source/asterisk.tar.gz",
        "source/Cargo.toml",
        "source/Cargo.lock",
        "source/LICENSE-MIT",
        "source/LICENSE-APACHE",
    ] {
        std::fs::remove_file(directory.path().join(name)).unwrap();
        std::fs::remove_file(directory.path().join("manifest.json")).unwrap();
        bundle::seal(directory.path(), Kind::Package).unwrap();
        assert!(
            xtask::package::validate(directory.path()).is_err(),
            "{name}"
        );
        std::fs::write(directory.path().join(name), "fixture").unwrap();
    }
    std::fs::write(
        directory.path().join("source/Cargo.toml"),
        "different source",
    )
    .unwrap();
    std::fs::remove_file(directory.path().join("manifest.json")).unwrap();
    bundle::seal(directory.path(), Kind::Package).unwrap();
    assert!(xtask::package::validate(directory.path()).is_err());
}
