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
    let router = Router::new()
        .route(
            "/ari/applications/homephone",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"channel_ids":[]})) }),
        )
        .route(
            "/ari/channels",
            axum::routing::get(|| async { axum::Json(Vec::<String>::new()) })
                .post(|| async { StatusCode::NO_CONTENT }),
        )
        .route(
            "/ari/bridges",
            axum::routing::get(|| async { axum::Json(Vec::<String>::new()) }),
        )
        .route(
            "/ari/events",
            axum::routing::get(|socket: axum::extract::WebSocketUpgrade| async {
                socket
                    .on_upgrade(|mut socket| async move { while socket.recv().await.is_some() {} })
            }),
        )
        .fallback(any(move |method: axum::http::Method| {
            let failed = failed.clone();
            async move {
                if method == axum::http::Method::DELETE
                    && failed.load(std::sync::atomic::Ordering::SeqCst)
                {
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    StatusCode::NO_CONTENT
                }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config {
        data_dir: dir.path().into(),
        ..Config::default()
    };
    config.ari.url = format!("http://{address}/ari/");
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
    (control, task, dir)
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
