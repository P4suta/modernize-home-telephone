const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;
const RETRY_BASE_SECONDS: u64 = 5_u64;
const MAX_BACKOFF_EXPONENT: u32 = 6;
const DELIVERY_LEASE_SECONDS: u64 = 60;
const EVENT_QUEUE_CAPACITY: usize = 256;

use crate::model::{Call, Delivery, Event, Phase, PushSubscription, Settings};
use anyhow::{Context, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
pub struct Store {
    pub(crate) events: tokio::sync::broadcast::Sender<Event>,
    connection: Arc<Mutex<Connection>>,
    lease: Arc<std::fs::File>,
    pub recording_gate: Arc<tokio::sync::RwLock<()>>,
    background_paused: Arc<AtomicBool>,
    owner: Arc<Mutex<Option<uuid::Uuid>>>,
}

impl Store {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = path.parent() {
                let mut builder = std::fs::DirBuilder::new(); builder.recursive(true);
                #[cfg(unix)] { use std::os::unix::fs::DirBuilderExt; builder.mode(PRIVATE_DIRECTORY_MODE); }
                builder.create(parent)?;
            }
            let mut options = std::fs::OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(unix)] {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(PRIVATE_FILE_MODE);
            }
            let mut lock_path = path.as_os_str().to_owned(); lock_path.push(".lock");
            let lease = options.open(std::path::PathBuf::from(lock_path))?;
            lease.try_lock().context("another service owns this database")?;
            options.open(&path)?;
            let connection = Connection::open(path)?;
            connection.busy_timeout(Duration::from_secs(2))?;
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "synchronous", "FULL")?;
            let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
            ensure!(version <= 1, "database schema is newer than this application");
            connection.execute_batch("
                CREATE TABLE IF NOT EXISTS calls (
                    id TEXT PRIMARY KEY, phase TEXT NOT NULL, started_at INTEGER NOT NULL,
                    ended_at INTEGER, payload TEXT NOT NULL
                );
                CREATE UNIQUE INDEX IF NOT EXISTS one_active_line ON calls((1)) WHERE ended_at IS NULL;
                CREATE TABLE IF NOT EXISTS events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, created_at INTEGER NOT NULL, payload TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS outbox (
                    id INTEGER PRIMARY KEY, payload TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0,
                    next_at INTEGER NOT NULL, delivered INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS push_subscriptions (endpoint TEXT PRIMARY KEY, payload TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS resources (channel_id TEXT PRIMARY KEY);
                CREATE TABLE IF NOT EXISTS jobs (call_id TEXT PRIMARY KEY, attempts INTEGER NOT NULL DEFAULT 0, next_at INTEGER NOT NULL);
                CREATE TABLE IF NOT EXISTS capture_jobs (call_id TEXT PRIMARY KEY, attempts INTEGER NOT NULL DEFAULT 0, next_at INTEGER NOT NULL);
                INSERT OR IGNORE INTO capture_jobs(call_id,next_at) SELECT id,0 FROM calls WHERE ended_at IS NOT NULL AND json_extract(payload,'$.recording.status')='pending';
                CREATE TABLE IF NOT EXISTS recording_cleanup (name TEXT PRIMARY KEY, next_at INTEGER NOT NULL);
                PRAGMA user_version=1;
            ")?;
            Ok(Self { events: tokio::sync::broadcast::channel(EVENT_QUEUE_CAPACITY).0, connection: Arc::new(Mutex::new(connection)), lease: Arc::new(lease), recording_gate: Arc::new(tokio::sync::RwLock::new(())), background_paused: Arc::new(AtomicBool::new(false)), owner:Arc::new(Mutex::new(None)) })
        }).await?
    }

    pub async fn with<T, F>(&self, operation: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> anyhow::Result<T> + Send + 'static,
    {
        let connection = self.connection.clone();
        let lease = self.lease.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let mut locked = connection
                .lock()
                .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
            operation(&mut locked)
        })
        .await?
    }

    pub async fn save(&self, call: &Call, kind: &str, now: u64) -> anyhow::Result<Event> {
        let call = call.clone();
        let kind = kind.to_owned();
        let events = self.events.clone();
        self.with(move |connection| {
            let transaction = connection.transaction()?;
            let event = save_transaction(&transaction, &call, &kind, now)?;
            transaction.commit()?;
            Ok(Self::committed(&events, event))
        })
        .await
    }

    pub async fn calls(&self) -> anyhow::Result<Vec<Call>> {
        self.with(|connection| {
            read_calls(
                connection,
                "SELECT json_set(payload,'$.transcript',json('[]')) FROM calls ORDER BY started_at DESC, id DESC LIMIT 200",
            )
        })
        .await
    }
    pub async fn has_active(&self) -> anyhow::Result<bool> {
        self.with(|connection| {
            Ok(connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM calls WHERE ended_at IS NULL)",
                [],
                |row| row.get(0),
            )?)
        })
        .await
    }

    pub async fn update_finished<F>(
        &self,
        id: uuid::Uuid,
        kind: &str,
        now: u64,
        edit: F,
    ) -> anyhow::Result<Option<Event>>
    where
        F: FnOnce(&mut Call) + Send + 'static,
    {
        let kind = kind.to_owned();
        let events = self.events.clone();
        self.with(move |connection| {
            let transaction = connection.transaction()?;
            let payload: Option<String> = transaction
                .query_row(
                    "SELECT payload FROM calls WHERE id=?1 AND ended_at IS NOT NULL",
                    [id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(payload) = payload else {
                return Ok(None);
            };
            let mut call: Call = serde_json::from_str(&payload)?;
            let was_pending = call.recording == crate::model::Recording::Pending;
            edit(&mut call);
            if was_pending && matches!(call.recording, crate::model::Recording::Available { .. }) {
                transaction.execute(
                    "UPDATE jobs SET attempts=0,next_at=?2 WHERE call_id=?1",
                    params![id.to_string(), i64::try_from(now)?],
                )?;
            }
            let event = save_transaction(&transaction, &call, &kind, now)?;
            transaction.commit()?;
            Ok(Some(Self::committed(&events, event)))
        })
        .await
    }

    pub async fn delete_finished(&self, id: uuid::Uuid, now: u64) -> anyhow::Result<Option<Event>> {
        ensure!(!self.owns_line(id), "gateway cleanup still owns this call");
        let events = self.events.clone();
        self.with(move |connection| {
            let transaction = connection.transaction()?;
            let payload: Option<String> = transaction
                .query_row(
                    "SELECT payload FROM calls WHERE id=?1",
                    [id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(payload) = payload else {
                return Ok(None);
            };
            let mut call: Call = serde_json::from_str(&payload)?;
            ensure!(call.phase == Phase::Ended, "cannot delete an active call");
            transaction.execute(
                "DELETE FROM outbox WHERE json_extract(payload,'$.call.id')=?1",
                [id.to_string()],
            )?;
            transaction.execute(
                "DELETE FROM events WHERE json_extract(payload,'$.call.id')=?1",
                [id.to_string()],
            )?;
            transaction.execute("DELETE FROM jobs WHERE call_id=?1", [id.to_string()])?;
            transaction.execute(
                "DELETE FROM capture_jobs WHERE call_id=?1",
                [id.to_string()],
            )?;
            call.caller.clear();
            call.transcript.clear();
            call.summary = None;
            call.last_error = None;
            call.recording = crate::model::Recording::Deleted {
                reason: "user_deleted".into(),
            };
            let event = save_transaction(&transaction, &call, "deleted", now)?;
            transaction.execute("DELETE FROM calls WHERE id=?1", [id.to_string()])?;
            transaction.execute("DELETE FROM jobs WHERE call_id=?1", [id.to_string()])?;
            transaction.commit()?;
            Ok(Some(Self::committed(&events, event)))
        })
        .await
    }

    fn committed(events: &tokio::sync::broadcast::Sender<Event>, event: Event) -> Event {
        if events.send(event.clone()).is_err() {
            tracing::debug!(
                operation = "committed_event",
                code = "no_live_subscribers",
                event_id = event.id
            );
        }
        event
    }

    pub async fn jobs(&self, now: u64) -> anyhow::Result<Vec<(uuid::Uuid, u32)>> {
        let now = i64::try_from(now)?;
        self.with(move |connection| {
            let mut statement = connection.prepare("SELECT jobs.call_id,jobs.attempts FROM jobs JOIN calls ON calls.id=jobs.call_id WHERE next_at<=?1 AND jobs.attempts<8 AND json_extract(calls.payload,'$.recording.status')!='pending' AND json_extract(calls.payload,'$.processing') NOT IN ('complete','failed') ORDER BY next_at LIMIT 8")?;
            let rows = statement.query_map([now], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?.collect::<Result<Vec<_>,_>>()?;
            rows.into_iter().map(|(id, attempts)| Ok((id.parse()?, attempts))).collect()
        }).await
    }

    pub async fn finish_job(&self, id: uuid::Uuid) -> anyhow::Result<()> {
        self.with(move |connection| {
            connection.execute("DELETE FROM jobs WHERE call_id=?1", [id.to_string()])?;
            Ok(())
        })
        .await
    }
    pub async fn captures(&self, now: u64) -> anyhow::Result<Vec<(uuid::Uuid, u32)>> {
        let now = i64::try_from(now)?;
        self.with(move |connection| {
            let mut statement=connection.prepare("SELECT capture_jobs.call_id,capture_jobs.attempts FROM capture_jobs JOIN calls ON calls.id=capture_jobs.call_id WHERE next_at<=?1 AND attempts<8 AND ended_at IS NOT NULL AND json_extract(calls.payload,'$.recording.status')='pending' ORDER BY next_at LIMIT 16")?;
            let rows=statement.query_map([now],|row|Ok((row.get::<_,String>(0)?,row.get(1)?)))?.collect::<Result<Vec<_>,_>>()?;
            rows.into_iter().map(|(id,attempts)|Ok((id.parse()?,attempts))).collect()
        }).await
    }
    pub async fn finish_capture(&self, id: uuid::Uuid) -> anyhow::Result<()> {
        self.with(move |connection| {
            connection.execute(
                "DELETE FROM capture_jobs WHERE call_id=?1",
                [id.to_string()],
            )?;
            Ok(())
        })
        .await
    }
    pub async fn retry_capture(
        &self,
        id: uuid::Uuid,
        attempts: u32,
        now: u64,
    ) -> anyhow::Result<()> {
        let deadline = i64::try_from(
            now.checked_add(RETRY_BASE_SECONDS * (1_u64 << attempts.min(MAX_BACKOFF_EXPONENT)))
                .context("capture deadline overflow")?,
        )?;
        self.with(move |connection| {
            connection.execute(
                "UPDATE capture_jobs SET attempts=attempts+1,next_at=?2 WHERE call_id=?1",
                params![id.to_string(), deadline],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn retry_job(&self, id: uuid::Uuid, attempts: u32, now: u64) -> anyhow::Result<()> {
        let deadline = i64::try_from(
            now.checked_add(RETRY_BASE_SECONDS * (1_u64 << attempts.min(MAX_BACKOFF_EXPONENT)))
                .context("job deadline overflow")?,
        )?;
        self.with(move |connection| {
            connection.execute(
                "UPDATE jobs SET attempts=attempts+1,next_at=?2 WHERE call_id=?1",
                params![id.to_string(), deadline],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn backup(&self, destination: std::path::PathBuf) -> anyhow::Result<()> {
        self.with(move |connection| {
            connection.backup("main", destination, None::<fn(rusqlite::backup::Progress)>)?;
            Ok(())
        })
        .await
    }

    pub async fn snapshot(&self) -> anyhow::Result<(Vec<Call>, i64)> {
        self.with(|connection| {
            let transaction = connection.transaction()?;
            let calls = read_calls(
                &transaction,
                "SELECT json_set(payload,'$.transcript',json('[]')) FROM calls ORDER BY started_at DESC,id DESC LIMIT 200",
            )?;
            let cursor =
                transaction.query_row("SELECT COALESCE(MAX(id),0) FROM events", [], |row| {
                    row.get(0)
                })?;
            transaction.commit()?;
            Ok((calls, cursor))
        })
        .await
    }

    pub async fn events_after(&self, cursor: i64) -> anyhow::Result<Vec<Event>> {
        self.with(move |connection| {
            let mut statement = connection
                .prepare("SELECT payload FROM events WHERE id>?1 ORDER BY id LIMIT 100")?;
            let rows = statement
                .query_map([cursor], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|body| serde_json::from_str(&body).map_err(Into::into))
                .collect()
        })
        .await
    }

    pub async fn recordings(&self) -> anyhow::Result<Vec<Call>> {
        self.with(|connection| {
            read_calls(connection, "SELECT payload FROM calls WHERE ended_at IS NOT NULL ORDER BY ended_at, id LIMIT 10000")
        }).await
    }
    pub fn pause_background(&self, paused: bool) {
        self.background_paused.store(paused, Ordering::Release);
    }
    pub fn own_line(&self, owner: Option<uuid::Uuid>) {
        if let Ok(mut current) = self.owner.lock() {
            *current = owner;
        }
    }
    pub fn owns_line(&self, id: uuid::Uuid) -> bool {
        self.owner
            .lock()
            .map(|owner| *owner == Some(id))
            .unwrap_or(true)
    }
    pub fn background_paused(&self) -> bool {
        self.background_paused.load(Ordering::Acquire)
    }

    pub async fn recording_usage(&self) -> anyhow::Result<u64> {
        self.with(|connection| {
            let used:i64=connection.query_row("SELECT COALESCE(SUM(json_extract(payload,'$.recording.bytes')),0) FROM calls WHERE ended_at IS NOT NULL AND json_extract(payload,'$.recording.status')='available'", [], |row| row.get(0))?;
            Ok(u64::try_from(used)?)
        }).await
    }
    pub async fn purge_candidates(&self, cutoff: u64) -> anyhow::Result<Vec<Call>> {
        let cutoff = i64::try_from(cutoff)?;
        self.with(move |connection| {
            let mut statement=connection.prepare("SELECT json_set(payload,'$.transcript',json('[]')) FROM calls WHERE ended_at IS NOT NULL AND (ended_at<=?1 OR json_extract(payload,'$.recording.status')='available') ORDER BY ended_at,id LIMIT 200")?;
            let rows=statement.query_map([cutoff], |row| row.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
            rows.into_iter().map(|body|serde_json::from_str(&body).map_err(Into::into)).collect()
        }).await
    }
    pub async fn recording_cleanup(&self, now: u64) -> anyhow::Result<Vec<String>> {
        let now = i64::try_from(now)?;
        self.with(move |connection| {
            let mut statement=connection.prepare("SELECT name FROM recording_cleanup WHERE next_at<=?1 ORDER BY next_at,name LIMIT 16")?;
            statement.query_map([now],|row|row.get(0))?.collect::<Result<Vec<_>,_>>().map_err(Into::into)
        }).await
    }
    pub async fn recording_cleaned(&self, name: String) -> anyhow::Result<()> {
        self.with(move |connection| {
            connection.execute("DELETE FROM recording_cleanup WHERE name=?1", [name])?;
            Ok(())
        })
        .await
    }
    pub async fn retry_recording_cleanup(&self, name: String, now: u64) -> anyhow::Result<()> {
        let deadline = i64::try_from(
            now.checked_add(DELIVERY_LEASE_SECONDS)
                .context("recording cleanup deadline overflow")?,
        )?;
        self.with(move |connection| {
            connection.execute(
                "UPDATE recording_cleanup SET next_at=?2 WHERE name=?1",
                params![name, deadline],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get(&self, id: uuid::Uuid) -> anyhow::Result<Option<Call>> {
        self.with(move |connection| {
            let payload: Option<String> = connection
                .query_row(
                    "SELECT payload FROM calls WHERE id=?1",
                    [id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            payload
                .map(|body| serde_json::from_str(&body).map_err(Into::into))
                .transpose()
        })
        .await
    }

    pub async fn recover(&self, now: u64) -> anyhow::Result<usize> {
        self.with(move |connection| {
            let transaction = connection.transaction()?;
            let active = read_calls(
                &transaction,
                "SELECT payload FROM calls WHERE ended_at IS NULL",
            )?;
            for mut call in active.iter().cloned() {
                call.phase = Phase::Ended;
                call.ended_at = Some(now);
                call.reason = Some("service_restart".into());
                save_transaction(&transaction, &call, "interrupted", now)?;
            }
            transaction.commit()?;
            Ok(active.len())
        })
        .await
    }

    pub async fn settings(&self) -> anyhow::Result<Settings> {
        self.with(|connection| {
            let payload: Option<String> = connection
                .query_row("SELECT payload FROM settings WHERE id=1", [], |row| {
                    row.get(0)
                })
                .optional()?;
            payload
                .map(|body| serde_json::from_str(&body).map_err(Into::into))
                .unwrap_or_else(|| Ok(Settings::default()))
        })
        .await
    }

    pub async fn set_settings(&self, settings: &Settings) -> anyhow::Result<()> {
        let payload = serde_json::to_string(settings)?;
        self.with(move |connection| {
            connection.execute("INSERT INTO settings(id,payload) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [payload])?;
            Ok(())
        }).await
    }

    pub async fn due(&self, now: u64) -> anyhow::Result<Vec<Delivery>> {
        let now = i64::try_from(now)?;
        self.with(move |connection| {
            let mut statement = connection.prepare("SELECT id,payload,attempts FROM outbox WHERE delivered=0 AND attempts<8 AND next_at<=?1 ORDER BY id LIMIT 16")?;
            let rows = statement.query_map([now], |row| Ok(Delivery { id: row.get(0)?, payload: row.get(1)?, attempts: row.get(2)? }))?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        }).await
    }

    pub async fn retry(&self, id: i64, now: u64) -> anyhow::Result<()> {
        self.with(move |connection| {
            let attempts: u32 =
                connection.query_row("SELECT attempts FROM outbox WHERE id=?1", [id], |row| {
                    row.get(0)
                })?;
            let delay =
                RETRY_BASE_SECONDS.saturating_mul(1_u64 << attempts.min(MAX_BACKOFF_EXPONENT));
            connection.execute(
                "UPDATE outbox SET attempts=attempts+1,next_at=?2 WHERE id=?1 AND delivered=0",
                params![
                    id,
                    i64::try_from(now.checked_add(delay).context("retry timestamp overflow")?)?
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn delivered(&self, id: i64) -> anyhow::Result<()> {
        self.with(move |connection| {
            connection.execute("UPDATE outbox SET delivered=1 WHERE id=?1", [id])?;
            Ok(())
        })
        .await
    }

    pub async fn subscribe(&self, subscription: &PushSubscription) -> anyhow::Result<()> {
        let subscription = subscription.clone();
        self.with(move |connection| {
            connection.execute("INSERT INTO push_subscriptions(endpoint,payload) VALUES(?1,?2) ON CONFLICT(endpoint) DO UPDATE SET payload=excluded.payload", params![subscription.endpoint, serde_json::to_string(&subscription)?])?;
            Ok(())
        }).await
    }

    pub async fn subscriptions(&self) -> anyhow::Result<Vec<PushSubscription>> {
        self.with(|connection| {
            let mut statement = connection.prepare("SELECT payload FROM push_subscriptions")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|body| serde_json::from_str(&body).map_err(Into::into))
                .collect()
        })
        .await
    }

    pub async fn unsubscribe(&self, endpoint: String) -> anyhow::Result<()> {
        self.with(move |connection| {
            connection.execute(
                "DELETE FROM push_subscriptions WHERE endpoint=?1",
                [endpoint],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn prune_history(&self, cutoff: u64) -> anyhow::Result<()> {
        let cutoff = i64::try_from(cutoff)?;
        self.with(move |connection| {
            let transaction = connection.transaction()?;
            transaction.execute(
                "DELETE FROM jobs WHERE call_id NOT IN (SELECT id FROM calls)",
                [],
            )?;
            transaction.execute(
                "DELETE FROM capture_jobs WHERE call_id NOT IN (SELECT id FROM calls)",
                [],
            )?;
            transaction.execute(
                "DELETE FROM outbox WHERE id IN (SELECT id FROM events WHERE created_at<?1)",
                [cutoff],
            )?;
            transaction.execute("DELETE FROM events WHERE created_at<?1", [cutoff])?;
            transaction.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn track_resource(&self, id: &str) -> anyhow::Result<()> {
        let id = id.to_owned();
        self.with(move |connection| {
            connection.execute(
                "INSERT OR IGNORE INTO resources(channel_id) VALUES(?1)",
                [id],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn resources(&self) -> anyhow::Result<Vec<String>> {
        self.with(|connection| {
            let mut statement = connection.prepare("SELECT channel_id FROM resources")?;
            statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .await
    }

    pub async fn clear_resources(&self) -> anyhow::Result<()> {
        self.with(|connection| {
            connection.execute("DELETE FROM resources", [])?;
            Ok(())
        })
        .await
    }
}

fn read_calls(connection: &Connection, sql: &str) -> anyhow::Result<Vec<Call>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|body| serde_json::from_str(&body).map_err(Into::into))
        .collect()
}

fn save_transaction(
    transaction: &Transaction<'_>,
    call: &Call,
    kind: &str,
    now: u64,
) -> anyhow::Result<Event> {
    ensure!(
        (call.phase == Phase::Ended) == call.ended_at.is_some(),
        "terminal state and timestamp disagree"
    );
    let phase = serde_json::to_string(&call.phase)?;
    transaction.execute(
        "INSERT INTO calls(id,phase,started_at,ended_at,payload) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET phase=excluded.phase,ended_at=excluded.ended_at,payload=excluded.payload",
        params![call.id.to_string(), phase, i64::try_from(call.started_at)?, call.ended_at.map(i64::try_from).transpose()?, serde_json::to_string(call)?],
    )?;
    if call.ended_at.is_some() && call.processing != "complete" && call.processing != "failed" {
        transaction.execute(
            "INSERT OR IGNORE INTO jobs(call_id,next_at) VALUES(?1,?2)",
            params![call.id.to_string(), i64::try_from(now)?],
        )?;
    }
    if call.ended_at.is_some() && call.recording == crate::model::Recording::Pending {
        transaction.execute(
            "INSERT OR IGNORE INTO capture_jobs(call_id,next_at) VALUES(?1,?2)",
            params![call.id.to_string(), i64::try_from(now)?],
        )?;
    }
    if call.ended_at.is_some() && call.recording != crate::model::Recording::Pending {
        transaction.execute(
            "DELETE FROM capture_jobs WHERE call_id=?1",
            [call.id.to_string()],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO recording_cleanup(name,next_at) VALUES(?1,?2)",
            params![format!("hp-{}", call.id), i64::try_from(now)?],
        )?;
    }
    transaction.execute(
        "INSERT INTO events(created_at,payload) VALUES(?1,'')",
        [i64::try_from(now)?],
    )?;
    let mut preview = call.clone();
    preview.transcript.clear();
    let event = Event {
        id: transaction.last_insert_rowid(),
        kind: kind.into(),
        call: Some(preview),
        created_at: now,
    };
    let payload = serde_json::to_string(&event).context("serialize call event")?;
    transaction.execute(
        "UPDATE events SET payload=?2 WHERE id=?1",
        params![event.id, payload],
    )?;
    transaction.execute(
        "INSERT INTO outbox(id,payload,next_at) VALUES(?1,?2,?3)",
        params![event.id, payload, i64::try_from(now)?],
    )?;
    Ok(event)
}
