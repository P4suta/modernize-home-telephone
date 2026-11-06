#[test]
fn provisioning_changes_only_firmware_qualified_routing_fields() {
    let input = xtask::provisioning::Input {
        firmware: "1.0.17.3",
        mac: "000b82123456",
        server: "192.0.2.10".parse().unwrap(),
        password: "private-gateway-secret",
    };
    let xml = xtask::provisioning::render(&input).unwrap();
    assert!(xml.contains("<P747>192.0.2.10</P747>"));
    assert!(xml.contains("<P3305>home</P3305>"));
    assert!(xml.contains("<P842>0</P842>"));
    assert!(xml.contains("<P2346>1</P2346>"));
    for field in ["<P2>", "<P64>", "<P186>", "<P798>"] {
        assert!(!xml.contains(field));
    }
    assert!(
        xtask::provisioning::render(&xtask::provisioning::Input {
            firmware: "1.0.19.6",
            ..input
        })
        .is_err()
    );
    assert!(
        xtask::provisioning::render(&xtask::provisioning::Input {
            mac: "../../secret",
            ..input
        })
        .is_err()
    );
}

#[cfg(feature = "vendor-archive")]
#[test]
fn archived_parameters_belong_to_the_qualified_template() {
    let input = xtask::provisioning::Input {
        firmware: "1.0.17.3",
        mac: "000b82123456",
        server: "192.0.2.10".parse().unwrap(),
        password: "private-gateway-secret",
    };
    let template = std::fs::read_to_string(
        "../.agents/skills/grandstream-ht813/references/upstream/ht813-configuration-template.txt",
    )
    .unwrap();
    for parameter in xtask::provisioning::parameters(&input).unwrap().keys() {
        assert!(
            template.lines().any(
                |line| line.trim_start().starts_with(&format!("{parameter}="))
                    || line.trim_start().starts_with(&format!("{parameter} ="))
            ),
            "{parameter}"
        );
    }
}
