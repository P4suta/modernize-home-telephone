const MIN_API_TOKEN_BYTES: usize = 32;
const MAX_SECRET_NAME_BYTES: usize = 128;
const MAX_NTFY_TOKEN_BYTES: usize = 4096;

use crate::config::{Config, Mode};

#[derive(Clone)]
pub struct Secrets {
    pub ari_password: String,
    pub sip_password: String,
    pub api_token: Option<String>,
    pub openai_key: Option<String>,
    pub webhook_secret: Option<String>,
    pub vapid_pem: Option<String>,
    pub ntfy_token: Option<NtfyToken>,
    pub nas_credentials: Option<NasCredentials>,
}

#[derive(Clone)]
pub struct NasCredentials {
    pub username: String,
    pub password: String,
}

impl NasCredentials {
    fn parse(username: String, password: String) -> anyhow::Result<Self> {
        #[cfg(feature = "plugin-nas")]
        let supported_username = phone_plugin_nas::accepts_username(&username);
        #[cfg(not(feature = "plugin-nas"))]
        let supported_username = false;
        anyhow::ensure!(supported_username, "invalid NAS username");
        anyhow::ensure!(
            !password.is_empty()
                && password.len() <= 4096
                && !password.bytes().any(|b| matches!(b, b'\0' | b'\n' | b'\r')),
            "invalid NAS password"
        );
        Ok(Self { username, password })
    }
}

#[derive(Clone)]
pub struct NtfyToken(String);

impl NtfyToken {
    pub fn parse(value: String) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !value.is_empty()
                && value.len() <= MAX_NTFY_TOKEN_BYTES
                && value.bytes().all(|byte| byte.is_ascii_graphic()),
            "ntfy token must contain 1 to 4096 visible ASCII characters without spaces"
        );
        Ok(Self(value))
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

impl Secrets {
    pub fn export_nas_credentials(
        config: &Config,
        input: &std::path::Path,
        output: &std::path::Path,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<()> {
        use std::io::Write as _;
        config.validate()?;
        let nas = config
            .recording
            .as_ref()
            .and_then(|recording| recording.nas.as_ref())
            .ok_or_else(|| anyhow::anyhow!("NAS recording storage is unconfigured"))?;
        let credentials = NasCredentials::parse(
            lookup(&nas.username_env)
                .ok_or_else(|| anyhow::anyhow!("NAS username is unavailable"))?,
            lookup(&nas.password_env)
                .ok_or_else(|| anyhow::anyhow!("NAS password is unavailable"))?,
        )?;
        let mut values = parse_file(&std::fs::read_to_string(input)?)?;
        values.insert(nas.username_env.clone(), credentials.username);
        values.insert(nas.password_env.clone(), credentials.password);
        let contents: String = values
            .into_iter()
            .map(|(key, value)| format!("{key}={value}\n"))
            .collect();
        parse_file(&contents)?;
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(output)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
    pub fn from_lookup_for_calls(
        config: &Config,
        lookup: impl Fn(&str) -> Option<String>,
        directory: Option<&std::path::Path>,
    ) -> anyhow::Result<Self> {
        let mut required = config.clone();
        required.notifications.ntfy = None;
        if let Some(recording) = &mut required.recording {
            recording.nas = None;
        }
        let mut core = Self::from_lookup(&required, &lookup, directory)?;
        if config.notifications.ntfy.is_some() {
            let mut plugin = required.clone();
            plugin.notifications.ntfy = config.notifications.ntfy.clone();
            match Self::from_lookup(&plugin, &lookup, directory) {
                Ok(complete) => core.ntfy_token = complete.ntfy_token,
                Err(_) => {
                    tracing::warn!(
                        operation = "plugin_startup",
                        plugin = "ntfy",
                        code = "credentials_unavailable"
                    );
                }
            }
        }
        if config
            .recording
            .as_ref()
            .and_then(|recording| recording.nas.as_ref())
            .is_some()
        {
            required.recording = config.recording.clone();
            match Self::from_lookup(&required, &lookup, directory) {
                Ok(complete) => core.nas_credentials = complete.nas_credentials,
                Err(_) => tracing::warn!(
                    operation = "plugin_startup",
                    plugin = "nas",
                    code = "credentials_unavailable"
                ),
            }
        }
        Ok(core)
    }
    pub fn load(config: &Config) -> anyhow::Result<Self> {
        Self::load_with_file(config, None)
    }
    pub fn load_with_file(config: &Config, path: Option<&std::path::Path>) -> anyhow::Result<Self> {
        Self::load_file(config, path, false)
    }
    pub fn load_for_calls(config: &Config, path: Option<&std::path::Path>) -> anyhow::Result<Self> {
        Self::load_file(config, path, true)
    }
    fn load_file(
        config: &Config,
        path: Option<&std::path::Path>,
        isolate_plugins: bool,
    ) -> anyhow::Result<Self> {
        let values = match path.filter(|path| path.is_file()) {
            Some(path) => parse_file(&std::fs::read_to_string(path)?)?,
            None => std::collections::BTreeMap::new(),
        };
        let lookup = |name: &str| {
            std::env::var(name)
                .ok()
                .or_else(|| values.get(name).cloned())
                .filter(|value| !value.is_empty())
        };
        if isolate_plugins {
            Self::from_lookup_for_calls(config, lookup, path.and_then(|path| path.parent()))
        } else {
            Self::from_lookup(config, lookup, path.and_then(|path| path.parent()))
        }
    }
    pub fn from_lookup(
        config: &Config,
        lookup: impl Fn(&str) -> Option<String>,
        directory: Option<&std::path::Path>,
    ) -> anyhow::Result<Self> {
        let required = |name: &str, lab: &str| -> anyhow::Result<String> {
            lookup(name)
                .or_else(|| (config.mode == Mode::Lab).then(|| lab.to_owned()))
                .ok_or_else(|| {
                    anyhow::anyhow!("missing required credential environment variable: {name}")
                })
        };
        let api_token = lookup(&config.api_token_env);
        anyhow::ensure!(
            api_token
                .as_ref()
                .is_none_or(|value| value.len() >= MIN_API_TOKEN_BYTES),
            "API token must contain at least 32 bytes"
        );
        let vapid_pem = lookup(&config.notifications.vapid_private_key_env)
            .map(|value| -> anyhow::Result<String> {
                if let Some(path) = value.strip_prefix('@') {
                    Ok(std::fs::read_to_string(
                        directory
                            .unwrap_or_else(|| std::path::Path::new("."))
                            .join(path),
                    )?)
                } else {
                    Ok(value)
                }
            })
            .transpose()?;
        if let Some(pem) = &vapid_pem {
            crate::push::public_key(pem)?;
        }
        let ntfy_token = config
            .notifications
            .ntfy
            .as_ref()
            .map(|ntfy| {
                NtfyToken::parse(
                    lookup(&ntfy.token_env)
                        .ok_or_else(|| anyhow::anyhow!("ntfy publisher token is unavailable"))?,
                )
            })
            .transpose()?;
        let secrets = Self {
            ari_password: required(&config.ari.password_env, "lab-ari-password")?,
            sip_password: required(&config.sip.password_env, "lab-sip-password")?,
            api_token,
            openai_key: lookup(&config.ai.key_env),
            webhook_secret: lookup(&config.notifications.webhook_secret_env),
            vapid_pem,
            ntfy_token,
            nas_credentials: config
                .recording
                .as_ref()
                .and_then(|recording| recording.nas.as_ref())
                .map(|nas| {
                    NasCredentials::parse(
                        lookup(&nas.username_env)
                            .ok_or_else(|| anyhow::anyhow!("NAS username is unavailable"))?,
                        lookup(&nas.password_env)
                            .ok_or_else(|| anyhow::anyhow!("NAS password is unavailable"))?,
                    )
                })
                .transpose()?,
        };
        anyhow::ensure!(
            config.notifications.webhook_url.is_none() || secrets.webhook_secret.is_some(),
            "a webhook requires a signing secret"
        );
        Ok(secrets)
    }
}

pub fn parse_file(body: &str) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let mut values = std::collections::BTreeMap::new();
    for line in body
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("secret file requires KEY=VALUE entries"))?;
        anyhow::ensure!(
            !name.is_empty()
                && name.len() <= MAX_SECRET_NAME_BYTES
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'),
            "invalid secret variable name"
        );
        anyhow::ensure!(
            values.insert(name.into(), value.into()).is_none(),
            "duplicate secret variable"
        );
    }
    Ok(values)
}
