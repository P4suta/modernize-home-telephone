mod support;

use std::{io::Write, net::TcpListener};
use xtask::admin_http::{Client, Operation};

#[test]
fn administration_uses_only_loopback_and_authenticates_without_a_browser_origin() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let request = support::read_request(&mut socket);
        let end = request
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let text = std::str::from_utf8(&request[..end]).unwrap();
        assert!(text.starts_with("POST /api/admin/maintenance HTTP/1.1\r\n"));
        assert!(text.contains("Authorization: Bearer fixture-token"));
        assert!(!text.contains("Origin:"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&request[end..]).unwrap(),
            serde_json::json!({"enabled":true})
        );
        for byte in b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n" {
            socket.write_all(&[*byte]).unwrap();
        }
    });
    assert_eq!(
        Client {
            address,
            token: "fixture-token".into()
        }
        .request(Operation::Quiesce)
        .unwrap()
        .status,
        204
    );
    server.join().unwrap();
    assert!(
        Client {
            address: "192.0.2.1:8787".parse().unwrap(),
            token: "fixture-token".into()
        }
        .request(Operation::Status)
        .is_err()
    );
    assert!(
        Client {
            address,
            token: "injected\r\nHeader: value".into()
        }
        .request(Operation::Status)
        .is_err()
    );
}
