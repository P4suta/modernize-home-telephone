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
    #[serde(rename = "allowed_users", skip_serializing)]
    pub legacy_allowed_users: Vec<String>,
    pub api_token_env: String,
    pub ari: Ari,
    pub ai: Ai,
    pub sip: Sip,
    pub storage: Storage,
    pub recording: Option<CallRecording>,
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
    #[serde(deserialize_with = "browser_endpoint")]
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
#[serde(deny_unknown_fields)]
pub struct CallRecording {
    pub enabled: bool,
    #[serde(default = "maximum_recording_seconds")]
    pub max_seconds: u32,
    pub gateway_spool_dir: PathBuf,
    pub nas: Option<Nas>,
}

fn maximum_recording_seconds() -> u32 {
    7200
}

impl CallRecording {
    pub fn maximum_bytes(&self) -> u64 {
        u64::from(self.max_seconds) * 16000 + 4096
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Nas {
    pub archive_id: uuid::Uuid,
    pub server: String,
    pub share: String,
    pub directory: String,
    #[serde(default = "smb_port")]
    pub port: u16,
    pub username_env: String,
    pub password_env: String,
}

fn smb_port() -> u16 {
    445
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
    pub ntfy: Option<Ntfy>,
    pub webhook_url: Option<String>,
    pub webhook_secret_env: String,
    pub vapid_private_key_env: String,
    pub vapid_subject: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ntfy {
    pub server: String,
    pub topic: String,
    #[serde(default = "ntfy_token_env")]
    pub token_env: String,
}

fn ntfy_token_env() -> String {
    "HOMEPHONE_NTFY_TOKEN".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            instance_id: uuid::Uuid::new_v4(),
            mode: Mode::Lab,
            listen: DEFAULT_LISTEN_ADDRESS.parse().expect("literal address"),
            data_dir: ".local/data".into(),
            public_origin: DEFAULT_PUBLIC_ORIGIN.into(),
            legacy_allowed_users: vec![],
            api_token_env: "HOMEPHONE_API_TOKEN".into(),
            ari: Ari::default(),
            ai: Ai::default(),
            sip: Sip::default(),
            storage: Storage::default(),
            recording: None,
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
            provider: if cfg!(feature = "plugin-ai") {
                "mock"
            } else {
                "disabled"
            }
            .into(),
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
            browser_endpoint: "PJSIP/browser".into(),
            handset_endpoint: "PJSIP/handset".into(),
            carrier_endpoint: "fxo".into(),
        }
    }
}
fn browser_endpoint<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let endpoint = String::deserialize(deserializer)?;
    Ok(if endpoint == "Local/browser@home-endpoint" {
        "PJSIP/browser".into()
    } else {
        endpoint
    })
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
            ntfy: None,
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
                matches!(self.ai.provider.as_str(), "openai" | "disabled"),
                "production refuses mock AI"
            );
        }
        ensure!(
            matches!(self.ai.provider.as_str(), "mock" | "openai" | "disabled"),
            "unknown AI provider"
        );
        ensure!(
            self.ai.provider == "disabled" || cfg!(feature = "plugin-ai"),
            "AI plugin is not installed"
        );
        ensure!(
            self.notifications.ntfy.is_none() || cfg!(feature = "plugin-ntfy"),
            "ntfy plugin is not installed"
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
            self.storage.retention_days <= MAX_RETENTION_DAYS,
            "retention must be between 0 and 365 days; 0 means manual deletion"
        );
        ensure!(
            self.storage.recording_limit_bytes >= MIN_RECORDING_BYTES,
            "recording limit is too small"
        );
        if let Some(recording) = &self.recording {
            ensure!(
                (1..=7200).contains(&recording.max_seconds),
                "recording duration must be between 1 and 7200 seconds"
            );
            ensure!(
                recording.gateway_spool_dir.is_absolute(),
                "recording spool path must be absolute"
            );
            ensure!(
                self.storage.recording_limit_bytes >= recording.maximum_bytes() * 4,
                "recording spool must reserve four maximum recordings"
            );
            if let Some(nas) = &recording.nas {
                ensure!(
                    self.storage.retention_days == 0,
                    "NAS recording retention requires manual deletion"
                );
                ensure!(nas.port != 0, "invalid SMB port");
                ensure!(!nas.archive_id.is_nil(), "NAS archive identity is required");
                ensure!(
                    nas.server.len() <= 253
                        && !nas.server.is_empty()
                        && nas
                            .server
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')),
                    "invalid SMB server"
                );
                ensure!(
                    nas.share.len() <= 64
                        && !nas.share.is_empty()
                        && nas
                            .share
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')),
                    "invalid SMB share"
                );
                ensure!(
                    nas.directory.len() <= 200
                        && !nas.directory.is_empty()
                        && nas.directory.split('/').all(|part| !part.is_empty()
                            && part.len() <= 128
                            && part != "."
                            && part != ".."
                            && part
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric()
                                    || matches!(b, b'_' | b'-' | b'.'))),
                    "invalid NAS directory"
                );
                for name in [&nas.username_env, &nas.password_env] {
                    ensure!(
                        !name.is_empty()
                            && name.len() <= 128
                            && name
                                .bytes()
                                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
                        "invalid NAS secret name"
                    );
                }
                let reserved = [
                    &self.api_token_env,
                    &self.ari.password_env,
                    &self.sip.password_env,
                    &self.ai.key_env,
                    &self.notifications.webhook_secret_env,
                    &self.notifications.vapid_private_key_env,
                ];
                ensure!(
                    nas.username_env != nas.password_env
                        && [&nas.username_env, &nas.password_env]
                            .into_iter()
                            .all(|name| !reserved.contains(&name)
                                && self
                                    .notifications
                                    .ntfy
                                    .as_ref()
                                    .is_none_or(|ntfy| &ntfy.token_env != name)),
                    "NAS credentials require independent secret names"
                );
                ensure!(
                    cfg!(feature = "plugin-nas"),
                    "NAS recording plugin is not compiled"
                );
            }
        }
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
        ]
        .into_iter()
        .chain(self.notifications.ntfy.iter().map(|ntfy| &ntfy.token_env))
        {
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
        if let Some(ntfy) = &self.notifications.ntfy {
            let server = url::Url::parse(&ntfy.server)?;
            let local = match server.host() {
                Some(url::Host::Ipv4(address)) => address.is_loopback(),
                Some(url::Host::Ipv6(address)) => address.is_loopback(),
                _ => false,
            };
            ensure!(
                (server.scheme() == "https"
                    || (self.mode == Mode::Lab && server.scheme() == "http" && local))
                    && server.host_str().is_some()
                    && server.username().is_empty()
                    && server.password().is_none()
                    && server.path() == "/"
                    && server.query().is_none()
                    && server.fragment().is_none(),
                "ntfy requires an HTTPS server origin; lab HTTP requires a loopback address"
            );
            ensure!(
                !ntfy.topic.is_empty()
                    && ntfy.topic.len() <= 64
                    && ntfy
                        .topic
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
                "ntfy topic must contain 1 to 64 ASCII letters, digits, underscores, or hyphens"
            );
        }
        Ok(())
    }
}

pub fn valid_number(number: &str) -> bool {
    phone_core::DialNumber::parse(number.as_bytes()).is_ok()
}
