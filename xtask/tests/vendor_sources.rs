use std::{path::Path, process::Command};

const REFERENCE: &str = ".agents/skills/grandstream-ht813/references";
const CHILD_FLAG: &str = "HOMEPHONE_SOURCE_CHECK_CHILD";

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let reference = root.path().join(REFERENCE);
    std::fs::create_dir_all(reference.join("upstream")).unwrap();
    let mut checksum = String::new();
    for index in 0..14 {
        let name = format!("upstream/document-{index}.txt");
        let path = reference.join(&name);
        std::fs::write(&path, format!("owned test document {index}\n")).unwrap();
        checksum.push_str(&format!(
            "{}  {name}\n",
            xtask::bundle::digest(&path).unwrap()
        ));
    }
    std::fs::write(reference.join("sources.sha256"), checksum).unwrap();
    root
}

fn check(root: &Path) -> bool {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sources_child"])
        .env(CHILD_FLAG, "1")
        .current_dir(root)
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn sources_child() -> anyhow::Result<()> {
    if std::env::var(CHILD_FLAG).is_ok() {
        xtask::check::sources()?;
    }
    Ok(())
}

#[test]
fn a_public_checkout_validates_the_catalog_without_private_originals() {
    let root = fixture();
    assert!(check(root.path()));
    std::fs::remove_dir_all(root.path().join(REFERENCE).join("upstream")).unwrap();
    assert!(
        check(root.path()),
        "a complete catalog needs no private archive"
    );
}

#[test]
fn partial_or_modified_vendor_archives_are_refused() {
    let root = fixture();
    assert!(check(root.path()));
    for index in 0..14 {
        let path = root
            .path()
            .join(REFERENCE)
            .join(format!("upstream/document-{index}.txt"));
        let original = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(!check(root.path()), "partial archive accepted at {index}");
        std::fs::write(&path, b"modified source").unwrap();
        assert!(!check(root.path()), "modified source accepted at {index}");
        std::fs::write(&path, original).unwrap();
    }
}

#[test]
fn invalid_catalogs_are_refused_even_without_the_private_archive() {
    let root = fixture();
    let checksum = root.path().join(REFERENCE).join("sources.sha256");
    let original = std::fs::read_to_string(&checksum).unwrap();
    std::fs::remove_dir_all(root.path().join(REFERENCE).join("upstream")).unwrap();
    for invalid in [
        String::new(),
        original.lines().take(13).collect::<Vec<_>>().join("\n"),
        std::iter::repeat_n(original.lines().next().unwrap(), 14)
            .collect::<Vec<_>>()
            .join("\n"),
        original.replace("upstream/document-0.txt", "../outside.txt"),
        original.replace("upstream/document-0.txt", "other/document-0.txt"),
        original.replace(&original[..64], "invalid"),
    ] {
        std::fs::write(&checksum, invalid).unwrap();
        assert!(!check(root.path()));
    }
}

#[cfg(unix)]
#[test]
fn vendor_archive_symlinks_cannot_reach_outside_the_reference_directory() {
    use std::os::unix::fs::symlink;

    let root = fixture();
    let archive = root.path().join(REFERENCE).join("upstream");
    let target = archive.join("document-0.txt");
    let outside = root.path().join("outside.txt");
    std::fs::rename(&target, &outside).unwrap();
    symlink(&outside, &target).unwrap();
    assert!(
        !check(root.path()),
        "a source file symlink escaped the reference directory"
    );
    std::fs::remove_file(target).unwrap();
    std::fs::rename(archive, root.path().join("outside-archive")).unwrap();
    symlink(
        root.path().join("outside-archive"),
        root.path().join(REFERENCE).join("upstream"),
    )
    .unwrap();
    assert!(
        !check(root.path()),
        "a directory symlink escaped the reference directory"
    );
}

#[test]
fn the_explicit_source_audit_requires_every_preserved_original() {
    let root = fixture();
    let audit = || {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("sources")
            .current_dir(root.path())
            .output()
            .unwrap()
    };
    assert!(audit().status.success());
    std::fs::remove_dir_all(root.path().join(REFERENCE).join("upstream")).unwrap();
    let absent = audit();
    assert!(!absent.status.success());
    assert!(
        String::from_utf8_lossy(&absent.stderr).contains("preserved vendor archive is required")
    );
}

#[test]
fn source_hashes_require_exactly_64_lowercase_hexadecimal_digits() {
    let root = fixture();
    let checksum = root.path().join(REFERENCE).join("sources.sha256");
    let original = std::fs::read_to_string(&checksum).unwrap();
    let hash = &original[..64];
    std::fs::remove_dir_all(root.path().join(REFERENCE).join("upstream")).unwrap();
    for character in ['0', '9', 'a', 'f'] {
        std::fs::write(
            &checksum,
            original.replacen(hash, &character.to_string().repeat(64), 1),
        )
        .unwrap();
        assert!(check(root.path()), "valid hash digit {character}");
    }
    for invalid in [
        "0".repeat(63),
        "0".repeat(65),
        "g".repeat(64),
        "A".repeat(64),
        "/".repeat(64),
    ] {
        std::fs::write(&checksum, original.replacen(hash, &invalid, 1)).unwrap();
        assert!(!check(root.path()), "invalid hash: {invalid}");
    }
}

#[test]
fn source_members_must_be_regular_files() {
    let root = fixture();
    let member = root.path().join(REFERENCE).join("upstream/document-0.txt");
    std::fs::remove_file(&member).unwrap();
    std::fs::create_dir(&member).unwrap();
    assert!(!check(root.path()));
}

#[cfg(unix)]
#[test]
fn inaccessible_archive_members_are_errors_instead_of_an_absent_archive() {
    use std::os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::CommandExt,
    };

    let root = fixture();
    let executable = root.path().join("source-test");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let archive = root.path().join(REFERENCE).join("upstream");
    for name in [
        ".agents",
        ".agents/skills",
        ".agents/skills/grandstream-ht813",
        REFERENCE,
    ] {
        std::fs::set_permissions(
            root.path().join(name),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    std::fs::set_permissions(&archive, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(
        root.path().join(REFERENCE).join("sources.sha256"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    for entry in std::fs::read_dir(&archive).unwrap() {
        std::fs::set_permissions(
            entry.unwrap().path(),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
    let mut command = Command::new(executable);
    command
        .args(["--exact", "sources_child"])
        .env(CHILD_FLAG, "1")
        .current_dir(root.path());
    if std::fs::metadata(root.path()).unwrap().uid() == 0 {
        command.gid(65534).uid(65534);
    }
    let readable = command.output().unwrap();
    assert!(
        readable.status.success(),
        "readable control failed: {}",
        String::from_utf8_lossy(&readable.stdout)
    );
    std::fs::set_permissions(&archive, std::fs::Permissions::from_mode(0o0)).unwrap();
    let result = command.output();
    std::fs::set_permissions(archive, std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = result.unwrap();
    assert!(!result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("Permission denied"), "{output}");
}
