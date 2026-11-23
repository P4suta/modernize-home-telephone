use crate::Phase;

pub const INCOMING_WINDOW_SECONDS: u64 = 120;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum EventKind {
    Incoming,
    Ringing,
    Voicemail,
    Ended,
    Complete,
    Other,
}

impl EventKind {
    pub fn from_kind(value: &str) -> Self {
        match value {
            "incoming" => Self::Incoming,
            "ringing" => Self::Ringing,
            "voicemail" => Self::Voicemail,
            "ended" => Self::Ended,
            "complete" => Self::Complete,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Direction {
    Inbound,
    Outbound,
    Unknown,
}

impl Direction {
    pub fn from_name(value: &str) -> Self {
        match value {
            "inbound" => Self::Inbound,
            "outbound" => Self::Outbound,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Publication {
    Publish,
    Skip,
}

#[derive(Clone, Copy, Debug)]
pub struct CallNotification {
    pub event: EventKind,
    pub direction: Direction,
    pub phase: Phase,
    pub started_at: u64,
    pub created_at: u64,
}

impl CallNotification {
    pub fn incoming(self, now: u64) -> Publication {
        if self.event == EventKind::Incoming
            && self.direction == Direction::Inbound
            && self.phase == Phase::Offered
            && self.started_at <= self.created_at
            && self.created_at <= now
            && now
                .checked_sub(self.started_at)
                .is_some_and(|age| age <= INCOMING_WINDOW_SECONDS)
        {
            Publication::Publish
        } else {
            Publication::Skip
        }
    }
}
