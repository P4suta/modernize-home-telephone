#[test]
fn plugin_dependencies_cannot_reach_the_host_or_another_plugin() {
    let clean = serde_json::json!({"packages":[
        {"name":"phone-core","dependencies":[]},
        {"name":"phone-plugin-ai","dependencies":[{"name":"phone-core"}]},
        {"name":"phone-plugin-ntfy","dependencies":[{"name":"phone-core"}]},
        {"name":"phone-plugin-nas","dependencies":[]},
        {"name":"phone-plugin-phonebook","dependencies":[{"name":"phone-core"}]}
    ]});
    xtask::plugins::validate_dependencies(&clean).unwrap();
    for (package, dependency) in [
        (0, "phone-server"),
        (0, "phone-plugin-ai"),
        (1, "phone-server"),
        (1, "xtask"),
        (1, "phone-plugin-ntfy"),
        (2, "phone-plugin-ai"),
        (3, "phone-server"),
        (3, "xtask"),
        (3, "phone-plugin-ntfy"),
        (4, "phone-server"),
        (4, "xtask"),
        (4, "phone-plugin-ntfy"),
    ] {
        let mut bad = clean.clone();
        bad["packages"][package]["dependencies"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"name":dependency}));
        assert!(xtask::plugins::validate_dependencies(&bad).is_err());
    }
    assert!(xtask::plugins::validate_dependencies(&serde_json::json!({"packages":[]})).is_err());
}

#[test]
fn the_host_lint_matrix_covers_every_proper_plugin_subset() {
    let sets = xtask::plugins::host_feature_sets();
    let unique: std::collections::BTreeSet<_> = sets.iter().cloned().collect();
    assert_eq!((sets.len(), unique.len()), (15, 15));
    for expected in [
        "",
        "plugin-phonebook",
        "plugin-ntfy,plugin-phonebook",
        "plugin-ai,plugin-ntfy,plugin-nas",
    ] {
        assert!(unique.contains(expected), "{expected}");
    }
    assert!(!unique.contains("plugin-ai,plugin-ntfy,plugin-nas,plugin-phonebook"));
}
