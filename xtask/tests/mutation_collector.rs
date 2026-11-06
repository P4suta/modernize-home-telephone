#![cfg(unix)]

use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};

const PRIVATE_EXECUTABLE_MODE: u32 = 0o700;

fn observe(missing: &str) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    for name in ["bin", "verification", "xtask/src", "crates/phone-core/src"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    for (name, content) in [
        ("Cargo.toml", "[workspace]\n"),
        ("Cargo.lock", "version = 4\n"),
        ("xtask/src/fixture.rs", "fixture\n"),
        ("crates/phone-core/src/lib.rs", "fixture\n"),
        ("verification/Dockerfile", "fixture image\n"),
    ] {
        std::fs::write(root.join(name), content).unwrap();
    }
    let outcomes: Vec<_> = [
        ("xtask/src/assurance.rs", "image"),
        ("xtask/src/assurance.rs", "image"),
        ("xtask/src/assurance.rs", "verify"),
        ("xtask/src/assurance.rs", "restore_verification_ownership"),
        ("xtask/src/assurance.rs", "inside"),
        ("xtask/src/check.rs", "browser"),
    ]
    .into_iter()
    .filter(|(_, function)| *function != missing)
    .map(|(file, function)| {
        serde_json::json!({
            "scenario": {"Mutant": {"file": file, "function": {"function_name": function}}},
            "summary": "CaughtMutant"
        })
    })
    .collect();
    let report = root.join("provider.json");
    std::fs::write(
        &report,
        serde_json::to_vec(&serde_json::json!({
            "total_mutants": outcomes.len(), "caught": outcomes.len(),
            "missed": 0, "timeout": 0, "unviable": 0, "outcomes": outcomes
        }))
        .unwrap(),
    )
    .unwrap();
    let executable = root.join("bin/cargo");
    std::fs::write(
        &executable,
        r#"#!/bin/sh
while [ "$#" -gt 0 ]; do
  if [ "$1" = --output ]; then
    shift
    mkdir -p "$1/mutants.out" || exit 71
    cp "$HOMEPHONE_TEST_REPORT" "$1/mutants.out/outcomes.json" || exit 72
    exit 0
  fi
  shift
done
exit 73
"#,
    )
    .unwrap();
    std::fs::set_permissions(
        &executable,
        std::fs::Permissions::from_mode(PRIVATE_EXECUTABLE_MODE),
    )
    .unwrap();
    let mut paths = vec![root.join("bin")];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "mutation_collector_child"])
        .current_dir(root)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("HOMEPHONE_TEST_REPORT", &report)
        .env("HOMEPHONE_MISSING_MUTATION", missing)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}; {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn mutation_collector_child() {
    let Ok(missing) = std::env::var("HOMEPHONE_MISSING_MUTATION") else {
        return;
    };
    let error = xtask::assurance::inside(xtask::assurance::Check::Mutants)
        .expect_err("an incomplete adapter campaign produced successful evidence");
    let cause = format!("{error:#}");
    assert!(cause.contains(&missing), "{cause}");
    assert!(!Path::new(".cache/assurance/mutants.json").exists());
}

#[test]
fn mutation_collection_refuses_missing_adapter_results() {
    for missing in ["browser", "inside"] {
        observe(missing);
    }
}
