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
    directory(bytes);
});

fn complete_national(digits: &[u8]) -> Option<String> {
    let subscriber = digits.strip_prefix(b"0")?;
    ((subscriber.len() == 9 || subscriber.len() == 10)
        && subscriber[0] != b'0'
        && subscriber.iter().all(u8::is_ascii_digit))
    .then(|| String::from_utf8_lossy(digits).into_owned())
}

fn directory(bytes: &[u8]) {
    use phone_core::directory::{
        CallerLabelText, NationalNumber, directory_phone_key, parse_directory_row,
    };
    if let Ok(number) = phone_core::DialNumber::parse(bytes) {
        let domestic = match bytes.strip_prefix(b"+81") {
            Some(rest) => [b"0".as_slice(), rest].concat(),
            None => bytes.to_vec(),
        };
        let national = NationalNumber::from_dial(number);
        assert_eq!(
            national.map(|national| national.to_string()),
            complete_national(&domestic)
        );
        if let Some(national) = national {
            assert_eq!(NationalNumber::from_key(national.key()), Some(national));
        }
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let parts: Vec<&str> = text.split('\u{1e}').collect();
    let expected = match parts.as_slice() {
        [spelling] => complete_national(
            &spelling
                .bytes()
                .filter(u8::is_ascii_digit)
                .collect::<Vec<_>>(),
        ),
        [national, spelling] if !spelling.is_empty() => complete_national(national.as_bytes()),
        _ => None,
    };
    assert_eq!(
        directory_phone_key(text).map(|national| national.to_string()),
        expected
    );
    let fields: Vec<&str> = text.split('\t').collect();
    let category = fields.first().and_then(|field| {
        (!field.is_empty()
            && field.bytes().all(|byte| byte.is_ascii_digit())
            && (*field == "0" || !field.starts_with('0')))
        .then(|| field.parse::<usize>().ok())
        .flatten()
    });
    let row = parse_directory_row(text);
    assert_eq!(
        row.is_ok(),
        fields.len() == 9 && category.is_some() && !fields[2].is_empty() && !fields[4].is_empty()
    );
    if let Ok(row) = row {
        assert_eq!(Some(row.category), category);
        assert_eq!(row.name, fields[2]);
        assert_eq!(row.telephones().count(), fields[4].split('\u{1f}').count());
    }
    assert_eq!(
        CallerLabelText::new(text).is_ok(),
        !text.is_empty() && text.len() <= 512 && !text.chars().any(char::is_control)
    );
}
