const CONTROL_QUEUE_CAPACITY: usize = 128;
const CONTROL_INITIALIZATION_SECONDS: u64 = 5;
const MAX_DTMF_DIGITS: usize = 32;
const MAX_SPEAK_CHARACTERS: usize = 500;
const MAX_TRANSCRIPT_ITEMS: usize = 256;
const MAX_TRANSCRIPT_BYTES: usize = 262144;

use crate::{
    config::{Config, valid_number},
    gateway::{Ari, AriEvent, Channel},
    media::{Notice, Purpose, Report},
    model::{Call, Command, Event, Phase, Recording, Transcript},
    provider,
    store::Store,
};
use futures_util::StreamExt;
use phone_core::{Action, CallId, Line, Route, initial_route};
use reqwest::Method;
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("external line is busy")]
    Busy,
    #[error("call does not exist")]
    NotFound,
    #[error("operation is not allowed in this call state")]
    InvalidTransition,
    #[error("invalid command input")]
    InvalidInput,
    #[error("telephone gateway is unavailable")]
    Unavailable,
    #[error("telephone operation failed")]
    Internal(#[from] anyhow::Error),
}
type Reply = oneshot::Sender<Result<Call, Failure>>;

enum Request {
    Offer {
        channel: Channel,
        direction: String,
        reply: Reply,
    },
    Command {
        id: Uuid,
        command: Command,
        reply: Reply,
    },
    Originate {
        number: String,
        reply: Reply,
    },
    Event(AriEvent),
    Connection(bool),
    Maintenance {
        enabled: bool,
        reply: oneshot::Sender<Result<(), Failure>>,
    },
    Deletable {
        id: Uuid,
        reply: oneshot::Sender<Result<(), Failure>>,
    },
    Shutdown(oneshot::Sender<()>),
}

struct Interface {
    sender: mpsc::Sender<Request>,
    ready: watch::Receiver<bool>,
    initialized: watch::Receiver<bool>,
    stop: CancellationToken,
}
impl Drop for Interface {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[derive(Clone)]
pub struct Control {
    pub store: Store,
    pub events: broadcast::Sender<Event>,
    inner: Arc<Interface>,
}

impl Control {
    pub fn start(config: Config, ari: Ari, store: Store) -> Self {
        Self::start_with_key(config, ari, store, None)
    }
    pub fn start_with_key(config: Config, ari: Ari, store: Store, key: Option<String>) -> Self {
        Self::start_quiesced(config, ari, store, key, false)
    }
    pub fn start_quiesced(
        config: Config,
        ari: Ari,
        store: Store,
        key: Option<String>,
        maintenance: bool,
    ) -> Self {
        store.pause_background(true);
        let (sender, receiver) = mpsc::channel(CONTROL_QUEUE_CAPACITY);
        let (ready, readiness) = watch::channel(false);
        let (initialized, initialization) = watch::channel(false);
        let events = store.events.clone();
        let stop = CancellationToken::new();
        let (reports, reported) = mpsc::channel(CONTROL_QUEUE_CAPACITY);
        let control = Self {
            store: store.clone(),
            events,
            inner: Arc::new(Interface {
                sender: sender.clone(),
                ready: readiness,
                initialized: initialization,
                stop: stop.clone(),
            }),
        };
        let actor = Actor {
            config,
            ari: ari.clone(),
            store,
            ready,
            initialized,
            active: None,
            line: Line::default(),
            counter: 0,
            connected: false,
            key,
            reports,
            maintenance,
        };
        tokio::spawn(actor.run(receiver, reported, stop.clone()));
        tokio::spawn(drive_events(ari, sender, stop));
        control
    }
    pub fn ready(&self) -> bool {
        *self.inner.ready.borrow() && !self.inner.sender.is_closed()
    }
    pub fn readiness(&self) -> watch::Receiver<bool> {
        self.inner.ready.clone()
    }
    pub fn initialized(&self) -> bool {
        *self.inner.initialized.borrow() && !self.inner.sender.is_closed()
    }
    pub async fn deletable(&self, id: Uuid) -> Result<(), Failure> {
        let (reply, result) = oneshot::channel();
        self.inner
            .sender
            .send(Request::Deletable { id, reply })
            .await
            .map_err(|_| Failure::Unavailable)?;
        result.await.map_err(|_| Failure::Unavailable)?
    }
    pub async fn maintenance(&self, enabled: bool) -> Result<(), Failure> {
        let (reply, result) = oneshot::channel();
        self.inner
            .sender
            .send(Request::Maintenance { enabled, reply })
            .await
            .map_err(|_| Failure::Unavailable)?;
        result.await.map_err(|_| Failure::Unavailable)?
    }
    pub async fn wait_ready(&self) -> anyhow::Result<()> {
        let mut readiness = self.inner.ready.clone();
        tokio::time::timeout(Duration::from_secs(CONTROL_INITIALIZATION_SECONDS), async {
            while !*readiness.borrow_and_update() {
                readiness.changed().await?;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }
    pub async fn offer(&self, channel: Channel, direction: String) -> Result<Call, Failure> {
        let (reply, result) = oneshot::channel();
        self.inner
            .sender
            .send(Request::Offer {
                channel,
                direction,
                reply,
            })
            .await
            .map_err(|_| Failure::Unavailable)?;
        result.await.map_err(|_| Failure::Unavailable)?
    }
    pub async fn command(&self, id: Uuid, command: Command) -> Result<Call, Failure> {
        let (reply, result) = oneshot::channel();
        self.inner
            .sender
            .send(Request::Command { id, command, reply })
            .await
            .map_err(|_| Failure::Unavailable)?;
        result.await.map_err(|_| Failure::Unavailable)?
    }
    pub async fn originate(&self, number: String) -> Result<Call, Failure> {
        let (reply, result) = oneshot::channel();
        self.inner
            .sender
            .send(Request::Originate { number, reply })
            .await
            .map_err(|_| Failure::Unavailable)?;
        result.await.map_err(|_| Failure::Unavailable)?
    }
    pub async fn shutdown(&self) {
        let (reply, result) = oneshot::channel();
        if self
            .inner
            .sender
            .send(Request::Shutdown(reply))
            .await
            .is_ok()
            && result.await.is_err()
        {
            tracing::debug!(operation = "controller", code = "shutdown_receiver_closed");
        }
        self.inner.stop.cancel();
    }
}

struct Active {
    call: Call,
    owner: CallId,
    root: String,
    bridge: String,
    peers: Vec<String>,
    winner: Option<String>,
    entered: tokio::time::Instant,
    root_link: RootLink,
    media: Option<provider::Handle>,
    ended_persisted: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum RootLink {
    Ringing,
    Answered,
    Bridged,
}
struct Actor {
    config: Config,
    ari: Ari,
    store: Store,
    ready: watch::Sender<bool>,
    initialized: watch::Sender<bool>,
    connected: bool,
    active: Option<Active>,
    line: Line,
    counter: u64,
    key: Option<String>,
    reports: mpsc::Sender<Report>,
    maintenance: bool,
}
impl Actor {
    async fn run(
        mut self,
        mut receiver: mpsc::Receiver<Request>,
        mut reported: mpsc::Receiver<Report>,
        stop: CancellationToken,
    ) {
        if self.store.recover(now()).await.is_err() {
            tracing::error!(operation = "startup", code = "storage_recovery_failed");
            return;
        }
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = stop.cancelled() => { let _ = self.end("service_shutdown").await; break; }
                Some(report) = reported.recv() => { if self.report(report).await.is_err() { let _ = self.end("media_operation_failed").await; } }
                _ = tick.tick() => { if self.tick().await.is_err() { tracing::warn!(operation="tick", code="call_maintenance_failed"); } }
                request = receiver.recv() => {
                    match request {
                        Some(Request::Offer { channel, direction, reply }) => { let result = self.offer(channel, direction).await; let _ = reply.send(result); }
                        Some(Request::Command { id, command, reply }) => { let result = self.command(id, command).await; let _ = reply.send(result); }
                        Some(Request::Originate { number, reply }) => { let result = self.originate(number).await; let _ = reply.send(result); }
                        Some(Request::Event(event)) => { if self.event(event).await.is_err() { tracing::warn!(operation="gateway_event", code="event_failed"); } }
                        Some(Request::Connection(connected)) => {
                            self.connected = connected;
                            self.store.pause_background(true);
                            self.ready.send_replace(false);
                            self.initialized.send_replace(false);
                            if connected {
                                if self.reconcile().await.is_ok() && !self.maintenance { self.ready.send_replace(true); self.store.pause_background(false); }
                            } else { let _ = self.end("gateway_disconnected").await; }
                        }
                        Some(Request::Shutdown(reply)) => { let _ = self.end("service_shutdown").await; let _ = reply.send(()); break; }
                        Some(Request::Maintenance { enabled, reply }) => {
                            let result = if self.active.is_some() && self.maintenance != enabled { Err(Failure::Busy) } else if !enabled && (!self.connected || (!*self.initialized.borrow() && self.reconcile().await.is_err())) { Err(Failure::Unavailable) } else {
                                self.maintenance = enabled;
                                self.store.pause_background(enabled || !*self.initialized.borrow());
                                self.ready.send_replace(!enabled && *self.initialized.borrow());
                                Ok(())
                            };
                            let _ = reply.send(result);
                        }
                        Some(Request::Deletable { id, reply }) => {
                            let result=if self.active.as_ref().is_some_and(|active|active.call.id==id) {Err(Failure::Busy)} else {Ok(())};
                            let _=reply.send(result);
                        }
                        None => break,
                    }
                }
            }
        }
    }

    async fn publish(&self, call: &Call, kind: &str) -> anyhow::Result<()> {
        let event = self.store.save(call, kind, now()).await?;
        tracing::info!(operation="call_state",call_id=%call.id,phase=?call.phase,event=kind,event_id=event.id);
        Ok(())
    }

    async fn reconcile(&mut self) -> anyhow::Result<()> {
        let channels: Vec<Channel> = self.ari.read(&["channels"], &[]).await?;
        #[derive(serde::Deserialize)]
        struct Application {
            channel_ids: Vec<String>,
        }
        let application: Application = self
            .ari
            .read(&["applications", self.ari.application()], &[])
            .await?;
        let resources = self.store.resources().await?;
        let mut owned: std::collections::BTreeSet<String> =
            application.channel_ids.into_iter().collect();
        owned.extend(resources.iter().cloned());
        for channel in channels {
            if channel.id.starts_with("hp-") || resources.iter().any(|id| id == &channel.id) {
                owned.insert(channel.id);
            }
        }
        for id in owned {
            self.ari
                .command(Method::DELETE, &["channels", &id], &[])
                .await?;
        }
        #[derive(serde::Deserialize)]
        struct Bridge {
            id: String,
        }
        let bridges: Vec<Bridge> = self.ari.read(&["bridges"], &[]).await?;
        for bridge in bridges {
            if bridge.id.starts_with("hp-") {
                self.ari
                    .command(Method::DELETE, &["bridges", &bridge.id], &[])
                    .await?;
            }
        }
        self.store.clear_resources().await?;
        self.active = None;
        self.store.own_line(None);
        self.line = Line::default();
        self.initialized.send_replace(true);
        Ok(())
    }

    async fn admit(&mut self, call: Call, root: String) -> Result<(), Failure> {
        if !*self.ready.borrow() {
            return Err(Failure::Unavailable);
        }
        self.counter = self.counter.checked_add(1).ok_or(Failure::Unavailable)?;
        let owner = CallId::new(self.counter).map_err(|_| Failure::Unavailable)?;
        let line = self.line.acquire(owner).map_err(|_| Failure::Busy)?;
        self.store.track_resource(&root).await?;
        self.publish(&call, "incoming").await?;
        self.store.own_line(Some(call.id));
        self.line = line;
        self.active = Some(Active {
            bridge: format!("hp-{}", call.id),
            call,
            owner,
            root: root.clone(),
            peers: vec![],
            winner: None,
            entered: tokio::time::Instant::now(),
            root_link: RootLink::Ringing,
            media: None,
            ended_persisted: false,
        });
        let bridge = self
            .active
            .as_ref()
            .ok_or(Failure::NotFound)?
            .bridge
            .clone();
        if self
            .ari
            .command(
                Method::POST,
                &["bridges", &bridge],
                &[("type", "mixing,proxy_media")],
            )
            .await
            .is_err()
        {
            if (self.end("gateway_failure").await).is_err() {
                tracing::warn!(
                    operation = "controller",
                    code = "gateway_failure_cleanup_failed"
                );
            }
            return Err(Failure::Unavailable);
        }
        Ok(())
    }

    async fn offer(&mut self, channel: Channel, direction: String) -> Result<Call, Failure> {
        if !*self.ready.borrow() {
            if (self
                .ari
                .command(
                    Method::DELETE,
                    &["channels", &channel.id],
                    &[("reason", "congestion")],
                )
                .await)
                .is_err()
            {
                tracing::warn!(
                    operation = "controller",
                    code = "busy_channel_cleanup_failed"
                );
            }
            return Err(Failure::Unavailable);
        }
        if let Some(active) = &self.active
            && active.root == channel.id
            && active.call.phase != Phase::Ended
        {
            return Ok(active.call.clone());
        }
        if self.line.owner().is_some() {
            if (self
                .ari
                .command(Method::DELETE, &["channels", &channel.id], &[])
                .await)
                .is_err()
            {
                tracing::warn!(
                    operation = "controller",
                    code = "unowned_channel_cleanup_failed"
                );
            }
            return Err(Failure::Busy);
        }
        let caller = if direction == "inbound" && !valid_number(&channel.caller.number) {
            "anonymous".into()
        } else {
            channel.caller.number
        };
        let call = Call::offered(caller, direction.clone(), now());
        let id = call.id;
        if let Err(error) = self.admit(call, channel.id.clone()).await {
            if self.active.is_some() {
                if (self.end("admission_failed").await).is_err() {
                    tracing::warn!(operation = "controller", code = "admission_cleanup_failed");
                }
            } else {
                self.ready.send_replace(false);
                self.initialized.send_replace(false);
                self.store.pause_background(true);
                if self
                    .ari
                    .command(Method::DELETE, &["channels", &channel.id], &[])
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        operation = "admission",
                        code = "external_channel_cleanup_pending"
                    );
                }
            }
            return Err(error);
        }
        let result = if direction == "outbound" {
            self.dial_carrier().await
        } else {
            let active = self.active.as_ref().ok_or(Failure::NotFound)?;
            let rule = self
                .store
                .settings()
                .await?
                .rules
                .get(&active.call.caller)
                .copied()
                .unwrap_or_default();
            match initial_route(rule.into()) {
                Route::Ring => self.ring(false).await,
                Route::End => self.end("number_blocked").await.map(|_| ()),
                Route::Screen => self.screen().await,
                Route::Continue | Route::Voicemail => Err(Failure::InvalidTransition),
            }
        };
        if let Err(error) = result {
            if (self.end("operation_failed").await).is_err() {
                tracing::warn!(operation = "controller", code = "operation_cleanup_failed");
            }
            return Err(error);
        }
        self.store.get(id).await?.ok_or(Failure::NotFound)
    }

    async fn transition(&mut self, action: Action, kind: &str) -> Result<Call, Failure> {
        let active = self.active.as_mut().ok_or(Failure::NotFound)?;
        let phase = phone_core::Phase::from(active.call.phase)
            .apply(action)
            .map_err(|_| Failure::InvalidTransition)?;
        active.call.phase = phase.into();
        active.entered = tokio::time::Instant::now();
        let call = active.call.clone();
        self.publish(&call, kind).await?;
        Ok(call)
    }

    async fn answer_and_record(&mut self) -> Result<(), Failure> {
        let active = self.active.as_ref().ok_or(Failure::NotFound)?;
        let root = active.root.clone();
        let bridge = active.bridge.clone();
        let record = active.call.recording == Recording::None;
        let root_link = active.root_link;
        if root_link == RootLink::Ringing {
            self.ari
                .command(Method::POST, &["channels", &root, "answer"], &[])
                .await?;
            self.active.as_mut().ok_or(Failure::NotFound)?.root_link = RootLink::Answered;
        }
        if root_link != RootLink::Bridged {
            self.ari
                .command(
                    Method::POST,
                    &["bridges", &bridge, "addChannel"],
                    &[("channel", &root)],
                )
                .await?;
            self.active.as_mut().ok_or(Failure::NotFound)?.root_link = RootLink::Bridged;
        }
        if record {
            let name = format!(
                "hp-{}",
                self.active.as_ref().ok_or(Failure::NotFound)?.call.id
            );
            match self
                .ari
                .command(
                    Method::POST,
                    &["bridges", &bridge, "record"],
                    &[
                        ("name", &name),
                        ("format", "wav"),
                        ("ifExists", "overwrite"),
                        ("beep", "false"),
                    ],
                )
                .await
            {
                Ok(()) => {
                    if let Some(active) = &mut self.active {
                        active.call.recording = Recording::Pending;
                    }
                }
                Err(_) => {
                    if let Some(active) = &mut self.active {
                        active.call.recording = Recording::Failed {
                            reason: "recording_unavailable".into(),
                        };
                    }
                }
            }
        }
        Ok(())
    }

    async fn screen(&mut self) -> Result<(), Failure> {
        self.answer_and_record().await?;
        self.transition(Action::Screen, "screening").await?;
        if let Err(error) = self.start_media(Purpose::Screening).await {
            let code = match &error {
                Failure::Internal(cause) => cause
                    .downcast_ref::<crate::media::StartupFault>()
                    .map_or_else(|| "media_start_failed".into(), ToString::to_string),
                _ => "media_start_failed".into(),
            };
            if let Some(active) = &mut self.active {
                tracing::warn!(operation="screening",call_id=%active.call.id,code);
                active.call.last_error = Some("screening_unavailable".into());
            }
            self.ring(false).await?;
        }
        Ok(())
    }

    async fn ring(&mut self, browser_only: bool) -> Result<(), Failure> {
        self.stop_media().await?;
        let active = self.active.as_ref().ok_or(Failure::NotFound)?;
        if active.call.phase != Phase::Ringing {
            self.transition(Action::Ring, "ringing").await?;
        }
        let active = self.active.as_ref().ok_or(Failure::NotFound)?;
        let root = active.root.clone();
        let bridge = active.bridge.clone();
        let call_id = active.call.id.to_string();
        let mut endpoints = vec![self.config.sip.browser_endpoint.clone()];
        if !browser_only {
            endpoints.push(self.config.sip.handset_endpoint.clone());
        }
        for (index, endpoint) in endpoints.into_iter().enumerate() {
            let id = format!("{bridge}-peer-{index}");
            if self
                .active
                .as_ref()
                .is_some_and(|active| active.peers.contains(&id))
            {
                continue;
            }
            self.store.track_resource(&id).await?;
            if let Some(active) = &mut self.active {
                active.peers.push(id.clone());
            }
            let args = format!("endpoint,{call_id}");
            let timeout = self.config.timers.ringing_seconds.to_string();
            if self
                .ari
                .command(
                    Method::POST,
                    &["channels"],
                    &[
                        ("endpoint", &endpoint),
                        ("channelId", &id),
                        ("app", self.ari.application()),
                        ("appArgs", &args),
                        ("timeout", &timeout),
                    ],
                )
                .await
                .is_err()
            {
                tracing::warn!(operation = "ring", code = "endpoint_unavailable");
            }
        }
        if (self
            .ari
            .command(Method::POST, &["channels", &root, "ring"], &[])
            .await)
            .is_err()
        {
            tracing::warn!(operation = "controller", code = "ring_indication_failed");
        }
        Ok(())
    }

    async fn originate(&mut self, number: String) -> Result<Call, Failure> {
        if !valid_number(&number) {
            return Err(Failure::InvalidInput);
        }
        if self.line.owner().is_some() {
            return Err(Failure::Busy);
        }
        let call = Call::offered(number.clone(), "outbound".into(), now());
        let root = format!("hp-{}-root", call.id);
        self.admit(call, root.clone()).await?;
        self.transition(Action::Ring, "dialing").await?;
        let args = format!("originator,{number}");
        if self
            .ari
            .command(
                Method::POST,
                &["channels"],
                &[
                    ("endpoint", &self.config.sip.browser_endpoint),
                    ("channelId", &root),
                    ("app", self.ari.application()),
                    ("appArgs", &args),
                ],
            )
            .await
            .is_err()
        {
            if (self.end("originate_failed").await).is_err() {
                tracing::warn!(
                    operation = "controller",
                    code = "origination_cleanup_failed"
                );
            }
            return Err(Failure::Unavailable);
        }
        Ok(self.active.as_ref().ok_or(Failure::NotFound)?.call.clone())
    }

    async fn dial_carrier(&mut self) -> Result<(), Failure> {
        let active = self.active.as_ref().ok_or(Failure::NotFound)?;
        if !valid_number(&active.call.caller) {
            return Err(Failure::InvalidInput);
        }
        let endpoint = format!(
            "PJSIP/{}@{}",
            active.call.caller, self.config.sip.carrier_endpoint
        );
        let id = format!("{}-carrier", active.bridge);
        let args = format!("endpoint,{}", active.call.id);
        if active.call.phase != Phase::Ringing {
            self.transition(Action::Ring, "dialing").await?;
        }
        self.store.track_resource(&id).await?;
        if let Some(active) = &mut self.active {
            active.peers.push(id.clone());
        }
        self.ari
            .command(
                Method::POST,
                &["channels"],
                &[
                    ("endpoint", &endpoint),
                    ("channelId", &id),
                    ("app", self.ari.application()),
                    ("appArgs", &args),
                ],
            )
            .await?;
        Ok(())
    }

    async fn command(&mut self, id: Uuid, command: Command) -> Result<Call, Failure> {
        let active = self
            .active
            .as_ref()
            .filter(|active| active.call.id == id)
            .ok_or(Failure::NotFound)?;
        if active.call.phase == Phase::Ended {
            return Err(Failure::InvalidTransition);
        }
        match command {
            Command::Hangup => self.end("local_hangup").await,
            Command::Answer | Command::Takeover => {
                self.ring(true).await?;
                Ok(self.active.as_ref().ok_or(Failure::NotFound)?.call.clone())
            }
            Command::Hold | Command::Resume => {
                let action = if matches!(command, Command::Hold) {
                    Action::Hold
                } else {
                    Action::Resume
                };
                phone_core::Phase::from(active.call.phase)
                    .apply(action)
                    .map_err(|_| Failure::InvalidTransition)?;
                let root = active.root.clone();
                let bridge = active.bridge.clone();
                let winner = active.winner.clone().ok_or(Failure::InvalidTransition)?;
                if action == Action::Hold {
                    self.stop_media().await?;
                }
                let method = if action == Action::Hold {
                    Method::POST
                } else {
                    Method::DELETE
                };
                let operation = if action == Action::Hold {
                    "removeChannel"
                } else {
                    "addChannel"
                };
                self.ari
                    .command(
                        Method::POST,
                        &["bridges", &bridge, operation],
                        &[("channel", &winner)],
                    )
                    .await?;
                self.ari
                    .command(method, &["channels", &root, "moh"], &[])
                    .await?;
                self.transition(action, "state").await
            }
            Command::Dtmf { digits } => {
                phone_core::Phase::from(active.call.phase)
                    .apply(Action::Dtmf)
                    .map_err(|_| Failure::InvalidTransition)?;
                if digits.is_empty()
                    || digits.len() > MAX_DTMF_DIGITS
                    || !digits
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b"*#ABCD".contains(&b))
                {
                    return Err(Failure::InvalidInput);
                }
                let target = if active.call.direction == "outbound" {
                    active.winner.as_ref().ok_or(Failure::InvalidTransition)?
                } else {
                    &active.root
                };
                self.ari
                    .command(
                        Method::POST,
                        &["channels", target, "dtmf"],
                        &[("dtmf", &digits)],
                    )
                    .await?;
                Ok(active.call.clone())
            }
            Command::Speak { text } => {
                phone_core::Phase::from(active.call.phase)
                    .apply(Action::Speak)
                    .map_err(|_| Failure::InvalidTransition)?;
                if text.trim().is_empty() || text.chars().count() > MAX_SPEAK_CHARACTERS {
                    return Err(Failure::InvalidInput);
                }
                if active.media.is_none() {
                    self.start_media(Purpose::ReadAloud)
                        .await
                        .map_err(|_| Failure::Unavailable)?;
                }
                let active = self.active.as_ref().ok_or(Failure::NotFound)?;
                active
                    .media
                    .as_ref()
                    .ok_or(Failure::Unavailable)?
                    .commands
                    .try_send(text)
                    .map_err(|_| Failure::Busy)?;
                Ok(active.call.clone())
            }
        }
    }

    async fn end(&mut self, reason: &str) -> Result<Call, Failure> {
        if (self.stop_media().await).is_err() {
            tracing::warn!(operation = "controller", code = "media_stop_failed");
        }
        let active = self.active.as_mut().ok_or(Failure::NotFound)?;
        if active.call.phase != Phase::Ended {
            active.call.phase = phone_core::Phase::from(active.call.phase)
                .apply(Action::End)
                .map_err(|_| Failure::InvalidTransition)?
                .into();
            active.call.ended_at = Some(now());
            active.call.reason = Some(reason.into());
        }
        let call = self.active.as_ref().ok_or(Failure::NotFound)?.call.clone();
        self.cleanup().await?;
        Ok(call)
    }

    async fn cleanup(&mut self) -> Result<(), Failure> {
        let active = self.active.as_ref().ok_or(Failure::NotFound)?;
        let mut failed = false;
        let name = format!("hp-{}", active.call.id);
        if active.call.recording == Recording::Pending {
            failed |= self
                .ari
                .cleanup(Method::POST, &["recordings", "live", &name, "stop"], &[])
                .await
                .is_err();
        }
        for id in self.store.resources().await? {
            failed |= self
                .ari
                .command(Method::DELETE, &["channels", &id], &[])
                .await
                .is_err();
        }
        failed |= self
            .ari
            .command(Method::DELETE, &["bridges", &active.bridge], &[])
            .await
            .is_err();
        if !active.ended_persisted {
            let call = active.call.clone();
            self.publish(&call, "ended").await?;
            self.active
                .as_mut()
                .ok_or(Failure::NotFound)?
                .ended_persisted = true;
        }
        if failed {
            return Err(Failure::Unavailable);
        }
        self.store.clear_resources().await?;
        let active = self.active.as_ref().ok_or(Failure::NotFound)?;
        self.line = self
            .line
            .release(active.owner)
            .map_err(|_| Failure::InvalidTransition)?;
        self.active = None;
        self.store.own_line(None);
        Ok(())
    }

    async fn event(&mut self, event: AriEvent) -> Result<(), Failure> {
        match event {
            AriEvent::StasisStart { channel, args } => {
                let kind = args.first().map(String::as_str).unwrap_or("");
                if kind == "endpoint" {
                    let Some(active) = &self.active else {
                        self.ari
                            .command(Method::DELETE, &["channels", &channel.id], &[])
                            .await?;
                        return Ok(());
                    };
                    if args.get(1) != Some(&active.call.id.to_string())
                        || !active.peers.contains(&channel.id)
                        || !matches!(active.call.phase, Phase::Ringing | Phase::Screening)
                    {
                        self.ari
                            .command(Method::DELETE, &["channels", &channel.id], &[])
                            .await?;
                        return Ok(());
                    }
                    let bridge = active.bridge.clone();
                    let losers: Vec<String> = active
                        .peers
                        .iter()
                        .filter(|id| **id != channel.id)
                        .cloned()
                        .collect();
                    self.stop_media().await?;
                    self.answer_and_record().await?;
                    self.ari
                        .command(
                            Method::POST,
                            &["bridges", &bridge, "addChannel"],
                            &[("channel", &channel.id)],
                        )
                        .await?;
                    if let Some(active) = &mut self.active {
                        active.winner = Some(channel.id);
                    }
                    self.transition(Action::Connect, "connected").await?;
                    for id in losers {
                        if (self
                            .ari
                            .command(Method::DELETE, &["channels", &id], &[])
                            .await)
                            .is_err()
                        {
                            tracing::warn!(
                                operation = "controller",
                                code = "late_channel_cleanup_failed"
                            );
                        }
                    }
                } else if kind == "originator" {
                    if self
                        .active
                        .as_ref()
                        .is_some_and(|active| active.root == channel.id)
                    {
                        self.dial_carrier().await?;
                    }
                } else if kind == "inbound" || kind == "outbound" {
                    let mut channel = channel;
                    if kind == "outbound" {
                        channel.caller.number =
                            args.get(1).cloned().ok_or(Failure::InvalidInput)?;
                    }
                    self.offer(channel, kind.into()).await?;
                }
            }
            AriEvent::StasisEnd { channel } | AriEvent::ChannelDestroyed { channel } => {
                if self.active.as_ref().is_some_and(|active| {
                    active.call.phase != Phase::Ended
                        && (active.root == channel.id
                            || active.winner.as_ref() == Some(&channel.id))
                }) {
                    self.end("remote_hangup").await?;
                }
            }
            AriEvent::ChannelStateChange { .. } | AriEvent::Other => {}
        }
        Ok(())
    }

    async fn tick(&mut self) -> Result<(), Failure> {
        if self.connected && !*self.initialized.borrow() {
            self.reconcile().await?;
            self.ready.send_replace(!self.maintenance);
            self.store.pause_background(self.maintenance);
        }
        let Some(active) = &self.active else {
            return Ok(());
        };
        if active.call.phase == Phase::Ended {
            return self.cleanup().await;
        }
        use phone_core::timers::{Timeout, expired};
        let timeout = expired(
            active.call.phase.into(),
            active.entered.elapsed().as_secs(),
            self.config.timers.screening_seconds,
            self.config.timers.ringing_seconds,
            self.config.timers.voicemail_seconds,
            active.call.direction == "outbound",
        );
        match timeout {
            Timeout::None => Ok(()),
            Timeout::Ring => self.ring(false).await,
            Timeout::Message => self.voicemail().await,
            Timeout::End => {
                let reason = if active.call.phase == Phase::Offered {
                    "admission_failed"
                } else if active.call.direction == "outbound" {
                    "no_answer"
                } else {
                    "voicemail_complete"
                };
                self.end(reason).await.map(|_| ())
            }
        }
    }

    async fn start_media(&mut self, purpose: Purpose) -> Result<(), Failure> {
        let active = self.active.as_ref().ok_or(Failure::NotFound)?;
        let generation = Uuid::new_v4();
        let channel = format!("{}-media-{generation}", active.bridge);
        let start = provider::Start {
            call_id: active.call.id,
            generation,
            channel: channel.clone(),
            bridge: active.bridge.clone(),
            caller: active.call.caller.clone(),
            purpose,
        };
        self.store.track_resource(&channel).await?;
        let handle = provider::start(
            &self.ari,
            &self.config.ai,
            self.key.clone(),
            start,
            self.reports.clone(),
        )
        .await?;
        self.active.as_mut().ok_or(Failure::NotFound)?.media = Some(handle);
        Ok(())
    }

    async fn stop_media(&mut self) -> Result<(), Failure> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        let Some(media) = active.media.take() else {
            return Ok(());
        };
        let unheard = media.unplayed.borrow().clone();
        active.call.transcript.retain(|line| {
            !unheard
                .iter()
                .any(|id| line.item_id == format!("{}:{id}", media.generation))
        });
        media.stop.cancel();
        self.ari
            .command(Method::DELETE, &["channels", &media.channel], &[])
            .await?;
        Ok(())
    }

    async fn report(&mut self, report: Report) -> Result<(), Failure> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        if active.call.id != report.call_id
            || !active
                .media
                .as_ref()
                .is_some_and(|media| media.generation == report.generation)
        {
            return Ok(());
        }
        match report.notice {
            Notice::Transcript {
                speaker,
                item_id,
                text,
            } => {
                let item_id = format!("{}:{item_id}", report.generation);
                if !active
                    .call
                    .transcript
                    .iter()
                    .any(|line| line.item_id == item_id)
                    && active.call.transcript.len() < MAX_TRANSCRIPT_ITEMS
                    && active
                        .call
                        .transcript
                        .iter()
                        .map(|line| line.text.len())
                        .sum::<usize>()
                        .saturating_add(text.len())
                        <= MAX_TRANSCRIPT_BYTES
                {
                    active.call.transcript.push(Transcript {
                        speaker,
                        item_id,
                        text,
                    });
                    let call = active.call.clone();
                    self.publish(&call, "transcript").await?;
                }
            }
            Notice::Truncate { item_id } => {
                let item_id = format!("{}:{item_id}", report.generation);
                active
                    .call
                    .transcript
                    .retain(|line| line.item_id != item_id);
                let call = active.call.clone();
                self.publish(&call, "transcript").await?;
            }
            Notice::Failed => {
                active.call.last_error = Some("ai_connection_failed".into());
                if active.call.phase == Phase::Screening {
                    self.ring(false).await?;
                } else {
                    self.stop_media().await?;
                }
            }
            Notice::Route(classification) if active.call.phase == Phase::Screening => {
                match phone_core::classified_route(classification) {
                    Route::End => {
                        self.end("ai_spam").await?;
                    }
                    Route::Ring => self.ring(false).await?,
                    Route::Voicemail => self.voicemail().await?,
                    Route::Continue => {}
                    Route::Screen => return Err(Failure::InvalidTransition),
                }
            }
            Notice::Route(_) => {}
        }
        Ok(())
    }

    async fn voicemail(&mut self) -> Result<(), Failure> {
        self.stop_media().await?;
        self.answer_and_record().await?;
        self.transition(Action::RecordMessage, "voicemail").await?;
        if let Some(active) = &self.active {
            for id in &active.peers {
                if (self
                    .ari
                    .command(Method::DELETE, &["channels", id], &[])
                    .await)
                    .is_err()
                {
                    tracing::warn!(
                        operation = "controller",
                        code = "media_channel_cleanup_failed"
                    );
                }
            }
            self.ari
                .command(
                    Method::POST,
                    &["channels", &active.root, "play"],
                    &[("media", "sound:homephone/voicemail")],
                )
                .await?;
        }
        Ok(())
    }
}

async fn drive_events(ari: Ari, sender: mpsc::Sender<Request>, stop: CancellationToken) {
    loop {
        let connection = tokio::select! { _ = stop.cancelled() => break, result = ari.connect_events() => result };
        if let Ok(mut socket) = connection {
            ari.reset_events();
            if sender.send(Request::Connection(true)).await.is_err() {
                break;
            }
            loop {
                let frame = tokio::select! { _ = stop.cancelled() => return, frame = socket.next() => frame };
                match frame {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        match serde_json::from_str::<AriEvent>(&text) {
                            Ok(event) => {
                                if ari.observe(&event).is_err() {
                                    break;
                                }
                                if matches!(
                                    event,
                                    AriEvent::Other | AriEvent::ChannelStateChange { .. }
                                ) {
                                    continue;
                                }
                                if sender.send(Request::Event(event)).await.is_err() {
                                    return;
                                }
                            }
                            Err(_) => {
                                tracing::warn!(operation = "gateway_events", code = "invalid_event")
                            }
                        }
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
                    | Some(Err(_))
                    | None => break,
                    Some(Ok(_)) => {}
                }
            }
        }
        ari.reset_events();
        if sender.send(Request::Connection(false)).await.is_err() {
            break;
        }
        tokio::select! { _ = stop.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(2)) => {} }
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
