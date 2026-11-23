#[test]
fn plugin_dependencies_cannot_reach_the_host_or_another_plugin() {
    let clean = serde_json::json!({"packages":[
        {"name":"phone-core","dependencies":[]},
        {"name":"phone-plugin-ai","dependencies":[{"name":"phone-core"}]}
    ]});
    xtask::plugins::validate_dependencies(&clean).unwrap();
    for (package, dependency) in [
        (0, "phone-server"),
        (0, "phone-plugin-ai"),
        (1, "phone-server"),
        (1, "xtask"),
        (1, "phone-plugin-ntfy"),
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
