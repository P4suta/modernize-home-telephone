use phone_core::console::{Mode, Phase, Refusal, Revision, SubmissionGuard};

fn revision(value: u64) -> Revision {
    Revision::new(value).unwrap()
}

#[test]
fn a_reviewed_change_preserves_save_or_apply_intent() {
    for mode in [Mode::Save, Mode::SaveAndApply] {
        let guard = SubmissionGuard::new(revision(7));
        let plan = guard.plan(revision(7), 2, mode).unwrap();
        assert_eq!(plan.mode(), mode);
        assert_eq!(plan.guard().phase(), Phase::InFlight);
        assert_eq!(plan.guard().revision(), revision(7));
    }
}

#[test]
fn stale_or_empty_drafts_cannot_begin_a_device_write() {
    let guard = SubmissionGuard::new(revision(7));
    for value in [1, 6, 8, u64::MAX] {
        assert_eq!(
            guard.plan(revision(value), 1, Mode::Save),
            Err(Refusal::StaleRevision)
        );
    }
    assert_eq!(
        guard.plan(revision(7), 0, Mode::SaveAndApply),
        Err(Refusal::NoChanges)
    );
}

#[test]
fn an_in_flight_write_blocks_submissions_and_refreshes() {
    let writing = SubmissionGuard::new(revision(7))
        .plan(revision(7), 1, Mode::Save)
        .unwrap()
        .guard();
    assert_eq!(
        writing.plan(revision(7), 1, Mode::SaveAndApply),
        Err(Refusal::Busy)
    );
    assert_eq!(writing.refreshed(false), Err(Refusal::Busy));
    assert_eq!(writing.refreshed(true), Err(Refusal::Busy));
}

#[test]
fn every_submitted_write_requires_readback_before_another_write() {
    let pending = SubmissionGuard::new(revision(7))
        .plan(revision(7), 1, Mode::SaveAndApply)
        .unwrap()
        .guard()
        .submitted()
        .unwrap();
    assert_eq!(pending.phase(), Phase::ReconciliationRequired);
    assert_eq!(
        pending.plan(revision(7), 1, Mode::Save),
        Err(Refusal::ReconciliationRequired)
    );
    let ready = pending.refreshed(false).unwrap();
    assert_eq!(ready.phase(), Phase::Ready);
    assert_eq!(ready.revision(), revision(8));
    assert_eq!(
        ready.plan(revision(7), 1, Mode::Save),
        Err(Refusal::StaleRevision)
    );
    assert!(ready.plan(revision(8), 1, Mode::Save).is_ok());
}

#[test]
fn unchanged_background_reads_preserve_a_draft_revision() {
    let guard = SubmissionGuard::new(revision(7));
    assert_eq!(guard.refreshed(false), Ok(guard));
    assert_eq!(guard.refreshed(true).unwrap().revision(), revision(8));
    assert_eq!(guard.submitted(), Err(Refusal::InvalidTransition));
}

#[test]
fn revisions_reject_zero_and_refuse_exhaustion_before_a_write() {
    assert_eq!(Revision::new(0), Err(Refusal::InvalidRevision));
    let guard = SubmissionGuard::new(revision(u64::MAX));
    assert_eq!(
        guard.plan(revision(u64::MAX), 1, Mode::Save),
        Err(Refusal::RevisionExhausted)
    );
    assert_eq!(guard.refreshed(true), Err(Refusal::RevisionExhausted));
}
