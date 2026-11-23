use super::*;

#[kani::proof]
fn incoming_notifications_require_a_fresh_inbound_offer() {
    use crate::notification::{CallNotification, Direction, EventKind, Publication};
    let event: EventKind = kani::any();
    let direction: Direction = kani::any();
    let phase: Phase = kani::any();
    let started_at: u64 = kani::any();
    let created_at: u64 = kani::any();
    let now: u64 = kani::any();
    let input = CallNotification {
        event,
        direction,
        phase,
        started_at,
        created_at,
    };
    let expected = event == EventKind::Incoming
        && direction == Direction::Inbound
        && phase == Phase::Offered
        && started_at <= created_at
        && created_at <= now
        && (now as u128) <= (started_at as u128) + 120;
    assert_eq!(input.incoming(now) == Publication::Publish, expected);
    kani::cover!(input.incoming(now) == Publication::Publish && started_at == 0 && now == 120);
    kani::cover!(input.incoming(now) == Publication::Publish && started_at == u64::MAX);
    kani::cover!(input.incoming(now) == Publication::Skip && now < started_at);
}

#[kani::proof]
fn console_revisions_preserve_exact_values_and_never_wrap() {
    use crate::console::{Phase, Refusal, Revision, SubmissionGuard};
    let raw: u64 = kani::any();
    let changed: bool = kani::any();
    match Revision::new(raw) {
        Err(error) => {
            assert!(raw == 0 && error == Refusal::InvalidRevision);
        }
        Ok(revision) => {
            assert!(raw != 0 && revision.get() == raw);
            let guard = SubmissionGuard::new(revision);
            assert!(guard.phase() == Phase::Ready && guard.revision() == revision);
            assert!(guard.submitted() == Err(Refusal::InvalidTransition));
            match guard.refreshed(changed) {
                Ok(refreshed) => {
                    assert!(refreshed.phase() == Phase::Ready);
                    if changed {
                        assert!(raw < u64::MAX && refreshed.revision().get() == raw + 1);
                    } else {
                        assert!(refreshed.revision() == revision);
                    }
                }
                Err(error) => {
                    assert!(changed && raw == u64::MAX && error == Refusal::RevisionExhausted)
                }
            }
        }
    }
    kani::cover!(raw == 0);
    kani::cover!(raw == u64::MAX && changed);
    kani::cover!(raw == u64::MAX && !changed);
}

#[kani::proof]
fn console_submissions_require_fresh_review_and_readback() {
    use crate::console::{Mode, Phase, Refusal, Revision, SubmissionGuard};
    let raw: u64 = kani::any();
    let expected_raw: u64 = kani::any();
    let changes: usize = kani::any();
    let apply: bool = kani::any();
    let changed: bool = kani::any();
    let revision = match Revision::new(raw) {
        Ok(value) => value,
        Err(error) => {
            assert!(raw == 0 && error == Refusal::InvalidRevision);
            return;
        }
    };
    let expected = Revision::new(expected_raw).unwrap_or(Revision::new(1).unwrap());
    let mode = if apply {
        Mode::SaveAndApply
    } else {
        Mode::Save
    };
    let guard = SubmissionGuard::new(revision);
    let result = guard.plan(expected, changes, mode);
    if expected != revision {
        assert!(result == Err(Refusal::StaleRevision));
    } else if changes == 0 {
        assert!(result == Err(Refusal::NoChanges));
    } else if raw == u64::MAX {
        assert!(result == Err(Refusal::RevisionExhausted));
    } else {
        assert!(result.is_ok());
        let plan = result.unwrap();
        assert!(plan.mode() == mode && plan.guard().revision() == revision);
        assert!(plan.guard().phase() == Phase::InFlight);
        assert!(plan.guard().plan(expected, changes, mode) == Err(Refusal::Busy));
        assert!(plan.guard().refreshed(changed) == Err(Refusal::Busy));
        let submitted = plan.guard().submitted().unwrap();
        assert!(submitted.phase() == Phase::ReconciliationRequired);
        assert!(submitted.plan(expected, changes, mode) == Err(Refusal::ReconciliationRequired));
        assert!(submitted.submitted() == Err(Refusal::InvalidTransition));
        let refreshed = submitted.refreshed(changed).unwrap();
        assert!(refreshed.phase() == Phase::Ready && refreshed.revision().get() == raw + 1);
    }
    kani::cover!(result.is_ok() && apply);
    kani::cover!(result.is_ok() && !apply);
    kani::cover!(result == Err(Refusal::StaleRevision));
}

#[kani::proof]
fn input_length_guards_cover_the_entire_integer_domain() {
    let length: usize = kani::any();
    let limit: usize = kani::any();
    assert!(bytes::nonempty_within(length, limit) == (length > 0 && length <= limit));
    kani::cover!(length > 257 && bytes::nonempty_within(length, limit));
    kani::cover!(length == usize::MAX && !bytes::nonempty_within(length, 256));
}

#[kani::proof]
fn streamed_byte_limits_refuse_overflow_and_preserve_the_exact_bound() {
    let current: usize = kani::any();
    let incoming: usize = kani::any();
    let limit: usize = kani::any();
    let wide = current as u128 + incoming as u128;
    let result = bytes::bounded_add(current, incoming, limit);
    assert!(result.is_some() == (wide <= limit as u128));
    if let Some(total) = result {
        assert!(total as u128 == wide);
    }
    kani::cover!(current == limit && incoming == 0 && result == Some(limit));
    kani::cover!(current == usize::MAX && incoming > 0 && result.is_none());
}

#[kani::proof]
fn deadlines_preserve_states_and_exact_integer_boundaries() {
    use crate::timers::{Timeout, expired};
    let phase = arbitrary_phase();
    let elapsed: u64 = kani::any();
    let screen: u32 = kani::any();
    let ring: u32 = kani::any();
    let message: u32 = kani::any();
    let outbound: bool = kani::any();
    let result = expired(phase, elapsed, screen, ring, message, outbound);
    assert!(
        (result == Timeout::Ring) == (phase == Phase::Screening && elapsed >= u64::from(screen))
    );
    assert!(
        (result == Timeout::Message)
            == (phase == Phase::Ringing && elapsed >= u64::from(ring) && !outbound)
    );
    assert!(
        (result == Timeout::End)
            == ((phase == Phase::Voicemail && elapsed >= u64::from(message))
                || (phase == Phase::Ringing && elapsed >= u64::from(ring) && outbound)
                || (phase == Phase::Offered && elapsed >= 5))
    );
    if matches!(phase, Phase::Connected | Phase::Held | Phase::Ended) {
        assert!(result == Timeout::None);
    }
    kani::cover!(result == Timeout::Message && elapsed == u64::from(ring));
}
fn arbitrary_phase() -> Phase {
    kani::any()
}

#[kani::proof]
fn every_call_transition_has_its_declared_precondition() {
    let phase = arbitrary_phase();
    let action: Action = kani::any();
    let expected = match action {
        Action::Screen if phase == Phase::Offered => Some(Phase::Screening),
        Action::Ring if matches!(phase, Phase::Offered | Phase::Screening) => Some(Phase::Ringing),
        Action::Connect if matches!(phase, Phase::Offered | Phase::Screening | Phase::Ringing) => {
            Some(Phase::Connected)
        }
        Action::Hold if phase == Phase::Connected => Some(Phase::Held),
        Action::Resume if phase == Phase::Held => Some(Phase::Connected),
        Action::RecordMessage if matches!(phase, Phase::Screening | Phase::Ringing) => {
            Some(Phase::Voicemail)
        }
        Action::End if phase != Phase::Ended => Some(Phase::Ended),
        Action::Speak if matches!(phase, Phase::Screening | Phase::Connected) => Some(phase),
        Action::Dtmf if phase == Phase::Connected => Some(phase),
        _ => None,
    };
    assert!(phase.apply(action) == expected.ok_or(Refusal::InvalidTransition));
    kani::cover!(phase == Phase::Connected && action == Action::Dtmf);
}

#[cfg(feature = "proof-negative-control")]
#[kani::proof]
fn negative_control_rejects_lost_ownership() {
    let owner = CallId::new(1).unwrap();
    let line = Line::default().acquire(owner).unwrap();
    assert!(line.owner().is_none());
}

#[kani::proof]
fn identifiers_preserve_every_nonzero_value_and_refuse_zero() {
    let value: u64 = kani::any();
    match CallId::new(value) {
        Ok(id) => {
            assert!(value != 0);
            assert!(id.get() == value);
        }
        Err(_) => assert!(value == 0),
    }
    kani::cover!(value == 0);
    kani::cover!(value == u64::MAX);
}

#[kani::proof]
fn update_transitions_preserve_the_required_predecessors() {
    use crate::update::{Action, Phase};
    let phase: Phase = kani::any();
    let action: Action = kani::any();
    let expected = match action {
        Action::Quiesce => match phase {
            Phase::ServingBusy => Err(Refusal::Busy),
            Phase::ServingIdle => Ok(Phase::Quiesced),
            _ => Err(Refusal::InvalidTransition),
        },
        Action::Stop => {
            if phase == Phase::Quiesced {
                Ok(Phase::Stopped)
            } else {
                Err(Refusal::InvalidTransition)
            }
        }
        Action::StartCandidate => {
            if phase == Phase::Stopped {
                Ok(Phase::CandidateRunning)
            } else {
                Err(Refusal::InvalidTransition)
            }
        }
        Action::HealthSucceeded => {
            if phase == Phase::CandidateRunning {
                Ok(Phase::CandidateHealthy)
            } else {
                Err(Refusal::InvalidTransition)
            }
        }
        Action::Commit => {
            if phase == Phase::CandidateHealthy {
                Ok(Phase::Committed)
            } else {
                Err(Refusal::InvalidTransition)
            }
        }
        Action::Rollback => {
            if matches!(
                phase,
                Phase::Quiesced
                    | Phase::Stopped
                    | Phase::CandidateRunning
                    | Phase::CandidateHealthy
            ) {
                Ok(Phase::RolledBack)
            } else {
                Err(Refusal::InvalidTransition)
            }
        }
    };
    assert!(phase.apply(action) == expected);
    kani::cover!(expected == Ok(Phase::Committed));
}

#[kani::proof]
fn routing_preserves_explicit_rules_and_rejects_suspected_spam() {
    let rule: Rule = kani::any();
    let route = initial_route(rule);
    assert!((route == Route::Ring) == (rule == Rule::Allow));
    assert!((route == Route::End) == (rule == Rule::Block));
    assert!((route == Route::Screen) == (rule == Rule::Screen));
    let classification: Classification = kani::any();
    let route = classified_route(classification);
    assert!(
        (route == Route::End)
            == matches!(
                classification,
                Classification::Spam | Classification::SuspectedSpam
            )
    );
    assert!((route == Route::Ring) == (classification == Classification::Human));
    assert!((route == Route::Voicemail) == (classification == Classification::Message));
    assert!((route == Route::Continue) == (classification == Classification::Continue));
    kani::cover!(classification == Classification::SuspectedSpam);
}

#[kani::proof]
#[kani::unwind(8)]
fn artifact_paths_cannot_escape_their_root() {
    let bytes: [u8; 257] = kani::any();
    let length: u16 = kani::any();
    kani::assume(length <= 257);
    let input = &bytes[..usize::from(length)];
    let position = usize::from(kani::any::<u8>());
    let accepted = artifact::accepts(input, position);
    if accepted {
        assert!(!input.is_empty() && input.len() <= 256);
        if position < input.len() {
            let byte = input[position];
            assert!(byte.is_ascii_alphanumeric() || b"._-/".contains(&byte));
            if byte == b'/' {
                assert!(position > 0 && position + 1 < input.len() && input[position - 1] != b'/');
            }
            if byte == b'.' && (position == 0 || input[position - 1] == b'/') {
                assert!(position + 1 < input.len() && input[position + 1] != b'/');
                if input[position + 1] == b'.' {
                    assert!(position + 2 < input.len() && input[position + 2] != b'/');
                }
            }
        }
    }
    kani::cover!(accepted && length == 256);
    kani::cover!(!accepted && length == 257);
}

#[kani::proof]
#[kani::unwind(22)]
fn candidate_commit_requires_quiescence_and_health() {
    use crate::update::{Action, Phase};
    let mut phase = if kani::any::<bool>() {
        Phase::ServingBusy
    } else {
        Phase::ServingIdle
    };
    let mut quiesced = false;
    let mut healthy = false;
    for _ in 0..20 {
        let action: Action = kani::any();
        if let Ok(next) = phase.apply(action) {
            quiesced |= action == Action::Quiesce;
            healthy |= action == Action::HealthSucceeded;
            phase = next;
        }
        assert!(phase != Phase::Committed || (quiesced && healthy));
    }
    kani::cover!(phase == Phase::Committed);
}

#[kani::proof]
#[kani::unwind(36)]
fn accepted_numbers_cannot_contain_dialplan_syntax() {
    let bytes: [u8; 33] = kani::any();
    let length: u8 = kani::any();
    kani::assume(length <= 33);
    let input = &bytes[..usize::from(length)];
    let parsed = DialNumber::parse(input);
    if let Ok(number) = parsed {
        assert!(number.bytes() == input);
        assert!(!input.is_empty() && input.len() <= 32);
        if input[0] == b'+' {
            assert!(input.len() > 1);
        }
        for (index, byte) in input.iter().enumerate() {
            assert!(byte.is_ascii_digit() || (index == 0 && *byte == b'+'));
        }
        kani::cover!(length == 32);
        kani::cover!(input[0] == b'+');
    }
    let mut expected = !input.is_empty() && input.len() <= 32;
    for (index, byte) in input.iter().enumerate() {
        expected &= byte.is_ascii_digit() || (index == 0 && *byte == b'+' && input.len() > 1);
    }
    assert_eq!(parsed.is_ok(), expected);
}

#[kani::proof]
fn line_ownership_cannot_be_replaced() {
    let first: u64 = kani::any();
    let second: u64 = kani::any();
    if let (Ok(first), Ok(second)) = (CallId::new(first), CallId::new(second)) {
        let line = Line::default().acquire(first).unwrap();
        assert!(Line::default().owner().is_none());
        assert_eq!(line.owner(), Some(first));
        assert_eq!(line.acquire(second), Err(Refusal::Busy));
        assert_eq!(line.release(first).unwrap(), Line::default());
        if first != second {
            assert_eq!(line.release(second), Err(Refusal::WrongOwner));
        }
        kani::cover!(first != second);
    }
}

#[kani::proof]
fn terminal_calls_refuse_every_action() {
    let action: Action = kani::any();
    assert!(Phase::Ended.apply(action) == Err(Refusal::InvalidTransition));
    kani::cover!(action == Action::End);
}

#[kani::proof]
fn active_recordings_are_never_purged() {
    let result = recording_policy(
        Recording::Active,
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
    );
    assert!(result == Purge::Keep);
    kani::cover!(result == Purge::Keep);
}

#[kani::proof]
fn closed_recording_policy_preserves_exact_boundaries() {
    let ended_at: u64 = kani::any();
    let now: u64 = kani::any();
    let retention: u64 = kani::any();
    let used: u64 = kani::any();
    let limit: u64 = kani::any();
    let result = recording_policy(Recording::Closed { ended_at }, now, retention, used, limit);
    match result {
        Purge::Expired => assert!(retention != 0 && now >= ended_at && now - ended_at >= retention),
        Purge::Quota => assert!(used > limit),
        Purge::Keep => assert!(used <= limit),
    }
    let expired = retention != 0
        && now
            .checked_sub(ended_at)
            .is_some_and(|age| age >= retention);
    assert_eq!(result == Purge::Expired, expired);
    assert_eq!(result == Purge::Quota, !expired && used > limit);
    assert_eq!(result == Purge::Keep, !expired && used <= limit);
    kani::cover!(result == Purge::Expired);
    kani::cover!(result == Purge::Quota);
    kani::cover!(result == Purge::Keep);
}

#[kani::proof]
fn recording_capacity_reserves_all_copies_without_integer_overflow() {
    let used: u64 = kani::any();
    let pending: u64 = kani::any();
    let maximum: u64 = kani::any();
    let limit: u64 = kani::any();
    let free: u64 = kani::any();
    let allowed = recording_capacity(used, pending, maximum, limit, free);
    if let Some(count) = pending.checked_add(4) {
        let reserved = count as u128 * maximum as u128;
        assert_eq!(
            allowed,
            maximum != 0 && reserved <= free as u128 && used as u128 + reserved <= limit as u128
        );
    } else {
        assert!(!allowed);
    }
    kani::cover!(allowed);
    kani::cover!(!allowed && pending == u64::MAX);
}

#[kani::proof]
fn playback_marks_cannot_overrun_or_move_backwards() {
    let bytes: u64 = kani::any();
    let first: u64 = kani::any();
    let second: u64 = kani::any();
    let sent = Playback::default().sent(bytes).unwrap();
    let extra: u64 = kani::any();
    match sent.sent(extra) {
        Ok(total) => assert_eq!(total.total_bytes(), bytes.checked_add(extra).unwrap()),
        Err(_) => assert!(bytes.checked_add(extra).is_none()),
    }
    if let Ok(played) = sent.acknowledge(first) {
        assert!(first <= bytes);
        assert_eq!(played.queued_bytes(), bytes - first);
        if let Ok(updated) = played.acknowledge(second) {
            assert!(second >= first && second <= bytes);
            assert_eq!(updated.played_ms(), second / 8);
        }
        assert_eq!(
            played.acknowledge(second).is_ok(),
            second >= first && second <= bytes
        );
        kani::cover!(first > 0);
    }
    assert_eq!(sent.acknowledge(first).is_ok(), first <= bytes);
}
#[kani::proof]
fn unavailable_reception_preserves_ordinary_calls() {
    let rule: Rule = kani::any();
    let available: bool = kani::any();
    let result = crate::plugins::route_with_optional_reception(rule, available);
    assert!(
        (result == Route::Ring) == (rule == Rule::Allow || (rule == Rule::Screen && !available))
    );
    assert!((result == Route::End) == (rule == Rule::Block));
    assert!((result == Route::Screen) == (rule == Rule::Screen && available));
    kani::cover!(rule == Rule::Allow && available);
    kani::cover!(rule == Rule::Allow && !available);
    kani::cover!(rule == Rule::Block && available);
    kani::cover!(rule == Rule::Block && !available);
    kani::cover!(rule == Rule::Screen && available);
    kani::cover!(rule == Rule::Screen && !available);
}

#[kani::proof]
#[kani::unwind(36)]
fn only_complete_japanese_numbers_normalize() {
    use crate::directory::NationalNumber;
    let bytes: [u8; 33] = kani::any();
    let input = &bytes[..usize::from(kani::any::<u8>() % 34)];
    let subscriber = match input {
        [b'+', b'8', b'1', rest @ ..] | [b'0', rest @ ..] => Some(rest),
        _ => None,
    };
    let complete = DialNumber::parse(input).is_ok()
        && subscriber
            .is_some_and(|digits| (digits.len() == 9 || digits.len() == 10) && digits[0] != b'0');
    let national = DialNumber::parse(input)
        .ok()
        .and_then(NationalNumber::from_dial);
    assert_eq!(national.is_some(), complete);
    if let Some(national) = national {
        assert!(NationalNumber::from_key(national.key()) == Some(national));
        kani::cover!(input[0] == b'+' && input.len() == 13);
        kani::cover!(input[0] == b'0' && input.len() == 10);
    }
    kani::cover!(!complete && input.len() == 11);
}

#[kani::proof]
#[kani::unwind(14)]
fn domestic_and_international_spellings_share_one_key() {
    use crate::directory::NationalNumber;
    let digits: [u8; 10] = kani::any();
    let length = 9 + usize::from(kani::any::<bool>());
    let mut domestic = [b'0'; 11];
    let mut international = [b'+', b'8', b'1', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    for index in 0..length {
        domestic[index + 1] = digits[index];
        international[index + 3] = digits[index];
    }
    let domestic = DialNumber::parse(&domestic[..length + 1])
        .ok()
        .and_then(NationalNumber::from_dial);
    let international = DialNumber::parse(&international[..length + 3])
        .ok()
        .and_then(NationalNumber::from_dial);
    assert!(domestic == international);
    kani::cover!(domestic.is_some() && length == 9);
    kani::cover!(domestic.is_some() && length == 10);
    kani::cover!(domestic.is_none());
}

#[kani::proof]
#[kani::unwind(5)]
fn shared_labels_count_every_distinct_directory_name() {
    use crate::directory::{CallerLabel, DirectoryMatch, caller_label};
    let matches = [0; 3].map(|_| DirectoryMatch {
        name: if kani::any() {
            "a"
        } else if kani::any() {
            "b"
        } else {
            "c"
        },
        category: "c",
    });
    let length = usize::from(kani::any::<u8>() & 3);
    let present = &matches[..length];
    let mut distinct = 0;
    for (index, candidate) in present.iter().enumerate() {
        if present[..index]
            .iter()
            .all(|earlier| earlier.name != candidate.name)
        {
            distinct += 1;
        }
    }
    match caller_label(present) {
        None => assert!(length == 0),
        Some(CallerLabel::Shared { names }) => {
            assert!(distinct >= 2 && names.get() == distinct)
        }
        Some(CallerLabel::Named { name, matches }) => {
            assert!(distinct == 1 && name == present[0].name && matches.len() == length)
        }
    }
    kani::cover!(distinct == 1 && length == 3);
    kani::cover!(distinct == 3);
}
