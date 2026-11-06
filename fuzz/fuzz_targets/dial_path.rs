#![no_main]
libfuzzer_sys::fuzz_target!(|bytes: &[u8]| {
    let expected = !bytes.is_empty()
        && bytes.len() <= 32
        && bytes.iter().enumerate().all(|(index, byte)| {
            byte.is_ascii_digit() || (index == 0 && *byte == b'+' && bytes.len() > 1)
        });
    assert_eq!(phone_core::DialNumber::parse(bytes).is_ok(), expected);
    if let Ok(number) = phone_core::DialNumber::parse(bytes) {
        assert_eq!(number.bytes(), bytes);
        assert_eq!(phone_core::DialNumber::parse(number.bytes()), Ok(number));
    }
    let relative = !bytes.is_empty()
        && bytes.len() <= 256
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(byte))
        && bytes
            .split(|byte| *byte == b'/')
            .all(|segment| !segment.is_empty() && segment != b"." && segment != b"..");
    assert_eq!(
        phone_core::artifact::RelativePath::parse(bytes).is_some(),
        relative
    );
    if let Some(path) = phone_core::artifact::RelativePath::parse(bytes) {
        assert_eq!(path.bytes(), bytes);
        assert_eq!(
            phone_core::artifact::RelativePath::parse(path.bytes()),
            Some(path)
        );
    }
});
