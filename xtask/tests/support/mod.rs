use std::io::Read;

pub fn read_request(reader: &mut impl Read) -> Vec<u8> {
    let mut bytes = [0; 4096];
    let mut count = 0;
    let total = loop {
        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut request = httparse::Request::new(&mut headers);
        if let httparse::Status::Complete(end) = request.parse(&bytes[..count]).unwrap() {
            assert!(
                !request
                    .headers
                    .iter()
                    .any(|header| header.name.eq_ignore_ascii_case("transfer-encoding"))
            );
            let lengths: Vec<_> = request
                .headers
                .iter()
                .filter(|header| header.name.eq_ignore_ascii_case("content-length"))
                .collect();
            assert!(lengths.len() <= 1, "ambiguous fixture request length");
            let length = lengths.first().map_or(0, |header| {
                std::str::from_utf8(header.value)
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
            });
            let total = end.checked_add(length).unwrap();
            assert!(total <= bytes.len(), "fixture request exceeded its bound");
            assert!(count <= total, "unexpected bytes after fixture request");
            break total;
        }
        assert!(count < bytes.len(), "fixture headers exceeded their bound");
        let read = reader.read(&mut bytes[count..]).unwrap();
        assert_ne!(read, 0, "fixture request ended before its headers");
        count += read;
    };
    while count < total {
        let read = reader.read(&mut bytes[count..total]).unwrap();
        assert_ne!(read, 0, "fixture request ended before its body");
        count += read;
    }
    bytes[..total].to_vec()
}
