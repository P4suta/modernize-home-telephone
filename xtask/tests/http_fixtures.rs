mod support;

use std::io::{Cursor, Read};

#[test]
fn fixture_reading_consumes_the_complete_body_across_every_transport_split() {
    let request = b"POST /fixture HTTP/1.1\r\ncontent-length: 16\r\n\r\n{\"enabled\":true}";
    for split in 0..=request.len() {
        let mut reader = Cursor::new(&request[..split]).chain(Cursor::new(&request[split..]));
        assert_eq!(support::read_request(&mut reader), request, "split {split}");
        assert_eq!(reader.read(&mut [0]).unwrap(), 0);
    }
    assert_eq!(
        support::read_request(&mut Cursor::new(b"GET /fixture HTTP/1.1\r\n\r\n")),
        b"GET /fixture HTTP/1.1\r\n\r\n"
    );
    let header = b"POST /fixture HTTP/1.1\r\nContent-Length: 4048\r\n\r\n";
    let mut bounded = header.to_vec();
    bounded.resize(header.len() + 4048, b'x');
    assert_eq!(bounded.len(), 4096);
    assert_eq!(support::read_request(&mut Cursor::new(&bounded)), bounded);
}

#[test]
fn fixture_reading_refuses_incomplete_ambiguous_and_oversized_requests() {
    let mut oversized = b"POST /fixture HTTP/1.1\r\nContent-Length: 4049\r\n\r\n".to_vec();
    oversized.resize(4097, b'x');
    for request in [
        Vec::new(),
        b"POST /fixture HTTP/1.1\r\nContent-Length: 0\r\n".to_vec(),
        b"POST /fixture HTTP/1.1\r\nContent-Length: 5\r\n\r\nabc".to_vec(),
        b"POST /fixture HTTP/1.1\r\nContent-Length: 3\r\nContent-Length: 4\r\n\r\nabcd".to_vec(),
        b"POST /fixture HTTP/1.1\r\nContent-Length: invalid\r\n\r\n".to_vec(),
        format!(
            "POST /fixture HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            usize::MAX
        )
        .into_bytes(),
        b"POST /fixture HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".to_vec(),
        vec![b'x'; 4097],
        oversized,
    ] {
        assert!(
            std::panic::catch_unwind(|| support::read_request(&mut Cursor::new(request))).is_err()
        );
    }
}
