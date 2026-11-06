const DELIVERY_TIMEOUT_SECONDS: u64 = 5;

use crate::{
    config::Config,
    model::{Delivery, Event},
    secrets::Secrets,
    store::Store,
};
use anyhow::ensure;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio_util::sync::CancellationToken;

pub struct Notifier {
    config: Config,
    store: Store,
    secrets: Secrets,
    client: reqwest::Client,
}
impl Notifier {
    pub fn new(config: Config, store: Store, secrets: Secrets) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            store,
            secrets,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(DELIVERY_TIMEOUT_SECONDS))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }
    pub async fn pump(&self, now: u64) -> anyhow::Result<()> {
        for delivery in self.store.due(now).await? {
            if self.deliver(&delivery, now).await.is_ok() {
                self.store.delivered(delivery.id).await?;
            } else {
                self.store.retry(delivery.id, now).await?;
            }
        }
        Ok(())
    }
    pub async fn run(self, stop: CancellationToken) {
        loop {
            tokio::select! { _ = stop.cancelled() => return, result = self.pump(crate::controller::now()) => { if result.is_err() { tracing::warn!(operation="notifications",code="outbox_unavailable"); } } }
            tokio::select! { _ = stop.cancelled() => return, _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {} }
        }
    }
    async fn deliver(&self, delivery: &Delivery, now: u64) -> anyhow::Result<()> {
        let event: Event = serde_json::from_str(&delivery.payload)?;
        if !matches!(
            event.kind.as_str(),
            "incoming" | "ringing" | "voicemail" | "ended" | "complete"
        ) {
            return Ok(());
        }
        let settings = self.store.settings().await?;
        if let Some(url) = settings.webhook_url.as_ref().or(self
            .config
            .notifications
            .webhook_url
            .as_ref())
        {
            let secret = self
                .secrets
                .webhook_secret
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("webhook signing secret is unavailable"))?;
            let response = self
                .client
                .post(url)
                .header("content-type", "application/json")
                .header("x-homephone-event-id", delivery.id.to_string())
                .header("x-homephone-timestamp", now.to_string())
                .header(
                    "x-homephone-signature",
                    signature(secret, delivery.id, now, &delivery.payload)?,
                )
                .body(delivery.payload.clone())
                .send()
                .await?;
            ensure!(response.status().is_success(), "webhook delivery failed");
        }
        if let Some(pem) = &self.secrets.vapid_pem {
            for subscription in self.store.subscriptions().await? {
                let payload = serde_json::to_vec(
                    &serde_json::json!({"id":delivery.id,"call_id":event.call.as_ref().map(|call|call.id),"title":"Home Telephone","body":if event.kind == "incoming" || event.kind == "ringing" { "固定電話に着信があります。" } else { "固定電話の通話履歴を更新しました。" }}),
                )?;
                let encoded = crate::push::encode(
                    pem,
                    &self.config.notifications.vapid_subject,
                    &subscription,
                    &payload,
                    now,
                )?;
                let request = self
                    .client
                    .post(&subscription.endpoint)
                    .header("authorization", encoded.authorization)
                    .header("content-encoding", "aes128gcm")
                    .header("content-type", "application/octet-stream")
                    .header("ttl", "60")
                    .body(encoded.body);
                let response = request.send().await?;
                if matches!(
                    response.status(),
                    reqwest::StatusCode::GONE | reqwest::StatusCode::NOT_FOUND
                ) {
                    self.store.unsubscribe(subscription.endpoint).await?;
                } else {
                    ensure!(
                        response.status().is_success(),
                        "browser push delivery failed"
                    );
                }
            }
        }
        Ok(())
    }
}

pub fn public_key(pem: Option<&str>) -> Option<String> {
    pem.and_then(|pem| crate::push::public_key(pem).ok())
}

pub fn signature(secret: &str, id: i64, timestamp: u64, body: &str) -> anyhow::Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    mac.update(format!("{id}.{timestamp}.").as_bytes());
    mac.update(body.as_bytes());
    let digest = mac.finalize().into_bytes();
    Ok(format!(
        "v1={}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}
