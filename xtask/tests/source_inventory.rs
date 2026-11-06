#[test]
fn artifact_identity_binds_build_context_rules_and_excludes_generated_coverage() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "workspace source").unwrap();
    std::fs::write(root.path().join(".dockerignore"), "target\n").unwrap();
    let initial = xtask::package::source_identity(root.path()).unwrap();
    std::fs::write(
        root.path().join(".dockerignore"),
        "target\ncrates/phone-core\n",
    )
    .unwrap();
    assert_ne!(
        xtask::package::source_identity(root.path()).unwrap(),
        initial
    );
    let changed = xtask::package::source_identity(root.path()).unwrap();
    std::fs::create_dir_all(root.path().join("web/coverage")).unwrap();
    std::fs::write(
        root.path().join("web/coverage/report.json"),
        "generated report",
    )
    .unwrap();
    assert_eq!(
        xtask::package::source_identity(root.path()).unwrap(),
        changed
    );
}

#[test]
fn artifact_identity_binds_git_byte_preservation_rules() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "workspace source").unwrap();
    let initial = xtask::package::source_identity(root.path()).unwrap();
    std::fs::write(root.path().join(".gitattributes"), "upstream/** -text\n").unwrap();
    assert_ne!(
        xtask::package::source_identity(root.path()).unwrap(),
        initial
    );
}

#[test]
fn git_index_round_trip_preserves_upstream_bytes() {
    use std::process::Command;

    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let attributes = std::fs::read(project.join(".gitattributes")).unwrap_or_default();
    std::fs::write(root.join(".gitattributes"), attributes).unwrap();
    let name =
        ".agents/skills/grandstream-ht813/references/upstream/ht813-configuration-template.txt";
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = b"# Preserved vendor fixture\r\nP123 = 456\r\n";
    std::fs::write(&path, original).unwrap();
    for arguments in [vec!["init", "--quiet"], vec!["add", ".gitattributes", name]] {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let prefix = format!("--prefix={}/restored/", root.display());
    let output = Command::new("git")
        .args(["checkout-index", "--all", &prefix])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(root.join("restored").join(name)).unwrap(),
        original
    );
}

#[test]
fn public_source_identity_excludes_private_vendor_originals() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "workspace source").unwrap();
    let initial = xtask::package::source_identity(root.path()).unwrap();
    let archive = root
        .path()
        .join(".agents/skills/grandstream-ht813/references/upstream");
    std::fs::create_dir_all(&archive).unwrap();
    std::fs::write(archive.join("manual.pdf"), "private publisher original").unwrap();
    assert_eq!(
        xtask::package::source_identity(root.path()).unwrap(),
        initial
    );
    let authored = archive.parent().unwrap().join("source-catalog.md");
    std::fs::write(authored, "Authored source catalog.\n").unwrap();
    assert_ne!(
        xtask::package::source_identity(root.path()).unwrap(),
        initial
    );
}

#[test]
fn public_policies_and_dependency_configuration_bind_artifact_identity() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "workspace source").unwrap();
    for name in [
        "CONTRIBUTING.md",
        "SECURITY.md",
        "CODE_OF_CONDUCT.md",
        "THIRD_PARTY_NOTICES.md",
        "renovate.json",
    ] {
        let before = xtask::package::source_identity(root.path()).unwrap();
        std::fs::write(root.path().join(name), name).unwrap();
        assert_ne!(
            xtask::package::source_identity(root.path()).unwrap(),
            before
        );
    }
}

#[test]
fn a_similar_directory_name_keeps_authored_material_in_the_source_inventory() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "workspace source").unwrap();
    let retained = root
        .path()
        .join(".agents/skills/grandstream-ht813/references/upstream-notes");
    std::fs::create_dir_all(&retained).unwrap();
    let before = xtask::package::source_identity(root.path()).unwrap();
    std::fs::write(retained.join("authored.md"), "Authored notes.\n").unwrap();
    assert_ne!(
        xtask::package::source_identity(root.path()).unwrap(),
        before
    );
}

#[cfg(unix)]
#[test]
fn public_source_inventory_refuses_symlinks_and_special_files() {
    use std::os::unix::{fs::symlink, net::UnixListener};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "workspace source").unwrap();
    std::fs::create_dir(root.path().join("docs")).unwrap();
    symlink(
        root.path().join("Cargo.toml"),
        root.path().join("docs/link"),
    )
    .unwrap();
    assert!(xtask::package::source_files(root.path()).is_err());
    std::fs::remove_file(root.path().join("docs/link")).unwrap();
    let _socket = UnixListener::bind(root.path().join("docs/socket")).unwrap();
    assert!(xtask::package::source_files(root.path()).is_err());
}

#[test]
fn publication_archive_excludes_vendor_originals_and_retains_authored_sources() {
    use std::process::Command;

    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    std::fs::copy(
        project.join(".gitattributes"),
        root.path().join(".gitattributes"),
    )
    .unwrap();
    let public = [
        "Cargo.toml",
        "README.md",
        "LICENSE-MIT",
        "CONTRIBUTING.md",
        ".github/workflows/ci.yml",
        ".agents/skills/grandstream-ht813/SKILL.md",
        ".agents/skills/grandstream-ht813/references/sources.sha256",
        "fuzz/corpus/state_sequences/seed-0",
    ];
    let private = [
        ".agents/skills/grandstream-ht813/references/upstream/manual.pdf",
        ".cache/check.log",
        ".local/telephone/config/secrets.env",
        ".env",
        "call.sqlite3",
        "recordings/call.wav",
        "recordings/call.ulaw",
    ];
    for name in public.iter().chain(&private) {
        let path = root.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, name).unwrap();
    }
    let run = |arguments: &[&str]| {
        let result = Command::new("git")
            .args(arguments)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    };
    run(&["init", "--quiet"]);
    run(&["add", "--force", "."]);
    let tree = String::from_utf8(run(&["write-tree"])).unwrap();
    let archive = root.path().join("public.tar");
    run(&[
        "archive",
        "--format=tar",
        "--output",
        archive.to_str().unwrap(),
        tree.trim(),
    ]);
    let result = Command::new("tar")
        .args(["-tf"])
        .arg(archive)
        .output()
        .unwrap();
    assert!(result.status.success());
    let listing = String::from_utf8(result.stdout).unwrap();
    let files: std::collections::BTreeSet<_> = listing.lines().collect();
    for name in public {
        assert!(files.contains(name), "missing public file: {name}");
    }
    for name in private {
        assert!(!files.contains(name), "private file in publication: {name}");
    }
    assert!(!files.iter().any(|name| name.starts_with(".git/")));
}
