use super::*;

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
        Purge::Expired => assert!(now >= ended_at && now - ended_at >= retention),
        Purge::Quota => assert!(used > limit),
        Purge::Keep => assert!(used <= limit),
    }
    let expired = now
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
