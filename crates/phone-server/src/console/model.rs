use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum PageId {
    Status,
    Basic,
    Advanced,
    Fxs,
    Fxo,
}

impl PageId {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Basic => "basic",
            Self::Advanced => "advanced",
            Self::Fxs => "fxs",
            Self::Fxo => "fxo",
        }
    }
    pub const ALL: [Self; 5] = [
        Self::Status,
        Self::Basic,
        Self::Advanced,
        Self::Fxs,
        Self::Fxo,
    ];

    pub const fn path(self) -> &'static str {
        match self {
            Self::Status => "/cgi-bin/index",
            Self::Basic => "/cgi-bin/config2",
            Self::Advanced => "/cgi-bin/config",
            Self::Fxs => "/cgi-bin/config_a1",
            Self::Fxo => "/cgi-bin/config_a2",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Status => "状態",
            Self::Basic => "基本設定",
            Self::Advanced => "詳細設定",
            Self::Fxs => "電話機（FXS）",
            Self::Fxo => "電話回線（FXO）",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Password,
    TextArea,
    Select,
    Radio,
    Checkbox,
    Number,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, Eq, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, Eq, PartialEq)]
pub struct Field {
    pub id: String,
    pub parameter: String,
    pub secret: bool,
    pub label: String,
    pub description: String,
    pub kind: FieldKind,
    pub values: Vec<String>,
    pub choices: Vec<Choice>,
    pub disabled: bool,
    pub read_only: bool,
    pub multiple: bool,
    pub required: bool,
    pub max_length: Option<u32>,
    pub min: Option<i32>,
    pub max: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, Eq, PartialEq)]
pub struct Information {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Login {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct SessionInfo {
    pub authenticated: bool,
    pub username: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WriteMode {
    Save,
    SaveAndApply,
}
impl From<WriteMode> for phone_core::console::Mode {
    fn from(value: WriteMode) -> Self {
        match value {
            WriteMode::Save => Self::Save,
            WriteMode::SaveAndApply => Self::SaveAndApply,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Changes {
    pub revision: String,
    pub mode: WriteMode,
    pub values: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HelpReference {
    Template { firmware: String, line: u32 },
    Device { firmware: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Help {
    pub key: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub parameters: Vec<String>,
    pub category: String,
    pub summary: String,
    pub input: String,
    pub default_value: String,
    pub recommendation: String,
    pub impact: String,
    pub source: String,
    pub template_value: Option<String>,
    pub reference: HelpReference,
    pub ranges: Vec<IntegerRange>,
    pub note: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IntegerRange {
    pub min: i32,
    pub max: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ParameterReference {
    pub id: String,
    pub title: String,
    pub topic: String,
    pub section: String,
    pub source_line: u32,
    pub active: bool,
    pub template_value: Option<String>,
    pub default_value: Option<String>,
    pub ranges: Vec<IntegerRange>,
    pub note: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HelpTopic {
    pub key: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub summary: String,
    pub input: String,
    pub recommendation: String,
    pub impact: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct Setting {
    pub field: Field,
    pub help: Option<Help>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct PageSnapshot {
    pub page: PageId,
    pub revision: String,
    pub fetched_at_ms: u64,
    pub settings: Vec<Setting>,
    pub information: Vec<Information>,
    pub can_save: bool,
    pub can_apply: bool,
    pub reconciliation_required: bool,
    pub undocumented: usize,
    pub native_session_recovered: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WriteOutcome {
    Confirmed,
    Uncertain,
    VerificationRequired,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct WriteReceipt {
    pub outcome: WriteOutcome,
    pub snapshot: Option<PageSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct Problem {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteReview {
    pub page: PageId,
    pub revision: String,
    pub mode: WriteMode,
    pub changes: usize,
    pub secret_changes: usize,
}
