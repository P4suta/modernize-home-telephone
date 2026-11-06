use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Offered,
    Screening,
    Ringing,
    Connected,
    Held,
    Voicemail,
    Ended,
}

impl From<phone_core::Phase> for Phase {
    fn from(value: phone_core::Phase) -> Self {
        match value {
            phone_core::Phase::Offered => Self::Offered,
            phone_core::Phase::Screening => Self::Screening,
            phone_core::Phase::Ringing => Self::Ringing,
            phone_core::Phase::Connected => Self::Connected,
            phone_core::Phase::Held => Self::Held,
            phone_core::Phase::Voicemail => Self::Voicemail,
            phone_core::Phase::Ended => Self::Ended,
        }
    }
}
impl From<Phase> for phone_core::Phase {
    fn from(value: Phase) -> Self {
        match value {
            Phase::Offered => Self::Offered,
            Phase::Screening => Self::Screening,
            Phase::Ringing => Self::Ringing,
            Phase::Connected => Self::Connected,
            Phase::Held => Self::Held,
            Phase::Voicemail => Self::Voicemail,
            Phase::Ended => Self::Ended,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Recording {
    None,
    Pending,
    Available { bytes: u64 },
    Deleted { reason: String },
    Failed { reason: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct Transcript {
    pub item_id: String,
    pub speaker: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
pub struct Call {
    pub id: Uuid,
    pub direction: String,
    pub caller: String,
    pub phase: Phase,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub reason: Option<String>,
    pub transcript: Vec<Transcript>,
    pub summary: Option<String>,
    pub recording: Recording,
    pub processing: String,
    pub last_error: Option<String>,
}
impl Call {
    pub fn offered(caller: String, direction: String, now: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            direction,
            caller,
            phase: Phase::Offered,
            started_at: now,
            ended_at: None,
            reason: None,
            transcript: vec![],
            summary: None,
            recording: Recording::None,
            processing: "waiting".into(),
            last_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    Allow,
    Block,
    #[default]
    Screen,
}
impl From<Rule> for phone_core::Rule {
    fn from(value: Rule) -> Self {
        match value {
            Rule::Allow => Self::Allow,
            Rule::Block => Self::Block,
            Rule::Screen => Self::Screen,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, ToSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub rules: BTreeMap<String, Rule>,
    pub retention_days: Option<u32>,
    pub recording_limit_bytes: Option<u64>,
    pub webhook_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct Event {
    pub id: i64,
    pub kind: String,
    pub call: Option<Call>,
    pub created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: PushKeys,
}
#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PushKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Clone, Debug)]
pub struct Delivery {
    pub id: i64,
    pub payload: String,
    pub attempts: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Answer,
    Hangup,
    Takeover,
    Hold,
    Resume,
    Dtmf { digits: String },
    Speak { text: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Originate {
    pub number: String,
}
