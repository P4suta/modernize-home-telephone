use crate::Refusal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Phase {
    ServingIdle,
    ServingBusy,
    Quiesced,
    Stopped,
    CandidateRunning,
    CandidateHealthy,
    Committed,
    RolledBack,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Action {
    Quiesce,
    Stop,
    StartCandidate,
    HealthSucceeded,
    Commit,
    Rollback,
}
impl Phase {
    pub fn apply(self, action: Action) -> Result<Self, Refusal> {
        match (self, action) {
            (Self::ServingBusy, Action::Quiesce) => Err(Refusal::Busy),
            (Self::ServingIdle, Action::Quiesce) => Ok(Self::Quiesced),
            (Self::Quiesced, Action::Stop) => Ok(Self::Stopped),
            (Self::Stopped, Action::StartCandidate) => Ok(Self::CandidateRunning),
            (Self::CandidateRunning, Action::HealthSucceeded) => Ok(Self::CandidateHealthy),
            (Self::CandidateHealthy, Action::Commit) => Ok(Self::Committed),
            (
                Self::Quiesced | Self::Stopped | Self::CandidateRunning | Self::CandidateHealthy,
                Action::Rollback,
            ) => Ok(Self::RolledBack),
            _ => Err(Refusal::InvalidTransition),
        }
    }
}
