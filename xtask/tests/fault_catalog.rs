#[test]
fn repository_fault_inventory_is_complete() {
    let root = std::path::Path::new("..");
    xtask::fault_catalog::check(
        root,
        &std::fs::read_to_string(root.join("verification/faults.toml")).unwrap(),
    )
    .unwrap();
}

#[test]
fn every_fault_test_requires_a_unique_cause_gap_and_executable_enforcement_record() {
    let root = tempfile::tempdir().unwrap();
    let tests = root.path().join("crates/phone-server/tests");
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::write(
        tests.join("storage_faults.rs"),
        "#[tokio::test] async fn broken_write() { assert!(true); }",
    )
    .unwrap();
    let valid = "[[cases]]\nsource = 'crates/phone-server/tests/storage_faults.rs'\ntest = 'broken_write'\ninvariant = 'Uncommitted work does not escape.'\nprevious_gap = 'The boundary never failed.'\nenforcement = 'A real write failure must preserve the prior state.'\n";
    xtask::fault_catalog::check(root.path(), valid).unwrap();
    assert!(xtask::fault_catalog::check(root.path(), "").is_err());
    assert!(
        xtask::fault_catalog::check(root.path(), &valid.replace("broken_write", "missing_test"))
            .is_err()
    );
    assert!(
        xtask::fault_catalog::check(
            root.path(),
            &valid.replace("A real write failure must preserve the prior state.", "")
        )
        .is_err()
    );
    assert!(xtask::fault_catalog::check(root.path(), &format!("{valid}{valid}")).is_err());
    std::fs::write(
        tests.join("storage_faults.rs"),
        "#[tokio::test] async fn broken_write() {}\n#[test] fn interrupted_write() {}\n",
    )
    .unwrap();
    assert!(xtask::fault_catalog::check(root.path(), valid).is_err());
}
