use xtask::asterisk::{Configuration, render};

#[test]
fn rusterisk_configuration_uses_explicit_authenticated_transports() {
    let files = xtask::rusterisk::render(&Configuration {
        address: "127.0.0.1".parse().unwrap(),
        ari_password: "privateAriCredential",
        browser_password: "privateBrowserCredential",
        gateway_password: "privateGatewayCredential",
        lab: true,
    })
    .unwrap();
    let sip = &files["pjsip.conf"];
    for (endpoint, transport) in [("fxo", "udp"), ("handset", "udp"), ("browser", "ws")] {
        assert!(sip.contains(&format!(
            "[{endpoint}]\ntype=endpoint\ntransport={transport}\n"
        )));
        assert!(sip.contains(&format!("auth={endpoint}-auth\n")));
        assert!(sip.contains(&format!(
            "[{endpoint}]\ntype=aor\nmax_contacts=1\nremove_existing=yes\n"
        )));
    }
    assert!(!sip.contains("type=global"));
    assert!(!files["rtp.conf"].contains("icesupport="));
    assert!(files["musiconhold.conf"].contains("directory=moh\n"));
}

#[test]
fn rendered_gateway_has_authenticated_ports_and_native_fallback() {
    let mut config = Configuration {
        address: "192.0.2.10".parse().unwrap(),
        ari_password: "privateAriCredential",
        browser_password: "privateBrowserCredential",
        gateway_password: "privateGatewayCredential",
        lab: true,
    };
    let lab = render(&config).unwrap();
    assert!(lab["pjsip.conf"].contains("webrtc=yes"));
    assert!(lab["pjsip.conf"].contains("external_media_address=192.0.2.10"));
    assert!(lab["http.conf"].contains("bindaddr=127.0.0.1"));
    assert!(lab["extensions.conf"].contains("Dial(PJSIP/handset,30)"));
    assert!(lab["extensions.conf"].contains("[home-lab]"));
    assert!(!lab["pjsip.conf"].lines().any(|line| line == "allow=all"));
    config.lab = false;
    let production = render(&config).unwrap();
    assert!(!production["extensions.conf"].contains("[home-lab]"));
    for endpoint in ["fxo", "handset", "browser"] {
        assert!(production["pjsip.conf"].contains(&format!(
            "[{endpoint}]\ntype=endpoint\nidentify_by=auth_username,username\n"
        )));
    }
    assert!(!production["pjsip.conf"].contains("type=identify"));
    assert!(lab["pjsip.conf"].contains("match=127.0.0.1:5070\n"));
    config.ari_password = "secret\n[anonymous]";
    assert!(render(&config).is_err());
}
