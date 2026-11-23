const DELIVERY_TIMEOUT_SECONDS: u64 = 5;

use crate::{
    config::Config,
    model::{Delivery, Event, NotificationReceipt},
    secrets::Secrets,
    store::Store,
};
#[cfg(feature = "plugin-ntfy")]
use anyhow::Context;
use anyhow::ensure;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tokio_util::sync::CancellationToken;

pub struct Notifier {
    config: Config,
    store: Store,
    secrets: Secrets,
    client: reqwest::Client,
    #[cfg(feature = "plugin-ntfy")]
    ntfy: Option<phone_plugin_ntfy::Ntfy>,
}
impl Notifier {
    pub fn new(config: Config, store: Store, secrets: Secrets) -> anyhow::Result<Self> {
        config.validate()?;
        ensure!(
            config.notifications.ntfy.is_none() || secrets.ntfy_token.is_some(),
            "ntfy publisher token is unavailable"
        );
        #[cfg(feature = "plugin-ntfy")]
        let ntfy = config
            .notifications
            .ntfy
            .as_ref()
            .map(|target| {
                phone_plugin_ntfy::Ntfy::new(
                    &target.server,
                    &target.topic,
                    secrets
                        .ntfy_token
                        .as_ref()
                        .context("ntfy publisher token is unavailable")?
                        .value(),
                    &config.public_origin,
                    config.mode == crate::config::Mode::Lab,
                )
            })
            .transpose()?;
        Ok(Self {
            #[cfg(feature = "plugin-ntfy")]
            ntfy,
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
        ensure!(
            event.id == delivery.id,
            "notification event identity differs from its outbox entry"
        );
        let ntfy = self.ntfy(&event, now).await;
        let webhook_and_push = self.webhook_and_push(delivery, &event, now).await;
        ntfy.and(webhook_and_push)
    }

    async fn webhook_and_push(
        &self,
        delivery: &Delivery,
        event: &Event,
        now: u64,
    ) -> anyhow::Result<()> {
        if !matches!(
            event.kind.as_str(),
            "incoming" | "ringing" | "voicemail" | "ended" | "complete"
        ) {
            return Ok(());
        }
        let receipt = NotificationReceipt::WebhookAndPush(delivery.id);
        if self.store.notification_sent(receipt).await? {
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
        self.store.record_notification(receipt, delivery.id).await
    }

    #[cfg(feature = "plugin-ntfy")]
    async fn ntfy(&self, event: &Event, now: u64) -> anyhow::Result<()> {
        use phone_core::notification::{CallNotification, Direction, EventKind, Publication};
        let Some(plugin) = &self.ntfy else {
            return Ok(());
        };
        let Some(call) = &event.call else {
            return Ok(());
        };
        let notification = CallNotification {
            event: EventKind::from_kind(&event.kind),
            direction: Direction::from_name(&call.direction),
            phase: call.phase.into(),
            started_at: call.started_at,
            created_at: event.created_at,
        };
        if notification.incoming(now) == Publication::Skip {
            return Ok(());
        }
        let receipt = NotificationReceipt::Ntfy(call.id);
        if self.store.notification_sent(receipt).await? {
            return Ok(());
        }
        let caller_label = call
            .caller_label
            .as_deref()
            .and_then(|label| phone_core::directory::CallerLabelText::new(label).ok());
        plugin
            .publish(call.id, &call.caller, caller_label, call.started_at)
            .await?;
        self.store.record_notification(receipt, event.id).await
    }

    #[cfg(not(feature = "plugin-ntfy"))]
    async fn ntfy(&self, _event: &Event, _now: u64) -> anyhow::Result<()> {
        ensure!(
            self.config.notifications.ntfy.is_none(),
            "ntfy plugin is not installed"
        );
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
