use std::{
    io::{Read, Write},
    net::TcpListener,
};
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
        let mut request = vec![];
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let text = String::from_utf8(request).unwrap();
        assert!(text.starts_with("POST /api/admin/maintenance HTTP/1.1\r\n"));
        assert!(text.contains("Authorization: Bearer fixture-token"));
        assert!(!text.contains("Origin:"));
        let length: usize = text
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        let mut body = vec![0; length];
        socket.read_exact(&mut body).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
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
