use phone_core::artifact::RelativePath;

#[test]
fn parsing_matches_the_component_specification_for_every_short_path() {
    let alphabet = b"a./_:\\";
    for length in 0_u32..=6 {
        for mut encoded in 0..alphabet.len().pow(length) {
            let mut bytes = vec![0; length as usize];
            for byte in &mut bytes {
                *byte = alphabet[encoded % alphabet.len()];
                encoded /= alphabet.len();
            }
            let expected = !bytes.is_empty()
                && bytes
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-/".contains(byte))
                && bytes
                    .split(|byte| *byte == b'/')
                    .all(|part| !part.is_empty() && part != b"." && part != b"..");
            assert_eq!(RelativePath::parse(&bytes).is_some(), expected, "{bytes:?}");
        }
    }
}

#[test]
fn artifact_names_are_bounded_relative_paths_without_traversal_or_platform_prefixes() {
    for value in [
        "phone.sqlite",
        "config/keys/vapid.pem",
        "nested/.hidden-file",
        "Cargo.lock",
    ] {
        assert_eq!(
            RelativePath::parse(value.as_bytes()).unwrap().bytes(),
            value.as_bytes()
        );
    }
    for value in [
        "",
        "/absolute",
        "../parent",
        "nested/../file",
        "nested/./file",
        "a//b",
        "trailing/",
        "C:/drive",
        "a\\b",
        "data\nfile",
        ".",
        "..",
    ] {
        assert!(RelativePath::parse(value.as_bytes()).is_none(), "{value:?}");
    }
    assert!(RelativePath::parse(&[b'a'; 256]).is_some());
    assert!(RelativePath::parse(&[b'a'; 257]).is_none());
    for byte in 128..=255 {
        assert!(RelativePath::parse(&[b'a', byte]).is_none());
    }
}
