use phone_core::{
    Action, CallId, Classification, Line, Phase, Playback, Purge, Recording, Refusal, Route, Rule,
    classified_route, initial_route, recording_policy,
};

fn id(value: u64) -> CallId {
    CallId::new(value).unwrap()
}

#[test]
fn dial_numbers_accept_exactly_bounded_ascii_digits_with_an_optional_leading_plus() {
    use phone_core::DialNumber;
    for length in 0..=40 {
        let number = vec![b'1'; length];
        assert_eq!(
            DialNumber::parse(&number).is_ok(),
            (1..=32).contains(&length)
        );
    }
    for byte in 0_u8..=255 {
        assert_eq!(
            DialNumber::parse(&[byte, b'1']).is_ok(),
            byte.is_ascii_digit() || byte == b'+'
        );
        assert_eq!(
            DialNumber::parse(&[b'1', byte]).is_ok(),
            byte.is_ascii_digit()
        );
    }
    assert!(DialNumber::parse(b"+").is_err());
    assert!(DialNumber::parse(b"+0").is_ok());
    assert!(DialNumber::parse(&[b'1'; 1024]).is_err());
}

#[test]
fn ids_reject_zero_and_preserve_all_other_values() {
    assert_eq!(CallId::new(0), Err(Refusal::InvalidId));
    for value in [1, 2, u64::MAX] {
        assert_eq!(id(value).get(), value);
    }
}

#[test]
fn the_external_line_has_exactly_one_owner() {
    let empty = Line::default();
    assert_eq!(empty.owner(), None);
    let occupied = empty.acquire(id(1)).unwrap();
    assert_eq!(occupied.owner(), Some(id(1)));
    assert_eq!(occupied.acquire(id(2)), Err(Refusal::Busy));
    assert_eq!(occupied.acquire(id(1)), Err(Refusal::Busy));
    assert_eq!(occupied.release(id(2)), Err(Refusal::WrongOwner));
    assert_eq!(occupied.release(id(1)).unwrap(), empty);
    assert_eq!(empty.release(id(1)), Err(Refusal::WrongOwner));
}

#[test]
fn every_transition_is_specified_including_refusals() {
    use Action::*;
    use Phase::*;
    let phases = [
        Offered, Screening, Ringing, Connected, Held, Voicemail, Ended,
    ];
    let actions = [
        Screen,
        Ring,
        Connect,
        Hold,
        Resume,
        RecordMessage,
        End,
        Speak,
        Dtmf,
    ];
    for phase in phases {
        for action in actions {
            let expected = match (phase, action) {
                (Offered, Screen) => Ok(Screening),
                (Offered | Screening, Ring) => Ok(Ringing),
                (Offered | Screening | Ringing, Connect) => Ok(Connected),
                (Connected, Hold) => Ok(Held),
                (Held, Resume) => Ok(Connected),
                (Screening | Ringing, RecordMessage) => Ok(Voicemail),
                (Offered | Screening | Ringing | Connected | Held | Voicemail, End) => Ok(Ended),
                (Screening | Connected, Speak) | (Connected, Dtmf) => Ok(phase),
                _ => Err(Refusal::InvalidTransition),
            };
            assert_eq!(phase.apply(action), expected, "{phase:?} / {action:?}");
        }
    }
}

#[test]
fn number_rules_and_aggressive_spam_decisions_route_calls() {
    assert_eq!(initial_route(Rule::Allow), Route::Ring);
    assert_eq!(initial_route(Rule::Block), Route::End);
    assert_eq!(initial_route(Rule::Screen), Route::Screen);
    assert_eq!(classified_route(Classification::Continue), Route::Continue);
    assert_eq!(classified_route(Classification::Spam), Route::End);
    assert_eq!(classified_route(Classification::SuspectedSpam), Route::End);
    assert_eq!(classified_route(Classification::Human), Route::Ring);
    assert_eq!(classified_route(Classification::Message), Route::Voicemail);
}

#[test]
fn retention_handles_boundaries_clock_regression_and_active_audio() {
    let closed = Recording::Closed { ended_at: 100 };
    assert_eq!(recording_policy(closed, 129, 30, 10, 10), Purge::Keep);
    assert_eq!(recording_policy(closed, 130, 30, 10, 10), Purge::Expired);
    assert_eq!(recording_policy(closed, 99, 30, 10, 10), Purge::Keep);
    assert_eq!(recording_policy(closed, 100, 30, 11, 10), Purge::Quota);
    assert_eq!(recording_policy(closed, 130, 30, 11, 10), Purge::Expired);
    for now in [0, 100, u64::MAX] {
        assert_eq!(
            recording_policy(Recording::Active, now, 0, u64::MAX, 0),
            Purge::Keep
        );
    }
}

#[test]
fn playback_tracks_only_acknowledged_pcmu_and_rejects_reordered_marks() {
    let empty = Playback::default();
    assert_eq!(empty.played_ms(), 0);
    let sent = empty.sent(160).unwrap().sent(80).unwrap();
    assert_eq!(sent.queued_bytes(), 240);
    let played = sent.acknowledge(160).unwrap();
    assert_eq!(played.total_bytes(), 240);
    assert_eq!(played.acknowledged_bytes(), 160);
    assert_eq!(played.played_ms(), 20);
    assert_eq!(played.queued_bytes(), 80);
    assert_eq!(
        played.acknowledge(159),
        Err(Refusal::InvalidAcknowledgement)
    );
    assert_eq!(
        played.acknowledge(241),
        Err(Refusal::InvalidAcknowledgement)
    );
    assert_eq!(played.acknowledge(240).unwrap().queued_bytes(), 0);
    assert_eq!(
        empty.sent(u64::MAX).unwrap().sent(1),
        Err(Refusal::Overflow)
    );
}
#[test]
fn validated_numbers_round_trip_without_changing_the_admitted_bytes() {
    for input in [
        "1".to_owned(),
        "+1".to_owned(),
        "1".repeat(32),
        format!("+{}", "1".repeat(31)),
    ] {
        let number = phone_core::DialNumber::parse(input.as_bytes()).unwrap();
        assert_eq!(number.bytes(), input.as_bytes());
        assert_eq!(phone_core::DialNumber::parse(number.bytes()), Ok(number));
    }
}
