#[test]
fn rootless_units_preserve_the_selected_engine_without_a_system_docker_dependency() {
    let root = tempfile::tempdir().unwrap();
    let installation = root.path().join("telephone");
    std::fs::create_dir(&installation).unwrap();
    let unit = root.path().join("homephone.service");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "unit",
            "--engine",
            "podman",
            "--user",
            "apps",
            "--binary",
            "/opt/homephone",
        ])
        .arg("--directory")
        .arg(&installation)
        .arg("--output")
        .arg(&unit)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let unit = std::fs::read_to_string(unit).unwrap();
    assert!(unit.contains("Environment=HOMEPHONE_CONTAINER_ENGINE=podman"));
    assert!(unit.contains("WantedBy=default.target"));
    assert!(!unit.contains("docker.service"));
    assert!(!unit.contains("\nUser="));
    assert!(unit.contains(" boot --directory "));
    assert!(unit.contains(" stop --directory "));
}

#[test]
fn installation_operations_run_only_through_the_selected_engine() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = vec![root.join("manager.rs"), root.join("operations.rs")];
    for entry in std::fs::read_dir(root.join("manager")).unwrap() {
        sources.push(entry.unwrap().path());
    }
    for source in sources {
        let text = std::fs::read_to_string(&source).unwrap();
        assert!(
            !text.contains("Command::new(\"docker\")")
                && !text.contains("Command::new(\"podman\")"),
            "{} bypasses the selected container engine",
            source.display()
        );
    }
}
