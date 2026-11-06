#![no_main]
use phone_core::{Action, Phase, Playback};
libfuzzer_sys::fuzz_target!(|bytes: &[u8]| {
    let mut phase = Phase::Offered;
    let mut playback = Playback::default();
    for &byte in bytes {
        let action = match byte % 9 {
            0 => Action::Screen,
            1 => Action::Ring,
            2 => Action::Connect,
            3 => Action::Hold,
            4 => Action::Resume,
            5 => Action::RecordMessage,
            6 => Action::End,
            7 => Action::Speak,
            _ => Action::Dtmf,
        };
        let previous = phase;
        if let Ok(updated) = phase.apply(action) {
            assert_ne!(previous, Phase::Ended);
            phase = updated;
        }
        if let Ok(sent) = playback.sent(u64::from(byte)) {
            playback = sent;
        }
        if let Ok(acknowledged) = playback.acknowledge(u64::from(byte) * 3) {
            playback = acknowledged;
        }
        assert!(playback.acknowledged_bytes() <= playback.total_bytes());
        assert_eq!(
            playback.queued_bytes() + playback.acknowledged_bytes(),
            playback.total_bytes()
        );
    }
});
