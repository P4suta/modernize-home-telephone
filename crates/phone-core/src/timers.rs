use crate::Phase;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timeout {
    None,
    Ring,
    Message,
    End,
}
pub fn expired(
    phase: Phase,
    elapsed: u64,
    screen: u32,
    ring: u32,
    message: u32,
    outbound: bool,
) -> Timeout {
    match phase {
        Phase::Offered => {
            if elapsed >= ADMISSION_TIMEOUT_SECONDS {
                Timeout::End
            } else {
                Timeout::None
            }
        }
        Phase::Screening => {
            if elapsed >= u64::from(screen) {
                Timeout::Ring
            } else {
                Timeout::None
            }
        }
        Phase::Ringing => {
            if elapsed < u64::from(ring) {
                Timeout::None
            } else if outbound {
                Timeout::End
            } else {
                Timeout::Message
            }
        }
        Phase::Voicemail => {
            if elapsed >= u64::from(message) {
                Timeout::End
            } else {
                Timeout::None
            }
        }
        Phase::Connected | Phase::Held | Phase::Ended => Timeout::None,
    }
}
const ADMISSION_TIMEOUT_SECONDS: u64 = 5;
