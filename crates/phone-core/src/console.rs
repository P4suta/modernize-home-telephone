#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Save,
    SaveAndApply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Ready,
    InFlight,
    ReconciliationRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    InvalidRevision,
    StaleRevision,
    NoChanges,
    Busy,
    ReconciliationRequired,
    InvalidTransition,
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Revision(u64);

impl Revision {
    pub const fn new(value: u64) -> Result<Self, Refusal> {
        if value == 0 {
            Err(Refusal::InvalidRevision)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmissionGuard {
    revision: Revision,
    phase: Phase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WritePlan {
    guard: SubmissionGuard,
    mode: Mode,
}

impl WritePlan {
    pub const fn guard(self) -> SubmissionGuard {
        self.guard
    }

    pub const fn mode(self) -> Mode {
        self.mode
    }
}

impl SubmissionGuard {
    pub const fn new(revision: Revision) -> Self {
        Self {
            revision,
            phase: Phase::Ready,
        }
    }

    pub const fn revision(self) -> Revision {
        self.revision
    }

    pub const fn phase(self) -> Phase {
        self.phase
    }

    pub fn plan(
        self,
        expected: Revision,
        changes: usize,
        mode: Mode,
    ) -> Result<WritePlan, Refusal> {
        match self.phase {
            Phase::InFlight => return Err(Refusal::Busy),
            Phase::ReconciliationRequired => return Err(Refusal::ReconciliationRequired),
            Phase::Ready => {}
        }
        if expected != self.revision {
            return Err(Refusal::StaleRevision);
        }
        if changes == 0 {
            return Err(Refusal::NoChanges);
        }
        if self.revision.get() == u64::MAX {
            return Err(Refusal::RevisionExhausted);
        }
        Ok(WritePlan {
            guard: Self {
                phase: Phase::InFlight,
                ..self
            },
            mode,
        })
    }

    pub fn submitted(self) -> Result<Self, Refusal> {
        match self.phase {
            Phase::InFlight => Ok(Self {
                phase: Phase::ReconciliationRequired,
                ..self
            }),
            Phase::Ready | Phase::ReconciliationRequired => Err(Refusal::InvalidTransition),
        }
    }

    pub fn refreshed(self, changed: bool) -> Result<Self, Refusal> {
        match self.phase {
            Phase::InFlight => Err(Refusal::Busy),
            Phase::Ready if !changed => Ok(self),
            Phase::Ready | Phase::ReconciliationRequired => {
                match self.revision.get().checked_add(1) {
                    Some(value) => Ok(Self::new(Revision(value))),
                    None => Err(Refusal::RevisionExhausted),
                }
            }
        }
    }
}
