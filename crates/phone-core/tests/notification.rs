use phone_core::{
    Phase,
    notification::{CallNotification, Direction, EventKind, Publication},
};

fn incoming(started_at: u64, created_at: u64) -> CallNotification {
    CallNotification {
        event: EventKind::Incoming,
        direction: Direction::Inbound,
        phase: Phase::Offered,
        started_at,
        created_at,
    }
}

#[test]
fn fresh_inbound_notifications_preserve_exact_time_boundaries() {
    assert_eq!(incoming(10, 10).incoming(10), Publication::Publish);
    assert_eq!(incoming(10, 11).incoming(130), Publication::Publish);
    assert_eq!(incoming(10, 10).incoming(131), Publication::Skip);
    assert_eq!(incoming(10, 9).incoming(10), Publication::Skip);
    assert_eq!(incoming(10, 11).incoming(10), Publication::Skip);
    assert_eq!(incoming(0, 0).incoming(0), Publication::Publish);
    assert_eq!(incoming(0, 0).incoming(120), Publication::Publish);
    assert_eq!(incoming(0, 0).incoming(u64::MAX), Publication::Skip);
    assert_eq!(
        incoming(u64::MAX, u64::MAX).incoming(u64::MAX),
        Publication::Publish
    );
    assert_eq!(incoming(u64::MAX, u64::MAX).incoming(0), Publication::Skip);
    assert_eq!(
        incoming(u64::MAX - 120, u64::MAX).incoming(u64::MAX),
        Publication::Publish
    );
    assert_eq!(
        incoming(u64::MAX - 121, u64::MAX).incoming(u64::MAX),
        Publication::Skip
    );
}

#[test]
fn only_the_original_inbound_offered_event_can_notify() {
    for event in [
        EventKind::Incoming,
        EventKind::Ringing,
        EventKind::Voicemail,
        EventKind::Ended,
        EventKind::Complete,
        EventKind::Other,
    ] {
        for direction in [Direction::Inbound, Direction::Outbound, Direction::Unknown] {
            for phase in [
                Phase::Offered,
                Phase::Screening,
                Phase::Ringing,
                Phase::Connected,
                Phase::Held,
                Phase::Voicemail,
                Phase::Ended,
            ] {
                let value = CallNotification {
                    event,
                    direction,
                    phase,
                    ..incoming(10, 10)
                };
                assert_eq!(
                    value.incoming(10),
                    if event == EventKind::Incoming
                        && direction == Direction::Inbound
                        && phase == Phase::Offered
                    {
                        Publication::Publish
                    } else {
                        Publication::Skip
                    }
                );
            }
        }
    }
}

#[test]
fn notification_names_reject_unknown_and_misleading_external_values() {
    for (value, expected) in [
        ("incoming", EventKind::Incoming),
        ("ringing", EventKind::Ringing),
        ("voicemail", EventKind::Voicemail),
        ("ended", EventKind::Ended),
        ("complete", EventKind::Complete),
        ("", EventKind::Other),
        ("Incoming", EventKind::Other),
        ("incoming ", EventKind::Other),
        ("connected", EventKind::Other),
    ] {
        assert_eq!(EventKind::from_kind(value), expected);
    }
    for (value, expected) in [
        ("inbound", Direction::Inbound),
        ("outbound", Direction::Outbound),
        ("", Direction::Unknown),
        ("inbound ", Direction::Unknown),
        ("INBOUND", Direction::Unknown),
    ] {
        assert_eq!(Direction::from_name(value), expected);
    }
}
