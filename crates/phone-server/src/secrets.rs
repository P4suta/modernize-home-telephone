const MIN_API_TOKEN_BYTES: usize = 32;
const MAX_SECRET_NAME_BYTES: usize = 128;

use crate::config::{Config, Mode};

#[derive(Clone)]
pub struct Secrets {
    pub ari_password: String,
    pub sip_password: String,
    pub api_token: Option<String>,
    pub openai_key: Option<String>,
    pub webhook_secret: Option<String>,
    pub vapid_pem: Option<String>,
}

impl Secrets {
    pub fn load(config: &Config) -> anyhow::Result<Self> {
        Self::load_with_file(config, None)
    }
    pub fn load_with_file(config: &Config, path: Option<&std::path::Path>) -> anyhow::Result<Self> {
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
        Self::from_lookup(config, lookup, path.and_then(|path| path.parent()))
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
        let secrets = Self {
            ari_password: required(&config.ari.password_env, "lab-ari-password")?,
            sip_password: required(&config.sip.password_env, "lab-sip-password")?,
            api_token,
            openai_key: lookup(&config.ai.key_env),
            webhook_secret: lookup(&config.notifications.webhook_secret_env),
            vapid_pem,
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
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
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
