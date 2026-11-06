const DEFAULT_RETENTION_DAYS: u32 = 30;
const DEFAULT_SCREENING_SECONDS: u32 = 120;
const DEFAULT_RINGING_SECONDS: u32 = 30;
const DEFAULT_VOICEMAIL_SECONDS: u32 = 60;
const MAX_RETENTION_DAYS: u32 = 365;
const MIN_RECORDING_BYTES: u64 = 1_048_576;
const MAX_TIMER_SECONDS: u32 = 3600;
const DEFAULT_LISTEN_ADDRESS: &str = "127.0.0.1:8787";
const DEFAULT_PUBLIC_ORIGIN: &str = "http://localhost:8787";
const DEFAULT_ARI_URL: &str = "http://127.0.0.1:8088/ari/";

use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Lab,
    Production,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub instance_id: uuid::Uuid,
    pub mode: Mode,
    pub listen: SocketAddr,
    pub data_dir: PathBuf,
    pub public_origin: String,
    pub allowed_users: Vec<String>,
    pub api_token_env: String,
    pub ari: Ari,
    pub ai: Ai,
    pub sip: Sip,
    pub storage: Storage,
    pub timers: Timers,
    pub notifications: Notifications,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ari {
    pub url: String,
    pub username: String,
    pub password_env: String,
    pub application: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ai {
    pub provider: String,
    pub key_env: String,
    pub realtime_model: String,
    pub transcription_model: String,
    pub summary_model: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sip {
    pub domain: String,
    pub username: String,
    pub password_env: String,
    pub browser_endpoint: String,
    pub handset_endpoint: String,
    pub carrier_endpoint: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Storage {
    pub retention_days: u32,
    pub recording_limit_bytes: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Timers {
    pub screening_seconds: u32,
    pub ringing_seconds: u32,
    pub voicemail_seconds: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Notifications {
    pub webhook_url: Option<String>,
    pub webhook_secret_env: String,
    pub vapid_private_key_env: String,
    pub vapid_subject: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            instance_id: uuid::Uuid::new_v4(),
            mode: Mode::Lab,
            listen: DEFAULT_LISTEN_ADDRESS.parse().expect("literal address"),
            data_dir: ".local/data".into(),
            public_origin: DEFAULT_PUBLIC_ORIGIN.into(),
            allowed_users: vec![],
            api_token_env: "HOMEPHONE_API_TOKEN".into(),
            ari: Ari::default(),
            ai: Ai::default(),
            sip: Sip::default(),
            storage: Storage::default(),
            timers: Timers::default(),
            notifications: Notifications::default(),
        }
    }
}
impl Default for Ari {
    fn default() -> Self {
        Self {
            url: DEFAULT_ARI_URL.into(),
            username: "homephone".into(),
            password_env: "HOMEPHONE_ARI_PASSWORD".into(),
            application: "homephone".into(),
        }
    }
}
impl Default for Ai {
    fn default() -> Self {
        Self {
            provider: "mock".into(),
            key_env: "OPENAI_API_KEY".into(),
            realtime_model: "gpt-realtime-2.1".into(),
            transcription_model: "gpt-transcribe".into(),
            summary_model: "gpt-5.4-mini".into(),
        }
    }
}
impl Default for Sip {
    fn default() -> Self {
        Self {
            domain: "homephone.invalid".into(),
            username: "browser".into(),
            password_env: "HOMEPHONE_SIP_PASSWORD".into(),
            browser_endpoint: "Local/browser@home-endpoint".into(),
            handset_endpoint: "PJSIP/handset".into(),
            carrier_endpoint: "fxo".into(),
        }
    }
}
impl Default for Storage {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            recording_limit_bytes: 4 * 1024 * 1024 * 1024,
        }
    }
}
impl Default for Timers {
    fn default() -> Self {
        Self {
            screening_seconds: DEFAULT_SCREENING_SECONDS,
            ringing_seconds: DEFAULT_RINGING_SECONDS,
            voicemail_seconds: DEFAULT_VOICEMAIL_SECONDS,
        }
    }
}
impl Default for Notifications {
    fn default() -> Self {
        Self {
            webhook_url: None,
            webhook_secret_env: "HOMEPHONE_WEBHOOK_SECRET".into(),
            vapid_private_key_env: "HOMEPHONE_VAPID_PRIVATE_KEY".into(),
            vapid_subject: "mailto:owner@example.invalid".into(),
        }
    }
}

impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::ensure;
        ensure!(
            self.listen.ip().is_loopback(),
            "the HTTP listener must bind to loopback"
        );
        let origin = url::Url::parse(&self.public_origin)?;
        ensure!(
            matches!(origin.scheme(), "http" | "https") && origin.host_str().is_some(),
            "invalid public origin"
        );
        ensure!(
            origin.username().is_empty()
                && origin.password().is_none()
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "public_origin must contain only an origin"
        );
        if self.mode == Mode::Production {
            ensure!(origin.scheme() == "https", "production requires HTTPS");
            ensure!(
                !self.allowed_users.is_empty(),
                "production requires allowed tailnet users"
            );
            ensure!(self.ai.provider == "openai", "production refuses mock AI");
        }
        ensure!(
            matches!(self.ai.provider.as_str(), "mock" | "openai"),
            "unknown AI provider"
        );
        let ari = url::Url::parse(&self.ari.url)?;
        ensure!(
            matches!(ari.scheme(), "http" | "https")
                && ari.host_str().is_some()
                && ari.username().is_empty()
                && ari.password().is_none()
                && ari.path().ends_with("/ari/"),
            "invalid ARI URL"
        );
        ensure!(
            (1..=MAX_RETENTION_DAYS).contains(&self.storage.retention_days),
            "retention must be between 1 and 365 days"
        );
        ensure!(
            self.storage.recording_limit_bytes >= MIN_RECORDING_BYTES,
            "recording limit is too small"
        );
        for duration in [
            self.timers.screening_seconds,
            self.timers.ringing_seconds,
            self.timers.voicemail_seconds,
        ] {
            ensure!(
                (1..=MAX_TIMER_SECONDS).contains(&duration),
                "timer must be between 1 and 3600 seconds"
            );
        }
        for name in [
            &self.api_token_env,
            &self.ari.password_env,
            &self.ai.key_env,
            &self.sip.password_env,
            &self.notifications.webhook_secret_env,
            &self.notifications.vapid_private_key_env,
        ] {
            ensure!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
                "invalid secret environment variable name"
            );
        }
        for value in [
            &self.ari.username,
            &self.ari.application,
            &self.sip.username,
            &self.sip.carrier_endpoint,
        ] {
            ensure!(
                !value.is_empty()
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')),
                "invalid telephony identifier"
            );
        }
        if let Some(value) = &self.notifications.webhook_url {
            let url = url::Url::parse(value)?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none(),
                "invalid webhook URL"
            );
        }
        Ok(())
    }
}

pub fn valid_number(number: &str) -> bool {
    phone_core::DialNumber::parse(number.as_bytes()).is_ok()
}
