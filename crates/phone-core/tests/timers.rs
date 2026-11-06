use phone_core::{
    Phase,
    timers::{Timeout, expired},
};
#[test]
fn deadlines_fire_at_the_boundary_and_never_end_a_connected_or_held_call() {
    for (phase, limit, expected) in [
        (Phase::Screening, 120, Timeout::Ring),
        (Phase::Ringing, 30, Timeout::Message),
        (Phase::Voicemail, 60, Timeout::End),
    ] {
        assert_eq!(expired(phase, limit - 1, 120, 30, 60, false), Timeout::None);
        assert_eq!(expired(phase, limit, 120, 30, 60, false), expected);
        assert_eq!(expired(phase, u64::MAX, 120, 30, 60, false), expected);
    }
    assert_eq!(expired(Phase::Ringing, 30, 120, 30, 60, true), Timeout::End);
    assert_eq!(
        expired(Phase::Offered, 4, 120, 30, 60, false),
        Timeout::None
    );
    assert_eq!(expired(Phase::Offered, 5, 120, 30, 60, false), Timeout::End);
    for phase in [Phase::Connected, Phase::Held, Phase::Ended] {
        assert_eq!(expired(phase, u64::MAX, 0, 0, 0, false), Timeout::None);
    }
}
