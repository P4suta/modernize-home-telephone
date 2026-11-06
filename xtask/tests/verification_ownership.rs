#![cfg(unix)]

use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
use xtask::assurance::{Check, verify};

const PRIVATE_EXECUTABLE_MODE: u32 = 0o700;

fn executable(path: &Path, bytes: &str) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(PRIVATE_EXECUTABLE_MODE),
    )
    .unwrap();
}

fn observe(mode: &str) -> (Output, Vec<Vec<String>>) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for name in ["bin", "verification", "xtask/src", "crates/phone-core/src"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
    std::fs::write(root.join("xtask/src/fixture.rs"), "fixture\n").unwrap();
    std::fs::write(root.join("crates/phone-core/src/lib.rs"), "fixture\n").unwrap();
    std::fs::write(root.join("verification/Dockerfile"), "fixture image\n").unwrap();
    let hash = xtask::bundle::digest(&root.join("verification/Dockerfile")).unwrap();
    let log = root.join("docker.log");
    executable(&root.join("bin/dotfiles"), "#!/bin/sh\nexit 1\n");
    executable(
        &root.join("bin/id"),
        r#"#!/bin/sh
if [ "$HOMEPHONE_OWNERSHIP_MODE" = invalid ]; then printf '%s\n' invalid; exit 0; fi
case "$1" in
  -u) printf '%s\n' 111 ;;
  -g) printf '%s\n' 222 ;;
  *) exit 71 ;;
esac
"#,
    );
    executable(
        &root.join("bin/docker"),
        r#"#!/bin/sh
printf '%s\n' CALL "$@" >> "$HOMEPHONE_DOCKER_LOG"
case "$1" in
  image)
    case "$*" in
      'image inspect homephone-verification:local')
        printf '%s\n' "$HOMEPHONE_IMAGE_INSPECTION" ;;
      'image inspect --format {{.Id}} homephone-verification:local')
        printf 'sha256:%s\n' "$HOMEPHONE_IMAGE_HASH" ;;
      *) exit 71 ;;
    esac ;;
  run)
    ownership=
    for argument in "$@"; do
      if [ "$argument" = chown ]; then ownership=yes; fi
    done
    if [ "$ownership" = yes ]; then
      case "$HOMEPHONE_OWNERSHIP_MODE" in *ownership*) exit 42 ;; esac
    else
      case "$HOMEPHONE_OWNERSHIP_MODE" in *verifier*) exit 43 ;; esac
    fi ;;
  rm)
    case "$HOMEPHONE_OWNERSHIP_MODE" in *cleanup*) exit 44 ;; esac ;;
  *) exit 71 ;;
esac
"#,
    );
    let mut paths = vec![root.join("bin")];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ownership_child"])
        .current_dir(root)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("HOMEPHONE_OWNERSHIP_MODE", mode)
        .env("HOMEPHONE_DOCKER_LOG", &log)
        .env("HOMEPHONE_IMAGE_HASH", "a".repeat(64))
        .env(
            "HOMEPHONE_IMAGE_INSPECTION",
            serde_json::json!([{"Config":{"Labels":{"homephone.recipe":hash}}}]).to_string(),
        )
        .output()
        .unwrap();
    let mut commands: Vec<Vec<String>> = Vec::new();
    for line in std::fs::read_to_string(log).unwrap().lines() {
        if line == "CALL" {
            commands.push(Vec::new());
        } else {
            commands.last_mut().unwrap().push(line.to_owned());
        }
    }
    (output, commands)
}

fn successful(output: &Output) {
    assert!(
        output.status.success(),
        "child assertion failed: {:?}; {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ownership_child() {
    let Ok(mode) = std::env::var("HOMEPHONE_OWNERSHIP_MODE") else {
        return;
    };
    let result = verify(Check::Proof);
    if mode.is_empty() {
        result.unwrap();
        assert!(
            std::fs::read_dir("target/verification-tools")
                .unwrap()
                .any(|entry| entry.unwrap().path().is_dir()),
            "the ownership target must exist before container execution"
        );
    } else {
        let error = format!("{:#}", result.unwrap_err());
        for (condition, expected) in [
            ("verifier", "bounded verification"),
            ("ownership", "ownership"),
            ("cleanup", "cleanup"),
            ("invalid", "identity"),
        ] {
            if mode.contains(condition) {
                assert!(error.contains(expected), "{error}");
            }
        }
    }
}

#[test]
fn verification_restores_private_outputs_to_the_caller_before_cleanup() {
    let (output, commands) = observe("");
    successful(&output);
    let verification = commands.iter().position(|args| args[0] == "run").unwrap();
    let ownership = commands
        .iter()
        .position(|args| args.iter().any(|argument| argument == "chown"))
        .expect("verification outputs never returned to their caller");
    let cleanup = commands.iter().position(|args| args[0] == "rm").unwrap();
    assert!(verification < ownership && ownership < cleanup);
    let args = &commands[ownership];
    for expected in ["111:222", "--read-only", "--recursive", "--no-dereference"] {
        assert!(args.iter().any(|argument| argument == expected), "{args:?}");
    }
    assert!(
        args.windows(2).any(|pair| pair == ["--network", "none"]),
        "{args:?}"
    );
    assert!(args.iter().any(|argument| argument == ".cache/assurance"));
    assert!(args.iter().any(|argument| {
        argument
            .strip_prefix("target/verification-tools/")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
    }));
}

#[test]
fn ownership_failures_and_verifier_failures_preserve_their_causes_and_cleanup() {
    for mode in [
        "verifier",
        "ownership",
        "cleanup",
        "ownership_cleanup",
        "verifier_ownership_cleanup",
        "invalid",
    ] {
        let (output, commands) = observe(mode);
        successful(&output);
        assert!(commands.iter().any(|args| args[0] == "rm"), "{mode}");
    }
}
