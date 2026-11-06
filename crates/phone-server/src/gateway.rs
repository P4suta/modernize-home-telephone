const MAX_CHANNEL_ID_BYTES: usize = 256;
const MAX_LIVE_CHANNELS: usize = 128;
const ARI_COMMAND_TIMEOUT_SECONDS: u64 = 3;
const MAX_ARI_RESPONSE_BYTES: usize = 1048576;
const WEBSOCKET_CONNECT_SECONDS: u64 = 5;
const MAX_WEBSOCKET_BYTES: usize = 524288;
const RECORDING_REQUEST_SECONDS: u64 = 120;

use crate::config::Ari as AriConfig;
use anyhow::{Context, ensure};
use reqwest::Method;
use serde::{Deserialize, de::DeserializeOwned};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig},
};

pub type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Clone)]
pub struct Ari {
    config: AriConfig,
    password: String,
    client: reqwest::Client,
    stasis: tokio::sync::watch::Sender<std::collections::BTreeSet<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Channel {
    pub id: String,
    pub state: String,
    #[serde(default)]
    pub caller: Caller,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Caller {
    #[serde(default)]
    pub number: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub enum AriEvent {
    StasisStart {
        channel: Channel,
        args: Vec<String>,
    },
    StasisEnd {
        channel: Channel,
    },
    ChannelDestroyed {
        channel: Channel,
    },
    ChannelStateChange {
        channel: Channel,
    },
    #[serde(other)]
    Other,
}
#[derive(Deserialize)]
pub struct Variable {
    pub value: String,
}

impl Ari {
    pub fn observe(&self, event: &AriEvent) -> anyhow::Result<()> {
        if let AriEvent::StasisStart { channel, .. }
        | AriEvent::StasisEnd { channel }
        | AriEvent::ChannelDestroyed { channel } = event
        {
            ensure!(
                phone_core::bytes::nonempty_within(channel.id.len(), MAX_CHANNEL_ID_BYTES)
                    && !channel.id.chars().any(char::is_control),
                "invalid Stasis channel identity"
            );
        }
        let mut overflow = false;
        self.stasis.send_modify(|channels| match event {
            AriEvent::StasisStart { channel, .. } => {
                if channels.len() >= MAX_LIVE_CHANNELS && !channels.contains(&channel.id) {
                    overflow = true;
                } else {
                    channels.insert(channel.id.clone());
                }
            }
            AriEvent::StasisEnd { channel } | AriEvent::ChannelDestroyed { channel } => {
                channels.remove(&channel.id);
            }
            AriEvent::ChannelStateChange { .. } | AriEvent::Other => {}
        });
        ensure!(!overflow, "Stasis channel inventory exceeded its bound");
        Ok(())
    }
    pub fn reset_events(&self) {
        self.stasis.send_replace(Default::default());
    }
    pub async fn wait_for_stasis(&self, id: &str) -> anyhow::Result<()> {
        let mut channels = self.stasis.subscribe();
        tokio::time::timeout(
            std::time::Duration::from_secs(ARI_COMMAND_TIMEOUT_SECONDS),
            async {
                while !channels.borrow_and_update().contains(id) {
                    channels.changed().await?;
                }
                Ok::<_, anyhow::Error>(())
            },
        )
        .await?
    }
    pub fn new(config: AriConfig, password: String) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            password,
            stasis: tokio::sync::watch::channel(Default::default()).0,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(ARI_COMMAND_TIMEOUT_SECONDS))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }

    pub fn application(&self) -> &str {
        &self.config.application
    }

    fn resource(&self, segments: &[&str]) -> anyhow::Result<url::Url> {
        let mut url = url::Url::parse(&self.config.url)?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid ARI base URL"))?;
            path.pop_if_empty();
            for segment in segments {
                path.push(segment);
            }
        }
        Ok(url)
    }

    async fn response(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> anyhow::Result<reqwest::Response> {
        let mut url = self.resource(segments)?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        self.client
            .request(method, url)
            .basic_auth(&self.config.username, Some(&self.password))
            .send()
            .await
            .context("ARI transport failed")
    }

    pub async fn command(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> anyhow::Result<()> {
        let cleanup = method == Method::DELETE;
        let response = self.response(method, segments, query).await?;
        if cleanup && response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        ensure!(
            response.status().is_success(),
            "ARI command failed with HTTP {}",
            response.status().as_u16()
        );
        Ok(())
    }

    pub async fn cleanup(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> anyhow::Result<()> {
        let response = self.response(method, segments, query).await?;
        ensure!(
            response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND,
            "ARI cleanup failed with HTTP {}",
            response.status().as_u16()
        );
        Ok(())
    }

    pub async fn read<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        let response = self.response(Method::GET, segments, query).await?;
        ensure!(
            response.status().is_success(),
            "ARI read failed with HTTP {}",
            response.status().as_u16()
        );
        crate::http_body::json(response, MAX_ARI_RESPONSE_BYTES)
            .await
            .context("invalid ARI response")
    }

    pub async fn create<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        let response = self.response(Method::POST, segments, query).await?;
        ensure!(
            response.status().is_success(),
            "ARI creation failed with HTTP {}",
            response.status().as_u16()
        );
        crate::http_body::json(response, MAX_ARI_RESPONSE_BYTES)
            .await
            .context("invalid ARI response")
    }

    pub async fn connect_events(&self) -> anyhow::Result<Socket> {
        let mut url = self.resource(&["events"])?;
        url.query_pairs_mut()
            .append_pair("app", &self.config.application);
        self.socket(url, None).await
    }

    pub async fn connect_media(&self, connection: &str) -> anyhow::Result<Socket> {
        ensure!(
            uuid::Uuid::parse_str(connection).is_ok(),
            "invalid media connection identity"
        );
        let mut url = url::Url::parse(&self.config.url)?;
        url.set_path(&format!("/media/{connection}"));
        self.socket(url, None).await
    }

    pub async fn connect_sip(&self) -> anyhow::Result<Socket> {
        let mut url = url::Url::parse(&self.config.url)?;
        url.set_path("/ws");
        self.socket(url, Some("sip")).await
    }

    async fn socket(&self, mut url: url::Url, protocol: Option<&str>) -> anyhow::Result<Socket> {
        use base64::Engine as _;
        let scheme = match url.scheme() {
            "http" => "ws",
            "https" => "wss",
            _ => anyhow::bail!("invalid websocket scheme"),
        };
        url.set_scheme(scheme)
            .map_err(|_| anyhow::anyhow!("invalid websocket URL"))?;
        let mut request = url.as_str().into_client_request()?;
        let authorization = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", self.config.username, self.password))
        );
        let mut authorization = HeaderValue::from_str(&authorization)?;
        authorization.set_sensitive(true);
        request.headers_mut().insert("authorization", authorization);
        if let Some(protocol) = protocol {
            request
                .headers_mut()
                .insert("sec-websocket-protocol", HeaderValue::from_str(protocol)?);
        }
        let (socket, response) = tokio::time::timeout(
            std::time::Duration::from_secs(WEBSOCKET_CONNECT_SECONDS),
            tokio_tungstenite::connect_async_with_config(
                request,
                Some(
                    WebSocketConfig::default()
                        .max_message_size(Some(MAX_WEBSOCKET_BYTES))
                        .max_frame_size(Some(MAX_WEBSOCKET_BYTES)),
                ),
                false,
            ),
        )
        .await??;
        if let Some(protocol) = protocol {
            ensure!(
                response
                    .headers()
                    .get("sec-websocket-protocol")
                    .and_then(|v| v.to_str().ok())
                    == Some(protocol),
                "upstream refused SIP subprotocol"
            );
        }
        Ok(socket)
    }

    pub async fn recording(&self, name: &str) -> anyhow::Result<reqwest::Response> {
        let response = self
            .client
            .get(self.resource(&["recordings", "stored", name, "file"])?)
            .basic_auth(&self.config.username, Some(&self.password))
            .timeout(std::time::Duration::from_secs(RECORDING_REQUEST_SECONDS))
            .send()
            .await?;
        ensure!(response.status().is_success(), "recording download failed");
        Ok(response)
    }
}
