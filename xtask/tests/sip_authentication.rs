use serde_json::json;
use xtask::sip_fixture::{check_channel, check_client_report};

#[test]
fn gateway_observations_require_the_authenticated_endpoint_context_and_original_number() {
    let baseline = json!([{
        "name": "PJSIP/fxo-00000001",
        "caller": {"number": "0312345678"},
        "dialplan": {"context": "home-carrier"}
    }]);
    check_channel(&baseline, "fxo", "home-carrier").unwrap();
    for (pointer, changed) in [
        ("/0/name", "PJSIP/handset-00000001"),
        ("/0/name", "PJSIP/fxo-spoof-00000001"),
        ("/0/caller/number", "fxo"),
        ("/0/dialplan/context", "home-outbound"),
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(pointer).unwrap() = changed.into();
        assert!(check_channel(&value, "fxo", "home-carrier").is_err());
    }
    for value in [json!([]), json!(null), json!([baseline[0], baseline[0]])] {
        assert!(check_channel(&value, "fxo", "home-carrier").is_err());
    }
}

#[test]
fn sip_client_reports_require_one_completed_dialog_without_failure() {
    let header = "IncomingCall(C);OutgoingCall(C);SuccessfulCall(C);FailedCall(C);CurrentCall\n";
    check_client_report(&format!("{header}0;1;1;0;0\n")).unwrap();
    for values in [
        "1;0;1;0;0",
        "0;0;0;0;0",
        "0;1;0;1;0",
        "0;1;1;0;1",
        "0;2;2;0;0",
        "0;1;1;0",
    ] {
        assert!(check_client_report(&format!("{header}{values}\n")).is_err());
    }
}
