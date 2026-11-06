use serde_json::json;
use xtask::assurance::{check_ghost_erasure, check_kani};
#[test]
fn public_parsers_reject_lengths_outside_the_finite_scanner_proof_domain() {
    let artifact = std::fs::read_to_string("../crates/phone-core/src/artifact.rs").unwrap();
    let numbers = std::fs::read_to_string("../crates/phone-core/src/lib.rs").unwrap();
    xtask::assurance::check_length_guards(&artifact, &numbers).unwrap();
    assert!(
        xtask::assurance::check_length_guards(
            &artifact.replace(
                "MAX_RELATIVE_PATH_BYTES: usize = 256",
                "MAX_RELATIVE_PATH_BYTES: usize = 257"
            ),
            &numbers
        )
        .is_err()
    );
    assert!(
        xtask::assurance::check_length_guards(
            &artifact,
            &numbers.replace(
                "MAX_DIAL_NUMBER_BYTES: usize = 32",
                "MAX_DIAL_NUMBER_BYTES: usize = 33"
            )
        )
        .is_err()
    );
}

#[test]
fn proof_inventory_and_assumptions_are_source_bound() {
    let source = std::fs::read_to_string("../crates/phone-core/src/proofs.rs").unwrap();
    let contract: toml::Value =
        toml::from_str(&std::fs::read_to_string("../verification/contracts.toml").unwrap())
            .unwrap();
    let names: Vec<String> = contract["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().into())
        .collect();
    xtask::assurance::check_harnesses(&source, &names).unwrap();
    assert!(
        xtask::assurance::check_harnesses(
            &source.replace(
                "assert!(line.owner().is_none());",
                "assert!(line.owner().is_some());"
            ),
            &names
        )
        .is_err()
    );
    assert!(
        xtask::assurance::check_harnesses(
            &source.replace("kani::assume(length <= 257)", "kani::assume(false)"),
            &names
        )
        .is_err()
    );
    assert!(
        xtask::assurance::check_harnesses(
            &source.replace(
                "let first: u64 = kani::any();",
                "kani::assume(false); let first: u64 = kani::any();"
            ),
            &names
        )
        .is_err()
    );
    assert!(xtask::assurance::check_harnesses(&source, &names[1..]).is_err());
}
#[test]
fn proof_gate_refuses_empty_missing_duplicate_failed_or_unreachable_claims() {
    let valid = json!({"verification_results":{"summary":{"executed":1,"total_harnesses":1,"successful":1,"failed":0,"status":"completed"},"results":[{"harness_id":"proofs::contract","status":"Success","checks":[{"function":"proofs::contract","status":"Success","category":"assertion"},{"function":"proofs::contract","status":"Satisfied","category":"cover"}]}]}});
    check_kani(&valid, &["proofs::contract".into()]).unwrap();
    assert!(check_kani(&json!({}), &[]).is_err());
    assert!(check_kani(&valid, &["proofs::missing".into()]).is_err());
    assert!(
        check_kani(
            &valid,
            &["proofs::contract".into(), "proofs::contract".into()]
        )
        .is_err()
    );
    let mut failed = valid.clone();
    failed["verification_results"]["results"][0]["status"] = json!("Failure");
    assert!(check_kani(&failed, &["proofs::contract".into()]).is_err());
    let mut unreachable = valid.clone();
    unreachable["verification_results"]["results"][0]["checks"][0]["status"] = json!("Unreachable");
    assert!(check_kani(&unreachable, &["proofs::contract".into()]).is_err());
    let mut vacuous = valid.clone();
    vacuous["verification_results"]["results"][0]["checks"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(check_kani(&vacuous, &["proofs::contract".into()]).is_err());
}
#[test]
fn observation_parameters_cannot_affect_production_or_change_the_public_wrapper() {
    let source = std::fs::read_to_string("../crates/phone-core/src/artifact.rs").unwrap();
    check_ghost_erasure(&source).unwrap();
    let changed = source.replace(
        "let mut remaining = bytes.len();",
        "let mut remaining = _proof_position;",
    );
    assert!(check_ghost_erasure(&changed).is_err());
    let changed = source.replace("accepts(bytes, 0)", "true");
    assert!(check_ghost_erasure(&changed).is_err());
    let changed = source.replace(
        "loop_decreases(remaining)",
        "loop_decreases(bytes.len() - remaining)",
    );
    assert!(check_ghost_erasure(&changed).is_err());
}
