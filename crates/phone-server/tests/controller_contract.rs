use axum::{Router, http::StatusCode, routing::any};
use phone_server::{
    config::Config,
    controller::Control,
    gateway::{Ari, Caller, Channel},
    model::{Command, Phase, Rule, Settings},
    store::Store,
};

async fn setup() -> (Control, tokio::task::JoinHandle<()>, tempfile::TempDir) {
    setup_with_faults(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        false,
    )))
    .await
}
async fn setup_with_faults(
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> (Control, tokio::task::JoinHandle<()>, tempfile::TempDir) {
    let (control, task, directory, _) = setup_with_provider(failed, "mock", 30).await;
    (control, task, directory)
}

type Requests = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

#[cfg_attr(not(feature = "plugin-ai"), allow(dead_code))]
#[derive(Clone, Copy)]
enum CleanupFault {
    Recording,
    Channel,
    Bridge,
}

#[derive(Default)]
struct GatewayState {
    cleanup_fault: Option<CleanupFault>,
    channels: Vec<Channel>,
    recording: bool,
    recording_queries: Vec<String>,
    recording_finished: bool,
}

type GatewayStateHandle = std::sync::Arc<std::sync::Mutex<GatewayState>>;

async fn setup_with_provider(
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    provider: &str,
    ringing_seconds: u32,
) -> (
    Control,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
    Requests,
) {
    setup_with_event_source(failed, provider, ringing_seconds, None).await
}

async fn setup_with_event_source(
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    provider: &str,
    ringing_seconds: u32,
    frames: Option<tokio::sync::broadcast::Sender<String>>,
) -> (
    Control,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
    Requests,
) {
    setup_with_gateway_state(
        failed,
        provider,
        ringing_seconds,
        frames,
        Default::default(),
    )
    .await
}

async fn setup_with_gateway_state(
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    provider: &str,
    ringing_seconds: u32,
    frames: Option<tokio::sync::broadcast::Sender<String>>,
    state: GatewayStateHandle,
) -> (
    Control,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
    Requests,
) {
    let requests: Requests = Default::default();
    let recording = state.lock().unwrap().recording;
    let observed = requests.clone();
    let application_state = state.clone();
    let channel_state = state.clone();
    let router = Router::new()
        .route(
            "/ari/applications/homephone",
            axum::routing::get(move || {
                let ids: Vec<_> = application_state.lock().unwrap().channels.iter()
                    .map(|channel| channel.id.clone()).collect();
                async move { axum::Json(serde_json::json!({"channel_ids":ids})) }
            }),
        )
        .route(
            "/ari/channels",
            axum::routing::get(move || {
                let channels: Vec<_> = channel_state.lock().unwrap().channels.iter()
                    .map(|channel| serde_json::json!({
                        "id":channel.id,"state":channel.state,
                        "creationtime":channel.creationtime,"caller":{"number":channel.caller.number}
                    })).collect();
                async move { axum::Json(channels) }
            })
                .post(|| async { StatusCode::NO_CONTENT }),
        )
        .route(
            "/ari/bridges",
            axum::routing::get(|| async { axum::Json(Vec::<String>::new()) }),
        )
        .route(
            "/ari/events",
            axum::routing::get(move |socket: axum::extract::WebSocketUpgrade| {
                let frames = frames.clone();
                async move {
                    socket.on_upgrade(move |mut socket| async move {
                        if let Some(frames) = frames {
                            let mut frames = frames.subscribe();
                            loop {
                                tokio::select! {
                                    message = frames.recv() => {
                                        let Ok(message) = message else { break; };
                                        if socket.send(axum::extract::ws::Message::Text(message.into())).await.is_err() { break; }
                                    }
                                    message = socket.recv() => {
                                        if message.is_none() { break; }
                                    }
                                }
                            }
                        } else {
                            while socket.recv().await.is_some() {}
                        }
                    })
                }
            }),
        )
        .fallback(any(move |method: axum::http::Method, uri: axum::http::Uri| {
            let failed = failed.clone();
            let (fault,recording_finished)={
                let mut state=state.lock().unwrap();
                if uri.path().ends_with("/record"){state.recording_queries.push(uri.query().unwrap_or_default().to_owned());}
                (state.cleanup_fault,state.recording_finished)
            };
            async move {
                if recording_finished && method==axum::http::Method::POST && uri.path().starts_with("/ari/recordings/live/") && uri.path().ends_with("/stop") {return StatusCode::NOT_FOUND;}
                let injected = fault.is_some_and(|fault| match fault {
                    CleanupFault::Recording => method == axum::http::Method::POST
                        && uri.path().starts_with("/ari/recordings/live/")
                        && uri.path().ends_with("/stop"),
                    CleanupFault::Channel => method == axum::http::Method::DELETE
                        && uri.path().starts_with("/ari/channels/"),
                    CleanupFault::Bridge => method == axum::http::Method::DELETE
                        && uri.path().starts_with("/ari/bridges/"),
                });
                if injected || (method == axum::http::Method::DELETE
                    && failed.load(std::sync::atomic::Ordering::SeqCst))
                {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::NO_CONTENT
                }
            }
        }))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let observed = observed.clone();
                async move {
                    observed
                        .lock()
                        .unwrap()
                        .push(request.uri().path().to_owned());
                    next.run(request).await
                }
            },
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config {
        data_dir: dir.path().into(),
        ..Config::default()
    };
    config.ari.url = format!("http://{address}/ari/");
    config.ai.provider = provider.into();
    config.timers.ringing_seconds = ringing_seconds;
    if recording {
        let spool = dir.path().join("spool");
        tokio::fs::create_dir(&spool).await.unwrap();
        config.recording = Some(phone_server::config::CallRecording {
            enabled: true,
            max_seconds: 2,
            gateway_spool_dir: spool,
            nas: None,
        });
    }
    let store = Store::open(&dir.path().join("phone.sqlite")).await.unwrap();
    store
        .set_settings(&Settings {
            rules: [("1".into(), Rule::Allow)].into(),
            ..Settings::default()
        })
        .await
        .unwrap();
    let ari = Ari::new(config.ari.clone(), "secret".into()).unwrap();
    let control = Control::start(config, ari, store);
    control.wait_ready().await.unwrap();
    (control, task, dir, requests)
}

#[tokio::test]
async fn a_full_recording_spool_preserves_calls_and_recovers_after_space_is_freed() {
    let (frames, _) = tokio::sync::broadcast::channel::<String>(16);
    let state = std::sync::Arc::new(std::sync::Mutex::new(GatewayState {
        recording: true,
        ..GatewayState::default()
    }));
    let (control, task, directory, requests) = setup_with_gateway_state(
        Default::default(),
        "disabled",
        30,
        Some(frames.clone()),
        state.clone(),
    )
    .await;
    let mut settings = control.store.settings().await.unwrap();
    settings.recording_limit_bytes = Some(1048576);
    control.store.set_settings(&settings).await.unwrap();
    let filled = directory.path().join("spool/filled.wav");
    std::fs::File::create(&filled)
        .unwrap()
        .set_len(1048576)
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while frames.receiver_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    for blocked in [true, false] {
        let root = channel(if blocked {
            "full-spool"
        } else {
            "recovered-spool"
        });
        let call = control.offer(root.clone(), "inbound".into()).await.unwrap();
        let peer = control
            .store
            .resources()
            .await
            .unwrap()
            .into_iter()
            .find(|id| id.ends_with("-peer-0"))
            .unwrap();
        frames.send(endpoint_event(&peer, call.id)).unwrap();
        let connected = gateway_event_barrier(
            &control,
            &frames,
            &requests,
            &root,
            if blocked {
                "full-spool-connected"
            } else {
                "recovered-spool-connected"
            },
        )
        .await;
        assert_eq!(connected.phase, Phase::Connected, "blocked={blocked}");
        assert_eq!(
            connected.recording,
            if blocked {
                phone_server::model::Recording::Failed {
                    reason: "local_capacity".into(),
                }
            } else {
                phone_server::model::Recording::Pending
            }
        );
        control.command(call.id, Command::Hangup).await.unwrap();
        if blocked {
            std::fs::remove_file(&filled).unwrap();
        }
    }
    control.shutdown().await;
    task.abort();
    assert_eq!(state.lock().unwrap().recording_queries.len(), 1);
    assert!(
        !requests
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.ends_with("/externalMedia") || path.ends_with("/play"))
    );
}

#[tokio::test]
async fn explicit_recording_captures_ordinary_calls_without_ai_or_provider_media() {
    let (frames, _) = tokio::sync::broadcast::channel::<String>(16);
    let state = std::sync::Arc::new(std::sync::Mutex::new(GatewayState {
        recording: true,
        ..GatewayState::default()
    }));
    let (control, task, _directory, requests) = setup_with_gateway_state(
        Default::default(),
        "disabled",
        30,
        Some(frames.clone()),
        state.clone(),
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while frames.receiver_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let root = channel("recorded-handset");
    let call = control.offer(root.clone(), "inbound".into()).await.unwrap();
    assert_eq!(call.recording, phone_server::model::Recording::None);
    let peer = control
        .store
        .resources()
        .await
        .unwrap()
        .into_iter()
        .find(|id| id.ends_with("-peer-0"))
        .unwrap();
    frames.send(endpoint_event(&peer, call.id)).unwrap();
    let connected =
        gateway_event_barrier(&control, &frames, &requests, &root, "recording-connected").await;
    assert_eq!(connected.phase, Phase::Connected);
    assert_eq!(connected.recording, phone_server::model::Recording::Pending);
    state.lock().unwrap().recording_finished = true;
    frames.send(serde_json::json!({"type":"RecordingFinished","recording":{"name":format!("hp-{}",call.id),"format":"wav","state":"done","duration":2}}).to_string()).unwrap();
    let after_limit = gateway_event_barrier(
        &control,
        &frames,
        &requests,
        &root,
        "recording-limit-reached",
    )
    .await;
    assert_eq!(after_limit.phase, Phase::Connected);
    control.command(call.id, Command::Hangup).await.unwrap();
    control.shutdown().await;
    task.abort();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|path| path.ends_with("/record"))
            .count(),
        1
    );
    assert!(
        !requests
            .iter()
            .any(|path| path.ends_with("/externalMedia") || path.ends_with("/play"))
    );
    let state = state.lock().unwrap();
    assert_eq!(state.recording_queries.len(), 1);
    let query = url::form_urlencoded::parse(state.recording_queries[0].as_bytes())
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        query.get("maxDurationSeconds").map(|value| value.as_ref()),
        Some("2")
    );
    assert_eq!(
        query.get("terminateOn").map(|value| value.as_ref()),
        Some("none")
    );
}

#[tokio::test]
async fn ai_off_rings_unknown_callers_without_answering_recording_or_voicemail() {
    let (control, task, _root, requests) =
        setup_with_provider(Default::default(), "disabled", 1).await;
    let mut offered = channel("ordinary-inbound");
    offered.caller.number = "2".into();
    let call = control.offer(offered, "inbound".into()).await.unwrap();
    assert_eq!(call.phase, Phase::Ringing);
    assert_eq!(call.recording, phone_server::model::Recording::None);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let saved = control.store.get(call.id).await.unwrap().unwrap();
            if saved.phase == Phase::Ended {
                assert_eq!(saved.reason.as_deref(), Some("no_answer"));
                assert_eq!(saved.recording, phone_server::model::Recording::None);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    control.shutdown().await;
    task.abort();
    let requests = requests.lock().unwrap();
    assert!(
        !requests.iter().any(|path| path.ends_with("/answer")
            || path.ends_with("/record")
            || path.ends_with("/play")
            || path.ends_with("/externalMedia")),
        "{requests:?}"
    );
}

#[tokio::test]
async fn an_admission_error_cannot_leave_the_external_line_owned_forever() {
    let (control, task, _root) = setup().await;
    control
        .store
        .with(|database| {
            database.execute("UPDATE settings SET payload='malformed-json'", [])?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(
        control
            .offer(channel("bad-settings"), "inbound".into())
            .await
            .is_err()
    );
    let id = control.store.calls().await.unwrap()[0].id;
    let released = tokio::time::timeout(std::time::Duration::from_secs(8), async {
        while control.store.owns_line(id) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;
    control.shutdown().await;
    task.abort();
    assert!(
        released.is_ok(),
        "an admission failure leaked line ownership"
    );
}

fn channel(id: &str) -> Channel {
    Channel {
        creationtime: Some("2026-11-15T05:32:13.000+0900".into()),
        id: id.into(),
        state: "Ring".into(),
        caller: Caller { number: "1".into() },
    }
}

#[tokio::test]
async fn admission_and_hangup_are_serialized_and_persisted() {
    let (control, task, _dir) = setup().await;
    let call = control
        .offer(channel("carrier-1"), "inbound".into())
        .await
        .unwrap();
    assert_eq!(call.phase, Phase::Ringing);
    assert!(
        control
            .offer(channel("carrier-2"), "inbound".into())
            .await
            .is_err()
    );
    let ended = control.command(call.id, Command::Hangup).await.unwrap();
    assert_eq!(ended.phase, Phase::Ended);
    assert!(ended.ended_at.is_some());
    assert!(control.command(call.id, Command::Hold).await.is_err());
    assert_eq!(control.store.get(call.id).await.unwrap(), Some(ended));
    assert!(
        control
            .offer(channel("carrier-3"), "inbound".into())
            .await
            .is_ok()
    );
    task.abort();
}

#[tokio::test]
async fn competing_offers_have_only_one_successful_owner() {
    let (control, task, _dir) = setup().await;
    let (first, second) = tokio::join!(
        control.offer(channel("carrier-a"), "inbound".into()),
        control.offer(channel("carrier-b"), "inbound".into())
    );
    assert_eq!(u8::from(first.is_ok()) + u8::from(second.is_ok()), 1);
    assert_eq!(control.store.calls().await.unwrap().len(), 1);
    task.abort();
}

#[tokio::test]
async fn maintenance_refuses_active_calls_and_blocks_new_admission_until_resumed() {
    let (control, task, _dir) = setup().await;
    control.maintenance(true).await.unwrap();
    assert!(!control.ready());
    assert!(
        control
            .offer(channel("paused"), "inbound".into())
            .await
            .is_err()
    );
    control.maintenance(false).await.unwrap();
    let call = control
        .offer(channel("live"), "inbound".into())
        .await
        .unwrap();
    assert!(control.maintenance(true).await.is_err());
    control.command(call.id, Command::Hangup).await.unwrap();
    control.maintenance(true).await.unwrap();
    assert!(!control.ready());
    task.abort();
}

#[tokio::test]
async fn failed_hangup_quarantines_resources_and_retries_without_deleting_or_reassigning_the_call()
{
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let failed = Arc::new(AtomicBool::new(false));
    let (control, task, _root) = setup_with_faults(failed.clone()).await;
    let call = control
        .offer(channel("fault-carrier"), "inbound".into())
        .await
        .unwrap();
    failed.store(true, Ordering::SeqCst);
    assert!(control.command(call.id, Command::Hangup).await.is_err());
    assert_eq!(
        control.store.get(call.id).await.unwrap().unwrap().phase,
        Phase::Ended
    );
    assert!(control.store.owns_line(call.id));
    assert!(control.deletable(call.id).await.is_err());
    assert!(control.store.delete_finished(call.id, 3).await.is_err());
    assert!(
        control
            .offer(channel("competing"), "inbound".into())
            .await
            .is_err()
    );
    assert!(control.maintenance(true).await.is_err());
    failed.store(false, Ordering::SeqCst);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while control.store.owns_line(call.id) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    control.deletable(call.id).await.unwrap();
    assert!(
        control
            .offer(channel("recovered"), "inbound".into())
            .await
            .is_ok()
    );
    control.shutdown().await;
    task.abort();
}

#[tokio::test]
async fn a_terminal_ingress_cannot_be_admitted_again_or_block_the_next_call() {
    let (control, task, _directory, requests) =
        setup_with_provider(Default::default(), "disabled", 30).await;
    let first = control
        .offer(channel("retired-carrier"), "inbound".into())
        .await
        .unwrap();
    control.command(first.id, Command::Hangup).await.unwrap();
    let rings_before = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.ends_with("/ring"))
        .count();
    let replay = control
        .offer(channel("retired-carrier"), "inbound".into())
        .await;
    let records_after_replay = control.store.calls().await.unwrap().len();
    let rings_after = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.ends_with("/ring"))
        .count();
    let next = control
        .offer(channel("next-carrier"), "inbound".into())
        .await;
    control.shutdown().await;
    task.abort();
    assert!(
        replay.is_err(),
        "a terminal ingress was admitted a second time"
    );
    assert_eq!(records_after_replay, 1);
    assert_eq!(rings_after, rings_before);
    assert!(
        next.is_ok(),
        "a distinct ingress with the same caller must remain valid"
    );
}

#[tokio::test]
async fn a_replayed_ingress_leaves_a_new_channel_generation_with_the_same_id_alone() {
    let state: GatewayStateHandle = Default::default();
    let (control, task, _directory, requests) =
        setup_with_gateway_state(Default::default(), "disabled", 30, None, state.clone()).await;
    let first = control
        .offer(channel("reused-carrier"), "inbound".into())
        .await
        .unwrap();
    control.command(first.id, Command::Hangup).await.unwrap();
    let successor = Channel {
        creationtime: Some("2026-11-15T05:40:00.000+0900".into()),
        ..channel("reused-carrier")
    };
    state.lock().unwrap().channels.push(successor.clone());
    let deletions = |requests: &Requests| {
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|path| path.as_str() == "/ari/channels/reused-carrier")
            .count()
    };
    let before = deletions(&requests);
    let replay = control
        .offer(channel("reused-carrier"), "inbound".into())
        .await;
    let after = deletions(&requests);
    let next = control.offer(successor, "inbound".into()).await;
    control.shutdown().await;
    task.abort();
    assert!(replay.is_err());
    assert_eq!(after, before, "the replay deleted the live successor");
    assert!(next.is_ok());
}

#[tokio::test]
async fn deleting_history_does_not_allow_a_retired_ingress_to_ring_again() {
    let (control, task, _directory, _requests) =
        setup_with_provider(Default::default(), "disabled", 30).await;
    let call = control
        .offer(channel("deleted-ingress"), "inbound".into())
        .await
        .unwrap();
    let ended = control.command(call.id, Command::Hangup).await.unwrap();
    control
        .store
        .delete_finished(call.id, ended.ended_at.unwrap())
        .await
        .unwrap();
    assert!(control.store.calls().await.unwrap().is_empty());
    let replay = control
        .offer(channel("deleted-ingress"), "inbound".into())
        .await;
    let after_replay = control.store.calls().await.unwrap().len();
    let next = control
        .offer(channel("valid-after-deletion"), "inbound".into())
        .await;
    control.shutdown().await;
    task.abort();
    assert!(
        replay.is_err(),
        "history deletion reopened a consumed ingress"
    );
    assert_eq!(after_replay, 0);
    assert!(next.is_ok());
}

#[tokio::test]
async fn stale_gateway_events_cannot_reuse_or_terminate_another_channel_generation() {
    let (frames, _) = tokio::sync::broadcast::channel::<String>(16);
    let (control, task, _directory, requests) =
        setup_with_event_source(Default::default(), "disabled", 30, Some(frames.clone())).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while frames.receiver_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let first = control
        .offer(channel("generation-reuse"), "inbound".into())
        .await
        .unwrap();
    let mut next_channel = channel("generation-reuse");
    next_channel.creationtime = Some("2026-11-14T20:32:13.001Z".into());
    let while_active = control.offer(next_channel.clone(), "inbound".into()).await;
    control.command(first.id, Command::Hangup).await.unwrap();
    let next = control.offer(next_channel, "inbound".into()).await.unwrap();
    frames.send(serde_json::json!({
        "type":"StasisEnd",
        "channel":{"id":"generation-reuse","state":"Down","creationtime":"2026-11-15T05:32:13.000+0900"}
    }).to_string()).unwrap();
    frames.send(serde_json::json!({
        "type":"StasisStart",
        "channel":{"id":"event-barrier","state":"Up","creationtime":"2026-11-15T05:32:13.000+0900"},
        "args":["endpoint","unrelated-call"]
    }).to_string()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if requests
                .lock()
                .unwrap()
                .iter()
                .any(|path| path == "/ari/channels/event-barrier")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let saved = control.store.get(next.id).await.unwrap().unwrap();
    control.shutdown().await;
    task.abort();
    assert_eq!(
        saved.phase,
        Phase::Ringing,
        "a stale terminal event ended a different channel generation"
    );
    assert!(while_active.is_err());
}

#[tokio::test]
async fn rejected_ingress_cleanup_failure_blocks_admission_until_reconciled() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let failed = Arc::new(AtomicBool::new(false));
    let state: GatewayStateHandle = Default::default();
    let (control, task, _directory, _requests) =
        setup_with_gateway_state(failed.clone(), "mock", 30, None, state.clone()).await;
    let first = control
        .offer(channel("replayed-orphan"), "inbound".into())
        .await
        .unwrap();
    control.command(first.id, Command::Hangup).await.unwrap();
    state
        .lock()
        .unwrap()
        .channels
        .push(channel("replayed-orphan"));
    failed.store(true, Ordering::SeqCst);
    assert!(
        control
            .offer(channel("replayed-orphan"), "inbound".into())
            .await
            .is_err()
    );
    assert!(!control.ready());
    assert_eq!(
        control.store.resources().await.unwrap(),
        ["replayed-orphan"]
    );
    failed.store(false, Ordering::SeqCst);
    control.wait_ready().await.unwrap();
    assert!(control.store.resources().await.unwrap().is_empty());
    assert!(
        control
            .offer(channel("next-after-rejection"), "inbound".into())
            .await
            .is_ok()
    );
    control.shutdown().await;
    task.abort();
}

#[tokio::test]
async fn rejected_orphan_quarantine_preserves_the_active_call() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let failed = Arc::new(AtomicBool::new(false));
    let state: GatewayStateHandle = Default::default();
    let (control, task, _directory, _requests) =
        setup_with_gateway_state(failed.clone(), "mock", 30, None, state.clone()).await;
    let retired = control
        .offer(channel("retired-before-active"), "inbound".into())
        .await
        .unwrap();
    control.command(retired.id, Command::Hangup).await.unwrap();
    state
        .lock()
        .unwrap()
        .channels
        .push(channel("retired-before-active"));
    let active = control
        .offer(channel("retained-active"), "inbound".into())
        .await
        .unwrap();
    failed.store(true, Ordering::SeqCst);
    assert!(
        control
            .offer(channel("retired-before-active"), "inbound".into())
            .await
            .is_err()
    );
    assert!(!control.ready());
    failed.store(false, Ordering::SeqCst);
    let repeated = control
        .offer(channel("retained-active"), "inbound".into())
        .await;
    let saved = control.store.get(active.id).await.unwrap().unwrap();
    control.shutdown().await;
    task.abort();
    assert!(
        repeated.is_ok(),
        "quarantine rejected the already-owned live channel"
    );
    assert_eq!(repeated.unwrap().id, active.id);
    assert_eq!(saved.phase, Phase::Ringing);
}

#[tokio::test]
async fn malformed_ingress_cannot_delete_live_root_or_peer_channels() {
    let (control, task, _directory, requests) =
        setup_with_provider(Default::default(), "disabled", 30).await;
    let live = channel("protected-live-root");
    let call = control.offer(live.clone(), "inbound".into()).await.unwrap();
    let resources = control.store.resources().await.unwrap();
    assert!(resources.contains(&live.id));
    assert!(resources.iter().any(|id| id.ends_with("-peer-0")));
    assert!(resources.iter().any(|id| id.ends_with("-peer-1")));
    let requests_before = requests.lock().unwrap().clone();
    for id in &resources {
        let mut malformed = channel(id);
        malformed.creationtime = None;
        assert!(control.offer(malformed, "inbound".into()).await.is_err());
    }
    let replay = control.offer(live, "inbound".into()).await;
    let saved = control.store.get(call.id).await.unwrap().unwrap();
    let resources_after = control.store.resources().await.unwrap();
    let requests_after = requests.lock().unwrap().clone();
    let ready = control.ready();
    control.shutdown().await;
    task.abort();
    assert_eq!(replay.unwrap().id, call.id);
    assert_eq!(saved.phase, Phase::Ringing);
    assert_eq!(resources_after, resources);
    assert_eq!(requests_after, requests_before);
    assert!(ready);
}

#[tokio::test]
async fn gateway_admission_preserves_numbers_and_normalizes_unavailable_callers() {
    let (control, task, _directory) = setup_with_faults(Default::default()).await;
    for (id, direction, number, expected) in [
        ("numbered-incoming", "inbound", "1", "1"),
        ("withheld-incoming", "inbound", "withheld", "anonymous"),
        ("numbered-outgoing", "outbound", "2", "2"),
    ] {
        let mut offered = channel(id);
        offered.caller.number = number.into();
        let call = control.offer(offered, direction.into()).await.unwrap();
        assert_eq!(call.caller, expected);
        assert_eq!(call.direction, direction);
        assert_eq!(
            control.store.get(call.id).await.unwrap().unwrap().caller,
            expected
        );
        control.command(call.id, Command::Hangup).await.unwrap();
    }
    control.shutdown().await;
    task.abort();
}

async fn setup_replay_events() -> (
    Control,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
    Requests,
    tokio::sync::broadcast::Sender<String>,
) {
    let (frames, _) = tokio::sync::broadcast::channel::<String>(16);
    let (control, task, directory, requests) =
        setup_with_event_source(Default::default(), "disabled", 30, Some(frames.clone())).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while frames.receiver_count() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    (control, task, directory, requests, frames)
}

fn endpoint_event(id: &str, call: uuid::Uuid) -> String {
    serde_json::json!({
        "type":"StasisStart",
        "channel":{"id":id,"state":"Up","creationtime":"2026-11-15T05:32:13.000+0900"},
        "args":["endpoint",call.to_string()]
    })
    .to_string()
}

async fn gateway_event_barrier(
    control: &Control,
    frames: &tokio::sync::broadcast::Sender<String>,
    requests: &Requests,
    root: &Channel,
    marker: &str,
) -> phone_server::model::Call {
    frames
        .send(endpoint_event(marker, uuid::Uuid::nil()))
        .unwrap();
    let expected = format!("/ari/channels/{marker}");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !requests.lock().unwrap().contains(&expected) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    control.offer(root.clone(), "inbound".into()).await.unwrap()
}

#[tokio::test]
async fn valid_peer_replays_preserve_ringing_and_connected_calls() {
    let (control, task, _directory, requests, frames) = setup_replay_events().await;
    let root = channel("valid-peer-root");
    let call = control.offer(root.clone(), "inbound".into()).await.unwrap();
    let peers: Vec<_> = control
        .store
        .resources()
        .await
        .unwrap()
        .into_iter()
        .filter(|id| id != &root.id)
        .collect();
    assert_eq!(peers.len(), 2);
    let before = requests.lock().unwrap().clone();
    for peer in &peers {
        assert!(
            control
                .offer(channel(peer), "inbound".into())
                .await
                .is_err()
        );
    }
    assert_eq!(*requests.lock().unwrap(), before);
    assert_eq!(
        control.store.get(call.id).await.unwrap().unwrap().phase,
        Phase::Ringing
    );
    frames.send(endpoint_event(&peers[0], call.id)).unwrap();
    let connected =
        gateway_event_barrier(&control, &frames, &requests, &root, "valid-peer-connected").await;
    assert_eq!(connected.phase, Phase::Connected);
    let before = requests.lock().unwrap().clone();
    assert!(
        control
            .offer(channel(&peers[0]), "inbound".into())
            .await
            .is_err()
    );
    let after = requests.lock().unwrap().clone();
    let saved = control.store.get(call.id).await.unwrap().unwrap();
    control.shutdown().await;
    task.abort();
    assert_eq!(after, before);
    assert_eq!(saved.phase, Phase::Connected);
}

#[tokio::test]
async fn unavailable_read_aloud_tracks_no_media_channel() {
    let (control, task, _directory, requests, frames) = setup_replay_events().await;
    let root = channel("read-aloud-root");
    let call = control.offer(root.clone(), "inbound".into()).await.unwrap();
    let peers: Vec<_> = control
        .store
        .resources()
        .await
        .unwrap()
        .into_iter()
        .filter(|id| id != &root.id)
        .collect();
    frames.send(endpoint_event(&peers[0], call.id)).unwrap();
    let connected =
        gateway_event_barrier(&control, &frames, &requests, &root, "read-aloud-connected").await;
    assert_eq!(connected.phase, Phase::Connected);
    let tracked = control.store.resources().await.unwrap();
    for _ in 0..3 {
        assert!(
            control
                .command(
                    call.id,
                    Command::Speak {
                        text: "読み上げ".into()
                    }
                )
                .await
                .is_err()
        );
    }
    let after = control.store.resources().await.unwrap();
    control.shutdown().await;
    task.abort();
    assert_eq!(after, tracked);
}

#[tokio::test]
async fn endpoint_replays_preserve_the_winner_and_discard_late_losers() {
    let (control, task, _directory, requests, frames) = setup_replay_events().await;
    let root = channel("endpoint-replay-root");
    let call = control.offer(root.clone(), "inbound".into()).await.unwrap();
    let peers: Vec<_> = control
        .store
        .resources()
        .await
        .unwrap()
        .into_iter()
        .filter(|id| id != &root.id)
        .collect();
    assert_eq!(peers.len(), 2);
    frames.send(endpoint_event(&peers[0], call.id)).unwrap();
    let connected =
        gateway_event_barrier(&control, &frames, &requests, &root, "endpoint-connected").await;
    assert_eq!(connected.phase, Phase::Connected);
    let winner_path = format!("/ari/channels/{}", peers[0]);
    let loser_path = format!("/ari/channels/{}", peers[1]);
    let winner_before = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|p| *p == &winner_path)
        .count();
    assert_eq!(winner_before, 0);
    let loser_before = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|p| *p == &loser_path)
        .count();
    let before_replay = requests.lock().unwrap().len();
    frames.send(endpoint_event(&peers[0], call.id)).unwrap();
    let repeated = gateway_event_barrier(
        &control,
        &frames,
        &requests,
        &root,
        "endpoint-winner-replayed",
    )
    .await;
    let winner_after = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|p| *p == &winner_path)
        .count();
    assert!(
        requests.lock().unwrap()[before_replay..]
            .iter()
            .all(|path| path == "/ari/channels/endpoint-winner-replayed")
    );
    frames.send(endpoint_event(&peers[1], call.id)).unwrap();
    let after_loser = gateway_event_barrier(
        &control,
        &frames,
        &requests,
        &root,
        "endpoint-loser-discarded",
    )
    .await;
    let loser_after = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|p| *p == &loser_path)
        .count();
    control.shutdown().await;
    task.abort();
    assert_eq!(repeated.phase, Phase::Connected);
    assert_eq!(after_loser.phase, Phase::Connected);
    assert_eq!(winner_after, winner_before);
    assert_eq!(loser_after, loser_before + 1);
}

#[tokio::test]
async fn failed_busy_channel_cleanup_quarantines_the_orphan_and_preserves_the_call() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let failed = Arc::new(AtomicBool::new(false));
    let (control, task, _directory) = setup_with_faults(failed.clone()).await;
    let call = control
        .offer(channel("busy-owner"), "inbound".into())
        .await
        .unwrap();
    let resources = control.store.resources().await.unwrap();
    assert!(
        control
            .offer(channel("busy-discarded-orphan"), "inbound".into())
            .await
            .is_err()
    );
    assert_eq!(control.store.resources().await.unwrap(), resources);
    assert!(control.ready());
    failed.store(true, Ordering::SeqCst);
    assert!(
        control
            .offer(channel("busy-unconsumed-orphan"), "inbound".into())
            .await
            .is_err()
    );
    assert!(!control.ready());
    assert!(
        control
            .store
            .resources()
            .await
            .unwrap()
            .contains(&"busy-unconsumed-orphan".into())
    );
    assert_eq!(
        control.store.get(call.id).await.unwrap().unwrap().phase,
        Phase::Ringing
    );
    failed.store(false, Ordering::SeqCst);
    assert_eq!(
        control
            .offer(channel("busy-owner"), "inbound".into())
            .await
            .unwrap()
            .id,
        call.id
    );
    control.command(call.id, Command::Hangup).await.unwrap();
    control.wait_ready().await.unwrap();
    assert!(control.store.resources().await.unwrap().is_empty());
    assert!(
        control
            .offer(channel("after-busy-orphan"), "inbound".into())
            .await
            .is_ok()
    );
    control.shutdown().await;
    task.abort();
}

async fn wait_new_gateway_call(
    control: &Control,
    known: &[uuid::Uuid],
) -> phone_server::model::Call {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(call) = control
                .store
                .calls()
                .await
                .unwrap()
                .into_iter()
                .find(|call| !known.contains(&call.id) && call.phase == Phase::Ringing)
            {
                return call;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

async fn wait_gateway_marker(
    control: &Control,
    frames: &tokio::sync::broadcast::Sender<String>,
    requests: &Requests,
    marker: &str,
) {
    frames
        .send(endpoint_event(marker, uuid::Uuid::nil()))
        .unwrap();
    let expected = format!("/ari/channels/{marker}");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !requests.lock().unwrap().contains(&expected) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(control.maintenance(true).await.is_err());
}

#[tokio::test]
async fn gateway_start_events_preserve_inbound_and_outbound_identity() {
    let (control, task, _directory, _requests, frames) = setup_replay_events().await;
    for (id, direction, number, expected) in [
        ("wire-inbound", "inbound", "1", "1"),
        ("wire-outbound", "outbound", "handset", "2"),
    ] {
        let known: Vec<_> = control
            .store
            .calls()
            .await
            .unwrap()
            .iter()
            .map(|call| call.id)
            .collect();
        frames.send(serde_json::json!({
            "type":"StasisStart",
            "channel":{"id":id,"state":"Ring","creationtime":"2026-11-15T05:32:13.000+0900","caller":{"number":number}},
            "args":[direction,"2"]
        }).to_string()).unwrap();
        let call = wait_new_gateway_call(&control, &known).await;
        assert_eq!(call.direction, direction);
        assert_eq!(call.caller, expected);
        control.command(call.id, Command::Hangup).await.unwrap();
    }
    control.shutdown().await;
    task.abort();
}

#[tokio::test]
async fn browser_originator_events_dial_the_carrier_for_the_owned_root() {
    let (control, task, _directory, requests, frames) = setup_replay_events().await;
    let call = control.originate("2".into()).await.unwrap();
    let resources = control.store.resources().await.unwrap();
    assert_eq!(resources.len(), 1);
    let root = resources[0].clone();
    let before = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.as_str() == "/ari/channels")
        .count();
    for (id, marker, increment) in [
        (
            "foreign-originator".to_owned(),
            "foreign-originator-barrier",
            0,
        ),
        (root, "owned-originator-barrier", 1),
    ] {
        frames
            .send(
                serde_json::json!({
                    "type":"StasisStart",
                    "channel":{"id":id,"state":"Up","creationtime":"2026-11-15T05:32:13.000+0900"},
                    "args":["originator"]
                })
                .to_string(),
            )
            .unwrap();
        wait_gateway_marker(&control, &frames, &requests, marker).await;
        assert_eq!(
            requests
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.as_str() == "/ari/channels")
                .count(),
            before + increment
        );
    }
    assert_eq!(
        control.store.get(call.id).await.unwrap().unwrap().phase,
        Phase::Ringing
    );
    control.shutdown().await;
    task.abort();
}

#[tokio::test]
async fn gateway_terminal_events_end_the_owned_root_and_winning_peer() {
    for event in ["StasisEnd", "ChannelDestroyed"] {
        for winner in [false, true] {
            let (control, task, _directory, requests, frames) = setup_replay_events().await;
            let root = channel("terminal-event-root");
            let call = control.offer(root.clone(), "inbound".into()).await.unwrap();
            let target = if winner {
                let peer = control
                    .store
                    .resources()
                    .await
                    .unwrap()
                    .into_iter()
                    .find(|id| id != &root.id)
                    .unwrap();
                frames.send(endpoint_event(&peer, call.id)).unwrap();
                assert_eq!(
                    gateway_event_barrier(
                        &control,
                        &frames,
                        &requests,
                        &root,
                        "terminal-connected"
                    )
                    .await
                    .phase,
                    Phase::Connected
                );
                peer
            } else {
                root.id
            };
            frames.send(serde_json::json!({
                "type":event,
                "channel":{"id":target,"state":"Down","creationtime":"2026-11-15T05:32:13.000+0900"}
            }).to_string()).unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if control.store.get(call.id).await.unwrap().unwrap().phase == Phase::Ended
                        && !control.store.owns_line(call.id)
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            control.shutdown().await;
            task.abort();
        }
    }
}

#[tokio::test]
async fn resuming_admission_cannot_reconcile_away_a_live_call() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let failed = Arc::new(AtomicBool::new(false));
    let (control, task, _directory, _requests) =
        setup_with_provider(failed.clone(), "disabled", 30).await;
    let call = control
        .offer(channel("resume-owner"), "inbound".into())
        .await
        .unwrap();
    failed.store(true, Ordering::SeqCst);
    assert!(
        control
            .offer(channel("resume-orphan"), "inbound".into())
            .await
            .is_err()
    );
    failed.store(false, Ordering::SeqCst);
    assert!(control.maintenance(false).await.is_err());
    let saved = control.store.get(call.id).await.unwrap().unwrap();
    let owned = control.store.owns_line(call.id);
    control.shutdown().await;
    task.abort();
    assert_eq!(saved.phase, Phase::Ringing);
    assert!(owned);
}

#[tokio::test]
async fn quarantined_calls_keep_their_deadline_and_end_once() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let failed = Arc::new(AtomicBool::new(false));
    let (control, task, _directory, _requests) =
        setup_with_provider(failed.clone(), "disabled", 3).await;
    let call = control
        .offer(channel("deadline-owner"), "inbound".into())
        .await
        .unwrap();
    failed.store(true, Ordering::SeqCst);
    assert!(
        control
            .offer(channel("deadline-orphan"), "inbound".into())
            .await
            .is_err()
    );
    assert!(!control.ready());
    failed.store(false, Ordering::SeqCst);
    let ended = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let saved = control.store.get(call.id).await.unwrap().unwrap();
            if saved.phase == Phase::Ended && !control.store.owns_line(call.id) {
                return saved;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    control.wait_ready().await.unwrap();
    let ended_events = control
        .store
        .events_after(0)
        .await
        .unwrap()
        .iter()
        .filter(|event| {
            event.call.as_ref().is_some_and(|saved| saved.id == call.id) && event.kind == "ended"
        })
        .count();
    assert_eq!(ended.reason.as_deref(), Some("no_answer"));
    assert_eq!(ended_events, 1);
    assert!(
        control
            .offer(channel("after-quarantine-deadline"), "inbound".into())
            .await
            .is_ok()
    );
    control.shutdown().await;
    task.abort();
}

#[cfg(feature = "plugin-ai")]
#[tokio::test]
async fn ringing_deadlines_select_reception_and_outgoing_no_answer() {
    for direction in ["inbound", "outbound"] {
        let (control, task, _directory, requests) =
            setup_with_provider(Default::default(), "mock", 1).await;
        let call = control
            .offer(channel("reception-deadline"), direction.into())
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let events = control.store.events_after(0).await.unwrap();
                if direction == "inbound" {
                    if events.iter().any(|event| {
                        event.call.as_ref().is_some_and(|saved| {
                            saved.id == call.id && saved.phase == Phase::Voicemail
                        })
                    }) {
                        break;
                    }
                } else {
                    let saved = control.store.get(call.id).await.unwrap().unwrap();
                    if saved.phase == Phase::Ended {
                        assert_eq!(saved.reason.as_deref(), Some("no_answer"));
                        assert!(!events.iter().any(|event| event.call.as_ref().is_some_and(
                            |saved| saved.id == call.id && saved.phase == Phase::Voicemail
                        )));
                        break;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        control.shutdown().await;
        task.abort();
        let observed = requests.lock().unwrap();
        let stop = format!("/ari/recordings/live/hp-{}/stop", call.id);
        if direction == "inbound" {
            let stopped = observed.iter().position(|path| path == &stop).unwrap();
            let deleted = observed
                .iter()
                .position(|path| path == "/ari/channels/reception-deadline")
                .unwrap();
            assert!(stopped < deleted);
        } else {
            assert!(!observed.contains(&stop));
        }
    }
}

#[cfg(feature = "plugin-ai")]
#[tokio::test]
async fn each_cleanup_failure_retains_ownership_until_retry_succeeds() {
    for fault in [
        CleanupFault::Recording,
        CleanupFault::Channel,
        CleanupFault::Bridge,
    ] {
        let state: GatewayStateHandle = Default::default();
        let (control, task, _directory, _requests) =
            setup_with_gateway_state(Default::default(), "mock", 1, None, state.clone()).await;
        let call = control
            .offer(channel("isolated-cleanup"), "inbound".into())
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while control.store.get(call.id).await.unwrap().unwrap().phase != Phase::Voicemail {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        state.lock().unwrap().cleanup_fault = Some(fault);
        assert!(control.command(call.id, Command::Hangup).await.is_err());
        assert!(control.store.owns_line(call.id));
        assert!(control.deletable(call.id).await.is_err());
        assert!(
            control
                .offer(channel("blocked-during-cleanup"), "inbound".into())
                .await
                .is_err()
        );
        state.lock().unwrap().cleanup_fault = None;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while control.store.owns_line(call.id) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        control.wait_ready().await.unwrap();
        assert!(
            control
                .offer(channel("after-isolated-cleanup"), "inbound".into())
                .await
                .is_ok()
        );
        control.shutdown().await;
        task.abort();
    }
}

#[tokio::test]
async fn normal_cleanup_preserves_a_pending_gateway_ingress() {
    let state: GatewayStateHandle = Default::default();
    let (control, task, _directory, requests) =
        setup_with_gateway_state(Default::default(), "disabled", 30, None, state.clone()).await;
    let first = control
        .offer(channel("ordinary-cleanup"), "inbound".into())
        .await
        .unwrap();
    let pending = channel("pending-gateway-ingress");
    state.lock().unwrap().channels.push(pending.clone());
    control.command(first.id, Command::Hangup).await.unwrap();
    let pending_deleted = requests
        .lock()
        .unwrap()
        .iter()
        .any(|path| path == "/ari/channels/pending-gateway-ingress");
    let next = control.offer(pending, "inbound".into()).await.unwrap();
    control.shutdown().await;
    task.abort();
    assert!(!pending_deleted);
    assert_ne!(next.id, first.id);
    assert_eq!(next.phase, Phase::Ringing);
}

#[tokio::test]
async fn endpoint_identity_guards_reject_each_invalid_condition() {
    let (control, task, _directory, requests, frames) = setup_replay_events().await;
    let root = channel("endpoint-identity-root");
    let call = control.offer(root.clone(), "inbound".into()).await.unwrap();
    let peer = control
        .store
        .resources()
        .await
        .unwrap()
        .into_iter()
        .find(|id| id != &root.id)
        .unwrap();
    frames
        .send(endpoint_event(&peer, uuid::Uuid::nil()))
        .unwrap();
    let after_wrong_call =
        gateway_event_barrier(&control, &frames, &requests, &root, "wrong-call-rejected").await;
    let peer_path = format!("/ari/channels/{peer}");
    assert!(!requests.lock().unwrap().contains(&peer_path));
    frames
        .send(endpoint_event("untracked-endpoint", call.id))
        .unwrap();
    let after_wrong_peer =
        gateway_event_barrier(&control, &frames, &requests, &root, "wrong-peer-rejected").await;
    let untracked_deleted = requests
        .lock()
        .unwrap()
        .iter()
        .any(|path| path == "/ari/channels/untracked-endpoint");
    control.shutdown().await;
    task.abort();
    assert_eq!(after_wrong_call.phase, Phase::Ringing);
    assert_eq!(after_wrong_peer.phase, Phase::Ringing);
    assert!(untracked_deleted);
}
