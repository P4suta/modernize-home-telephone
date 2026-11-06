#[test]
fn lab_automation_refuses_production_missing_and_non_mock_configuration() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("config")).unwrap();
    for source in [
        "mode='production'\n[ai]\nprovider='mock'",
        "mode='lab'\n[ai]\nprovider='openai'",
        "[ai]\nprovider='mock'",
        "mode='lab'",
    ] {
        std::fs::write(root.path().join("config/homephone.toml"), source).unwrap();
        assert!(
            xtask::manager::require_lab(root.path()).is_err(),
            "{source}"
        );
    }
    std::fs::write(
        root.path().join("config/homephone.toml"),
        "mode='lab'\n[ai]\nprovider='mock'",
    )
    .unwrap();
    xtask::manager::require_lab(root.path()).unwrap();
}
