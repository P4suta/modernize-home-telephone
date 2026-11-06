#[test]
fn sip_evidence_requires_one_completed_dialog_and_no_failed_calls() {
    let header =
        "CurrentTime;IncomingCall(C);OutgoingCall(C);SuccessfulCall(C);FailedCall(C);CurrentCall;";
    let good = format!("{header}\nstart;0;0;0;0;0;\nend;1;0;1;0;0;\n");
    xtask::sip_fixture::check_report(&good).unwrap();
    for report in [
        String::new(),
        header.into(),
        format!("{header}\nend;0;0;0;0;0;\n"),
        format!("{header}\nend;1;0;0;1;0;\n"),
        format!("{header}\nend;1;0;1;0;1;\n"),
        format!("{header}\nend;2;0;1;1;0;\n"),
        format!("{header}\nend;1;0;1;0;\n"),
        "SuccessfulCall(C);SuccessfulCall(C);\n1;1;\n".into(),
    ] {
        assert!(
            xtask::sip_fixture::check_report(&report).is_err(),
            "{report}"
        );
    }
}
