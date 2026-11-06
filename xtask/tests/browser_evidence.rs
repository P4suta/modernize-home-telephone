use serde_json::json;
#[test]
fn browser_evidence_refuses_skips_retries_missing_cases_and_empty_or_failed_runs() {
    let valid = json!({"stats":{"expected":1,"unexpected":0,"flaky":0,"skipped":0},"errors":[],"suites":[{"specs":[{"title":"known audio","ok":true,"tests":[{"status":"expected","results":[{"status":"passed"}]}]}],"suites":[]}]});
    xtask::check::check_browser_report(&valid, &["known audio".into()]).unwrap();
    assert!(xtask::check::check_browser_report(&valid, &[]).is_err());
    assert!(xtask::check::check_browser_report(&valid, &["missing case".into()]).is_err());
    for field in ["unexpected", "flaky", "skipped"] {
        let mut changed = valid.clone();
        changed["stats"][field] = json!(1);
        assert!(xtask::check::check_browser_report(&changed, &["known audio".into()]).is_err());
    }
    let mut changed = valid.clone();
    changed["suites"][0]["specs"][0]["tests"][0]["results"] =
        json!([{"status":"passed"},{"status":"passed"}]);
    assert!(xtask::check::check_browser_report(&changed, &["known audio".into()]).is_err());
    assert!(xtask::check::check_browser_report(&json!({}), &["known audio".into()]).is_err());
}
