#[test]
fn nas_reports_refuse_missing_repeated_skipped_and_retried_cases() {
    let cases = vec!["capture".to_owned(), "playback".to_owned()];
    let valid = "NAS PASS capture\nNAS PASS playback\nNAS COMPLETE 2\n";
    xtask::check::check_nas_report(valid, &cases).unwrap();
    for invalid in [
        "",
        "NAS COMPLETE 2\n",
        "NAS PASS capture\nNAS COMPLETE 1\n",
        "NAS PASS capture\nNAS PASS capture\nNAS PASS playback\nNAS COMPLETE 2\n",
        "NAS PASS capture\nNAS SKIP playback\nNAS COMPLETE 2\n",
        "NAS PASS capture\nNAS RETRY playback\nNAS PASS playback\nNAS COMPLETE 2\n",
        "NAS PASS capture\nNAS PASS playback\nNAS COMPLETE 2\nNAS COMPLETE 2\n",
        "NAS PASS capture\nNAS PASS playback\nNAS PASS unknown\nNAS COMPLETE 2\n",
    ] {
        assert!(
            xtask::check::check_nas_report(invalid, &cases).is_err(),
            "{invalid}"
        );
    }
    assert!(xtask::check::check_nas_report(valid, &[]).is_err());
    assert!(xtask::check::check_nas_report(valid, &["capture".into(), "capture".into()]).is_err());
}
