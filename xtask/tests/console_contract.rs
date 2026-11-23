use xtask::console::parameters;

#[test]
fn nested_fault_tests_cannot_bypass_required_enforcement_records() {
    let root = tempfile::tempdir().unwrap();
    let source = "crates/phone-server/tests/nested_faults.rs";
    std::fs::create_dir_all(root.path().join("crates/phone-server/tests")).unwrap();
    std::fs::write(
        root.path().join(source),
        "#[test] fn baseline() {} mod nested { #[test] fn interrupted_write() {} }",
    )
    .unwrap();
    let baseline = format!(
        "[[cases]]\nsource = '{source}'\ntest = 'baseline'\ninvariant = 'A complete baseline is required.'\nprevious_gap = 'No baseline existed.'\nenforcement = 'A native test rejects an invalid baseline.'\n"
    );
    assert!(xtask::fault_catalog::check(root.path(), &baseline).is_err());
    let complete = format!(
        "{baseline}\n[[cases]]\nsource = '{source}'\ntest = 'nested::interrupted_write'\ninvariant = 'Interrupted writes retain their guard.'\nprevious_gap = 'Nested fault tests were omitted.'\nenforcement = 'The parser visits nested test modules.'\n"
    );
    xtask::fault_catalog::check(root.path(), &complete).unwrap();
}

#[test]
fn parameter_coverage_refuses_omissions_duplicate_identifiers_and_changed_source_bindings() {
    let root = tempfile::tempdir().unwrap();
    let bytes = include_bytes!("../../console/src/lib/parameters.json");
    xtask::console::check_references(root.path(), bytes).unwrap();
    let values: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let mut omitted = values.as_array().unwrap().clone();
    omitted.pop();
    assert!(
        xtask::console::check_references(root.path(), &serde_json::to_vec(&omitted).unwrap())
            .is_err()
    );
    let mut renamed = values.clone();
    renamed[0]["id"] = serde_json::json!("P99999999");
    assert!(
        xtask::console::check_references(root.path(), &serde_json::to_vec(&renamed).unwrap())
            .is_err()
    );
    let mut duplicated = values.clone();
    duplicated[0]["id"] = values[1]["id"].clone();
    assert!(
        xtask::console::check_references(root.path(), &serde_json::to_vec(&duplicated).unwrap())
            .is_err()
    );
}

#[test]
fn the_console_mutation_campaign_is_a_selectable_verification() {
    use clap::ValueEnum as _;
    let check = xtask::assurance::Check::from_str("console-mutants", false).unwrap();
    assert_eq!(
        serde_json::to_value(check).unwrap(),
        serde_json::json!("console_mutants")
    );
}

#[test]
fn published_definitions_refuse_manufacturer_parameter_names() {
    let root = tempfile::tempdir().unwrap();
    let bytes = include_bytes!("../../console/src/lib/parameters.json");
    let values: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assert!(
        values
            .as_array()
            .unwrap()
            .iter()
            .all(|value| value.get("alias").is_none())
    );
    let mut named = values.clone();
    named[0]["alias"] = serde_json::json!("Admin password for web interface");
    assert!(
        xtask::console::check_references(root.path(), &serde_json::to_vec(&named).unwrap())
            .is_err()
    );
}

#[test]
fn parameter_inventory_preserves_domains_and_rejects_ambiguous_definitions() {
    let source = "## HT813 firmware version 1.0.17.3\n# Local RTP port\n# Number: 1024 to 65535\nP39 = 5004\n# Codec\n# Number: 0, 8\nP57 = 0\n";
    let parsed = parameters(source).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id, "P39");
    assert_eq!(parsed[0].domain, "1024 to 65535");
    assert_eq!(parsed[0].template_value, "5004");
    assert!(parameters(&source.replace("P57", "P39")).is_err());
    assert!(parameters(&source.replace("1.0.17.3", "1.0.13.3")).is_err());
    assert!(parameters("## HT813 firmware version 1.0.17.3\n").is_err());
    let unqualified = parameters(&source.replace("P39", "39")).unwrap();
    assert_eq!(unqualified[0].id, "39");
    let section = source.replace(
        "# Local RTP port",
        "# Firmware Upgrade #\n\n# Local RTP port",
    );
    assert_eq!(parameters(&section).unwrap()[0].title, "Local RTP port");
    let inactive = format!("{source}# PPPoE password\n#P83 =\n");
    assert_eq!(parameters(&inactive).unwrap().len(), 3);
}
