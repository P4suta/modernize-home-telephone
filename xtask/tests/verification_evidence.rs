#[test]
fn verification_receipts_are_bound_to_the_exact_sources_and_reports() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "source").unwrap();
    std::fs::write(root.path().join("report.log"), "passed").unwrap();
    let source = xtask::package::source_files(root.path()).unwrap();
    let report = xtask::bundle::digest(&root.path().join("report.log")).unwrap();
    let receipt = serde_json::to_vec(
        &serde_json::json!({"schema":1,"source":source,"artifacts":{"report.log":report}}),
    )
    .unwrap();
    xtask::check::check_evidence(root.path(), &receipt).unwrap();
    std::fs::write(root.path().join("report.log"), "different run").unwrap();
    assert!(xtask::check::check_evidence(root.path(), &receipt).is_err());
    std::fs::write(root.path().join("report.log"), "passed").unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "different source").unwrap();
    assert!(xtask::check::check_evidence(root.path(), &receipt).is_err());
    assert!(xtask::check::check_evidence(root.path(), b"{}").is_err());
}
