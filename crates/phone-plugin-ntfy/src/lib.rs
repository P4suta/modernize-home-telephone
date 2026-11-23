use anyhow::{Context, ensure};

const MAX_NTFY_RESPONSE_BYTES: usize = 16_384;
mod http_body;

pub struct Ntfy {
    client: reqwest::Client,
    server: url::Url,
    topic: String,
    token: String,
    origin: url::Url,
}

impl Ntfy {
    pub fn new(
        server: &str,
        topic: &str,
        token: &str,
        origin: &str,
        allow_loopback: bool,
    ) -> anyhow::Result<Self> {
        let server = url::Url::parse(server)?;
        let local = match server.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        ensure!(
            (server.scheme() == "https" || (allow_loopback && server.scheme() == "http" && local))
                && server.host_str().is_some()
                && server.username().is_empty()
                && server.password().is_none()
                && server.path() == "/"
                && server.query().is_none()
                && server.fragment().is_none(),
            "invalid ntfy origin"
        );
        ensure!(
            !topic.is_empty()
                && topic.len() <= 64
                && topic
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
            "invalid ntfy topic"
        );
        ensure!(
            !token.is_empty() && token.len() <= 4096 && token.bytes().all(|b| b.is_ascii_graphic()),
            "invalid ntfy token"
        );
        let origin = url::Url::parse(origin)?;
        ensure!(
            matches!(origin.scheme(), "http" | "https")
                && origin.host_str().is_some()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "invalid console origin"
        );
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            server,
            topic: topic.into(),
            token: token.into(),
            origin,
        })
    }

    pub async fn publish(
        &self,
        id: uuid::Uuid,
        caller: &str,
        started_at: u64,
    ) -> anyhow::Result<()> {
        let caller = if phone_core::DialNumber::parse(caller.as_bytes()).is_ok() {
            caller
        } else {
            "非通知・番号取得不可"
        };
        let timestamp = time::OffsetDateTime::from_unix_timestamp(i64::try_from(started_at)?)?
            .checked_to_offset(time::UtcOffset::from_hms(9, 0, 0)?)
            .context("incoming call time is outside the supported range")?
            .format(&time::format_description::well_known::Rfc3339)?;
        let sequence_id = format!("homephone-{}", id);
        let payload = serde_json::json!({
            "topic": self.topic,
            "title": "固定電話に着信",
            "message": format!("発信者番号: {caller}\n着信時刻: {timestamp}"),
            "priority": 4,
            "tags": ["telephone_receiver"],
            "click": self.origin.as_str(),
            "sequence_id": sequence_id,
        });
        let response = self
            .client
            .post(self.server.clone())
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?;
        ensure!(response.status().is_success(), "ntfy publication failed");
        #[derive(serde::Deserialize)]
        struct Published {
            id: String,
            event: String,
            topic: String,
            sequence_id: String,
        }
        let accepted: Published = crate::http_body::json(response, MAX_NTFY_RESPONSE_BYTES).await?;
        ensure!(
            !accepted.id.is_empty()
                && accepted.id.len() <= 64
                && accepted.id.bytes().all(|byte| byte.is_ascii_alphanumeric())
                && accepted.event == "message"
                && accepted.topic == self.topic
                && accepted.sequence_id == sequence_id,
            "ntfy did not acknowledge the selected notification"
        );
        Ok(())
    }
}
