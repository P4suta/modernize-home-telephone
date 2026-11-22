#[test]
fn authenticated_registration_replaces_only_previous_contacts() -> Result<(), SipError> {
    let mut registrar = Registrar::new("remove_existing=yes\n")?;
    let old: SocketAddr = "127.0.0.1:5100".parse()?;
    let current: SocketAddr = "127.0.0.1:5101".parse()?;
    let first = "Contact: <sip:first@old.invalid>\r\n";
    let second = "Contact: <sip:second@old.invalid>\r\n";
    let replacement = "Contact: <sip:current@new.invalid>\r\n";
    registrar.request(old, "phone", first, Some(10), 0)?;
    registrar.request(old, "phone", second, Some(30), 1)?;
    registrar.request(old, "backup", first, Some(30), 2)?;
    let response = registrar.request(current, "phone", replacement, Some(60), 3)?;
    assert_eq!(
        contacts(&response)?,
        vec![
            ("sip:second@old.invalid".into(), 30),
            ("sip:current@new.invalid".into(), 60)
        ]
    );
    let refreshed = registrar.request(current, "phone", replacement, Some(50), 4)?;
    assert_eq!(
        contacts(&refreshed)?[1],
        ("sip:current@new.invalid".into(), 50)
    );
    let too_many = format!("{first}{second}{replacement}");
    let refused = registrar.request(current, "phone", &too_many, Some(30), 5)?;
    assert_eq!(self::response(&refused)?.status_code.code(), 403);
    let after = registrar.request(current, "phone", "", None, 6)?;
    assert_eq!(contacts(&after)?, contacts(&refreshed)?);
    let other = registrar.request(old, "backup", "", None, 7)?;
    assert_eq!(contacts(&other)?[0].0, "sip:first@old.invalid");
    Ok(())
}
