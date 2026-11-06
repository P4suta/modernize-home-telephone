#![no_std]
#![cfg_attr(kani, feature(stmt_expr_attributes, proc_macro_hygiene))]
#![deny(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
pub mod artifact;
pub mod bytes;
pub mod timers;
pub mod update;

pub const MAX_DIAL_NUMBER_BYTES: usize = 32;
const PCMU_BYTES_PER_MILLISECOND: u64 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    InvalidId,
    Busy,
    WrongOwner,
    InvalidTransition,
    Overflow,
    InvalidAcknowledgement,
    InvalidNumber,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialNumber<'a>(&'a [u8]);

impl<'a> DialNumber<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Refusal> {
        if !bytes::nonempty_within(bytes.len(), MAX_DIAL_NUMBER_BYTES) {
            return Err(Refusal::InvalidNumber);
        }
        let digits = if bytes.first() == Some(&b'+') {
            &bytes[1..]
        } else {
            bytes
        };
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return Err(Refusal::InvalidNumber);
        }
        Ok(Self(bytes))
    }
    pub const fn bytes(self) -> &'a [u8] {
        self.0
    }
}

impl CallId {
    pub const fn new(value: u64) -> Result<Self, Refusal> {
        if value == 0 {
            Err(Refusal::InvalidId)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Line {
    owner: Option<CallId>,
}

impl Line {
    pub const fn owner(self) -> Option<CallId> {
        self.owner
    }
    pub fn acquire(self, id: CallId) -> Result<Self, Refusal> {
        match self.owner {
            None => Ok(Self { owner: Some(id) }),
            Some(_) => Err(Refusal::Busy),
        }
    }
    pub fn release(self, id: CallId) -> Result<Self, Refusal> {
        match self.owner {
            Some(owner) if owner == id => Ok(Self::default()),
            None | Some(_) => Err(Refusal::WrongOwner),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Phase {
    Offered,
    Screening,
    Ringing,
    Connected,
    Held,
    Voicemail,
    Ended,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Action {
    Screen,
    Ring,
    Connect,
    Hold,
    Resume,
    RecordMessage,
    End,
    Speak,
    Dtmf,
}

impl Phase {
    pub fn apply(self, action: Action) -> Result<Self, Refusal> {
        use Action::*;
        use Phase::*;
        match (self, action) {
            (Offered, Screen) => Ok(Screening),
            (Offered | Screening, Ring) => Ok(Ringing),
            (Offered | Screening | Ringing, Connect) => Ok(Connected),
            (Connected, Hold) => Ok(Held),
            (Held, Resume) => Ok(Connected),
            (Screening | Ringing, RecordMessage) => Ok(Voicemail),
            (Offered | Screening | Ringing | Connected | Held | Voicemail, End) => Ok(Ended),
            (Screening | Connected, Speak) | (Connected, Dtmf) => Ok(self),
            _ => Err(Refusal::InvalidTransition),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Rule {
    Allow,
    Block,
    Screen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    Screen,
    Ring,
    End,
    Continue,
    Voicemail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Classification {
    Continue,
    Spam,
    SuspectedSpam,
    Human,
    Message,
}

pub fn initial_route(rule: Rule) -> Route {
    match rule {
        Rule::Allow => Route::Ring,
        Rule::Block => Route::End,
        Rule::Screen => Route::Screen,
    }
}
pub fn classified_route(classification: Classification) -> Route {
    match classification {
        Classification::Continue => Route::Continue,
        Classification::Spam | Classification::SuspectedSpam => Route::End,
        Classification::Human => Route::Ring,
        Classification::Message => Route::Voicemail,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recording {
    Active,
    Closed { ended_at: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Purge {
    Keep,
    Expired,
    Quota,
}

pub fn recording_policy(
    recording: Recording,
    now: u64,
    retention: u64,
    used: u64,
    limit: u64,
) -> Purge {
    match recording {
        Recording::Active => Purge::Keep,
        Recording::Closed { ended_at } => {
            if now
                .checked_sub(ended_at)
                .is_some_and(|age| age >= retention)
            {
                Purge::Expired
            } else if used > limit {
                Purge::Quota
            } else {
                Purge::Keep
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Playback {
    sent: u64,
    acknowledged: u64,
}

impl Playback {
    pub const fn total_bytes(self) -> u64 {
        self.sent
    }
    pub const fn acknowledged_bytes(self) -> u64 {
        self.acknowledged
    }
    pub fn sent(self, bytes: u64) -> Result<Self, Refusal> {
        match self.sent.checked_add(bytes) {
            Some(sent) => Ok(Self { sent, ..self }),
            None => Err(Refusal::Overflow),
        }
    }
    pub fn acknowledge(self, bytes: u64) -> Result<Self, Refusal> {
        if bytes >= self.acknowledged && bytes <= self.sent {
            Ok(Self {
                acknowledged: bytes,
                ..self
            })
        } else {
            Err(Refusal::InvalidAcknowledgement)
        }
    }
    pub fn played_ms(self) -> u64 {
        self.acknowledged / PCMU_BYTES_PER_MILLISECOND
    }
    pub fn queued_bytes(self) -> u64 {
        self.sent - self.acknowledged
    }
}

#[cfg(kani)]
mod proofs;
