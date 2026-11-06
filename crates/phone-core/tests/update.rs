use phone_core::{
    Refusal,
    update::{Action, Phase},
};

#[test]
fn replacement_requires_quiescence_and_observed_candidate_health() {
    assert_eq!(
        Phase::ServingBusy.apply(Action::Quiesce),
        Err(Refusal::Busy)
    );
    for phase in [
        Phase::ServingIdle,
        Phase::Quiesced,
        Phase::Stopped,
        Phase::CandidateRunning,
    ] {
        assert!(phase.apply(Action::Commit).is_err());
    }
    let phase = Phase::ServingIdle
        .apply(Action::Quiesce)
        .unwrap()
        .apply(Action::Stop)
        .unwrap()
        .apply(Action::StartCandidate)
        .unwrap();
    assert_eq!(phase, Phase::CandidateRunning);
    assert_eq!(
        phase
            .apply(Action::HealthSucceeded)
            .unwrap()
            .apply(Action::Commit)
            .unwrap(),
        Phase::Committed
    );
    assert_eq!(phase.apply(Action::Rollback).unwrap(), Phase::RolledBack);
    assert!(Phase::Committed.apply(Action::Commit).is_err());
}
