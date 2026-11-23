use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use phone_server::{
    config::Config, model::Call, notifications::Notifier, secrets::Secrets, store::Store,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

type Requests = Vec<(HeaderMap, Value)>;

#[derive(Default)]
struct Captured {
    ntfy: Requests,
    webhooks: Requests,
    ntfy_statuses: VecDeque<StatusCode>,
    webhook_statuses: VecDeque<StatusCode>,
    ntfy_responses: VecDeque<Value>,
}

struct Fixture {
    origin: String,
    captured: Arc<Mutex<Captured>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    async fn start() -> Self {
        async fn ntfy(
            State(state): State<Arc<Mutex<Captured>>>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> Response {
            let mut state = state.lock().unwrap();
            state.ntfy.push((headers, body.clone()));
            let status = state.ntfy_statuses.pop_front().unwrap_or(StatusCode::OK);
            if status != StatusCode::OK {
                return (status, [("location", "/hook")]).into_response();
            }
            if let Some(response) = state.ntfy_responses.pop_front() {
                return Json(response).into_response();
            }
            Json(json!({"id":"fixture12345", "event":"message", "topic":body["topic"], "sequence_id":body["sequence_id"]})).into_response()
        }
        async fn hook(
            State(state): State<Arc<Mutex<Captured>>>,
            headers: HeaderMap,
            Json(body): Json<Value>,
        ) -> StatusCode {
            let mut state = state.lock().unwrap();
            state.webhooks.push((headers, body));
            state
                .webhook_statuses
                .pop_front()
                .unwrap_or(StatusCode::NO_CONTENT)
        }
        let captured = Arc::new(Mutex::new(Captured::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/", post(ntfy))
            .route("/hook", post(hook))
            .with_state(captured.clone());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            origin,
            captured,
            task,
        }
    }

    fn config(&self, webhook: bool) -> Config {
        let mut config: Config = toml::from_str(&format!(
            "[notifications.ntfy]\nserver = '{}'\ntopic = 'home-status'\n",
            self.origin
        ))
        .unwrap();
        if webhook {
            config.notifications.webhook_url = Some(format!("{}/hook", self.origin));
        }
        config.validate().unwrap();
        config
    }

    fn notifier(&self, store: Store, webhook: bool) -> Notifier {
        let config = self.config(webhook);
        let secrets = Secrets::from_lookup(
            &config,
            |name| match name {
                "HOMEPHONE_NTFY_TOKEN" => Some("synthetic-ntfy-token".into()),
                "HOMEPHONE_WEBHOOK_SECRET" => Some("synthetic-webhook-secret".into()),
                _ => None,
            },
            None,
        )
        .unwrap();
        Notifier::new(config, store, secrets).unwrap()
    }
}

#[tokio::test]
async fn incoming_calls_publish_private_japanese_ntfy_payloads() {
    let fixture = Fixture::start().await;
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut call = Call::offered("+81312345678".into(), "inbound".into(), 10);
    call.summary = Some("private-summary".into());
    call.last_error = Some("private-diagnostic".into());
    store.save(&call, "incoming", 10).await.unwrap();
    fixture
        .notifier(store.clone(), false)
        .pump(10)
        .await
        .unwrap();
    let captured = fixture.captured.lock().unwrap();
    assert_eq!(captured.ntfy.len(), 1);
    let (headers, payload) = &captured.ntfy[0];
    assert_eq!(headers["authorization"], "Bearer synthetic-ntfy-token");
    assert_eq!(
        payload,
        &json!({
            "topic":"home-status",
            "title":"固定電話に着信",
            "message":"発信者番号: +81312345678\n着信時刻: 1970-01-01T09:00:10+09:00",
            "priority":4,
            "tags":["telephone_receiver"],
            "click":"http://localhost:8787/",
            "sequence_id":format!("homephone-{}", call.id),
        })
    );
    assert!(!payload.to_string().contains("private-"));
}

#[tokio::test]
async fn successful_ntfy_calls_stay_suppressed_across_events_and_restarts() {
    let fixture = Fixture::start().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    let call = Call::offered("0312345678".into(), "inbound".into(), 10);
    {
        let store = Store::open(&path).await.unwrap();
        let notifier = fixture.notifier(store.clone(), false);
        for kind in ["incoming", "incoming", "ringing", "incoming"] {
            store.save(&call, kind, 10).await.unwrap();
            notifier.pump(10).await.unwrap();
        }
        assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 1);
    }
    let store = Store::open(&path).await.unwrap();
    store.save(&call, "incoming", 10).await.unwrap();
    fixture.notifier(store, false).pump(10).await.unwrap();
    assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 1);
}

#[tokio::test]
async fn ntfy_and_webhook_failures_retry_without_repeating_successful_channels() {
    for failing_ntfy in [true, false] {
        let fixture = Fixture::start().await;
        {
            let mut captured = fixture.captured.lock().unwrap();
            if failing_ntfy {
                captured
                    .ntfy_statuses
                    .push_back(StatusCode::SERVICE_UNAVAILABLE);
            } else {
                captured
                    .webhook_statuses
                    .push_back(StatusCode::SERVICE_UNAVAILABLE);
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        let call = Call::offered("0312345678".into(), "inbound".into(), 10);
        store.save(&call, "incoming", 10).await.unwrap();
        let notifier = fixture.notifier(store.clone(), true);
        notifier.pump(10).await.unwrap();
        {
            let captured = fixture.captured.lock().unwrap();
            assert_eq!(captured.ntfy.len(), 1);
            assert_eq!(captured.webhooks.len(), 1);
        }
        assert!(store.due(14).await.unwrap().is_empty());
        notifier.pump(15).await.unwrap();
        assert!(store.due(100).await.unwrap().is_empty());
        let captured = fixture.captured.lock().unwrap();
        assert_eq!(captured.ntfy.len(), if failing_ntfy { 2 } else { 1 });
        assert_eq!(captured.webhooks.len(), if failing_ntfy { 1 } else { 2 });
        if failing_ntfy {
            assert_eq!(captured.ntfy[0].1, captured.ntfy[1].1);
        }
    }
}

#[tokio::test]
async fn delayed_ntfy_retries_expire_and_do_not_publish_stale_calls() {
    let fixture = Fixture::start().await;
    fixture
        .captured
        .lock()
        .unwrap()
        .ntfy_statuses
        .push_back(StatusCode::SERVICE_UNAVAILABLE);
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let call = Call::offered("0312345678".into(), "inbound".into(), 10);
    store.save(&call, "incoming", 10).await.unwrap();
    let notifier = fixture.notifier(store.clone(), false);
    notifier.pump(10).await.unwrap();
    notifier.pump(131).await.unwrap();
    assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 1);
    assert!(store.due(1000).await.unwrap().is_empty());
}

#[tokio::test]
async fn unknown_caller_values_never_enter_the_notification_body() {
    for caller in [
        "",
        "anonymous",
        "0312345678\r\nprivate-data",
        "<script>private-data</script>",
        "１２３",
        &"1".repeat(33),
    ] {
        let fixture = Fixture::start().await;
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        let call = Call::offered(caller.into(), "inbound".into(), 10);
        store.save(&call, "incoming", 10).await.unwrap();
        fixture.notifier(store, false).pump(10).await.unwrap();
        let captured = fixture.captured.lock().unwrap();
        assert_eq!(captured.ntfy.len(), 1);
        assert_eq!(
            captured.ntfy[0].1["message"],
            "発信者番号: 非通知・番号取得不可\n着信時刻: 1970-01-01T09:00:10+09:00"
        );
    }
}

#[tokio::test]
async fn rejected_or_unverified_ntfy_responses_remain_pending() {
    for status in [
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::BAD_REQUEST,
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        let fixture = Fixture::start().await;
        fixture
            .captured
            .lock()
            .unwrap()
            .ntfy_statuses
            .push_back(status);
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        let call = Call::offered("0312345678".into(), "inbound".into(), 10);
        store.save(&call, "incoming", 10).await.unwrap();
        let notifier = fixture.notifier(store.clone(), false);
        notifier.pump(10).await.unwrap();
        assert_eq!(store.due(15).await.unwrap().len(), 1);
        assert!(fixture.captured.lock().unwrap().webhooks.is_empty());
        notifier.pump(15).await.unwrap();
        assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 2);
        assert!(store.due(100).await.unwrap().is_empty());
    }
    for response in [json!({}), json!("x".repeat(16_385))] {
        let fixture = Fixture::start().await;
        fixture
            .captured
            .lock()
            .unwrap()
            .ntfy_responses
            .push_back(response);
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        let call = Call::offered("0312345678".into(), "inbound".into(), 10);
        store.save(&call, "incoming", 10).await.unwrap();
        let notifier = fixture.notifier(store.clone(), false);
        notifier.pump(10).await.unwrap();
        assert_eq!(store.due(15).await.unwrap().len(), 1);
        notifier.pump(15).await.unwrap();
        assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 2);
        assert!(store.due(100).await.unwrap().is_empty());
    }
    for (field, value) in [
        ("id", json!("")),
        ("id", json!("x".repeat(65))),
        ("id", json!("bad/id")),
        ("event", json!("poll_request")),
        ("topic", json!("another-topic")),
        ("sequence_id", json!("another-call")),
    ] {
        let fixture = Fixture::start().await;
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        let call = Call::offered("0312345678".into(), "inbound".into(), 10);
        let mut response = json!({"id":"fixture12345", "event":"message", "topic":"home-status", "sequence_id":format!("homephone-{}",call.id)});
        response[field] = value;
        fixture
            .captured
            .lock()
            .unwrap()
            .ntfy_responses
            .push_back(response);
        store.save(&call, "incoming", 10).await.unwrap();
        let notifier = fixture.notifier(store.clone(), false);
        notifier.pump(10).await.unwrap();
        assert_eq!(store.due(15).await.unwrap().len(), 1, "{field}");
        notifier.pump(15).await.unwrap();
        assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 2);
        assert!(store.due(100).await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn failed_ntfy_checkpoint_writes_keep_the_same_pending_notification() {
    let fixture = Fixture::start().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    let call = Call::offered("0312345678".into(), "inbound".into(), 10);
    {
        let store = Store::open(&path).await.unwrap();
        store.with(|connection| {
            connection.execute_batch("CREATE TRIGGER reject_ntfy_checkpoint BEFORE INSERT ON notification_receipts WHEN NEW.channel='ntfy' BEGIN SELECT RAISE(FAIL,'synthetic checkpoint failure'); END;")?;
            Ok(())
        }).await.unwrap();
        store.save(&call, "incoming", 10).await.unwrap();
        fixture
            .notifier(store.clone(), false)
            .pump(10)
            .await
            .unwrap();
        assert_eq!(store.due(15).await.unwrap().len(), 1);
        assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 1);
        store
            .with(|connection| {
                connection.execute_batch("DROP TRIGGER reject_ntfy_checkpoint")?;
                Ok(())
            })
            .await
            .unwrap();
    }
    let store = Store::open(&path).await.unwrap();
    fixture
        .notifier(store.clone(), false)
        .pump(15)
        .await
        .unwrap();
    assert!(store.due(100).await.unwrap().is_empty());
    let captured = fixture.captured.lock().unwrap();
    assert_eq!(captured.ntfy.len(), 2);
    assert_eq!(captured.ntfy[0].1, captured.ntfy[1].1);
}

#[tokio::test]
async fn notification_checkpoints_follow_call_deletion_and_history_retention() {
    for prune in [true, false] {
        let fixture = Fixture::start().await;
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        let mut call = Call::offered("0312345678".into(), "inbound".into(), 10);
        store.save(&call, "incoming", 10).await.unwrap();
        fixture
            .notifier(store.clone(), false)
            .pump(10)
            .await
            .unwrap();
        let before: i64 = store
            .with(|connection| {
                Ok(connection.query_row(
                    "SELECT count(*) FROM notification_receipts",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(before, 2);
        call.phase = phone_server::model::Phase::Ended;
        call.ended_at = Some(20);
        store.save(&call, "ended", 20).await.unwrap();
        if prune {
            store.prune_history(21).await.unwrap();
        } else {
            store.delete_finished(call.id, 21).await.unwrap();
        }
        let after: i64 = store
            .with(|connection| {
                Ok(connection.query_row(
                    "SELECT count(*) FROM notification_receipts",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(after, 0);
        assert_eq!(fixture.captured.lock().unwrap().ntfy.len(), 1);
    }
}

#[tokio::test]
async fn ineligible_events_and_disabled_configuration_never_contact_ntfy() {
    for (kind, direction, created_at, now, phase) in [
        (
            "incoming",
            "outbound",
            10,
            10,
            phone_server::model::Phase::Offered,
        ),
        (
            "incoming",
            "unknown",
            10,
            10,
            phone_server::model::Phase::Offered,
        ),
        (
            "ringing",
            "inbound",
            10,
            10,
            phone_server::model::Phase::Ringing,
        ),
        (
            "complete",
            "inbound",
            10,
            10,
            phone_server::model::Phase::Offered,
        ),
        (
            "incoming",
            "inbound",
            10,
            131,
            phone_server::model::Phase::Offered,
        ),
        (
            "incoming",
            "inbound",
            11,
            10,
            phone_server::model::Phase::Offered,
        ),
        (
            "incoming",
            "inbound",
            10,
            10,
            phone_server::model::Phase::Screening,
        ),
    ] {
        let fixture = Fixture::start().await;
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        let mut call = Call::offered("0312345678".into(), direction.into(), 10);
        call.phase = phase;
        store.save(&call, kind, created_at).await.unwrap();
        fixture.notifier(store, false).pump(now).await.unwrap();
        assert!(fixture.captured.lock().unwrap().ntfy.is_empty());
    }
    let fixture = Fixture::start().await;
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let config = Config::default();
    let secrets = Secrets::from_lookup(&config, |_| None, None).unwrap();
    let call = Call::offered("0312345678".into(), "inbound".into(), 10);
    store.save(&call, "incoming", 10).await.unwrap();
    Notifier::new(config, store, secrets)
        .unwrap()
        .pump(10)
        .await
        .unwrap();
    assert!(fixture.captured.lock().unwrap().ntfy.is_empty());
}
