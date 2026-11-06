use xtask::asterisk::{Configuration, render};

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
    config.ari_password = "secret\n[anonymous]";
    assert!(render(&config).is_err());
}
