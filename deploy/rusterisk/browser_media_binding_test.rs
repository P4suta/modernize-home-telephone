#[test]
fn browser_media_binding() {
    use std::net::{IpAddr, Ipv4Addr};

    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let lan = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));
    let mut browser = NativeMediaProfile {
        transport: Transport {
            name: "ws".into(),
            protocol: Protocol::Ws,
            bind: SocketAddr::new(local, 8088),
            external_media_address: Some(lan),
            tls: None,
            ca_list_file: None,
        },
        rtp: Rtp {
            start: 10000,
            end: 10099,
            stun: None,
            turn: None,
            turn_username: None,
            turn_password: None,
        },
        codecs: vec![Codec::Pcmu],
        webrtc: true,
        dtmf: Dtmf::Rfc4733,
    };
    assert_eq!(browser.bind_ip(), lan);
    browser.transport.external_media_address = None;
    assert_eq!(browser.bind_ip(), local);
    browser.transport.external_media_address = Some(lan);
    browser.webrtc = false;
    assert_eq!(browser.bind_ip(), local);
}
