use phone_server::{
    model::{Call, Phase, Rule, Settings},
    store::Store,
};

#[tokio::test]
async fn a_second_service_cannot_recover_another_services_live_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    let first = Store::open(&path).await.unwrap();
    assert!(Store::open(&path).await.is_err());
    drop(first);
    assert!(Store::open(&path).await.is_ok());
}

#[tokio::test]
async fn committed_calls_events_and_settings_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("phone.sqlite");
    let store = Store::open(&path).await.unwrap();
    let call = Call::offered("0312345678".into(), "inbound".into(), 100);
    let event = store.save(&call, "incoming", 100).await.unwrap();
    assert!(event.id > 0);
    assert_eq!(event.call.as_ref(), Some(&call));
    let settings = Settings {
        rules: [("0312345678".into(), Rule::Allow)].into(),
        ..Settings::default()
    };
    store.set_settings(&settings).await.unwrap();
    drop(store);
    let reopened = Store::open(&path).await.unwrap();
    assert_eq!(reopened.get(call.id).await.unwrap(), Some(call.clone()));
    assert_eq!(reopened.calls().await.unwrap(), vec![call]);
    assert_eq!(reopened.settings().await.unwrap(), settings);
}

#[tokio::test]
async fn database_also_refuses_two_active_external_calls() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("phone.sqlite")).await.unwrap();
    let first = Call::offered("1".into(), "inbound".into(), 100);
    let second = Call::offered("2".into(), "outbound".into(), 101);
    store.save(&first, "incoming", 100).await.unwrap();
    assert!(store.save(&second, "incoming", 101).await.is_err());
    assert_eq!(store.calls().await.unwrap(), vec![first]);
}

#[tokio::test]
async fn recovery_marks_interruption_once_and_releases_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("phone.sqlite")).await.unwrap();
    let call = Call::offered("1".into(), "inbound".into(), 100);
    store.save(&call, "incoming", 100).await.unwrap();
    assert_eq!(store.recover(200).await.unwrap(), 1);
    assert_eq!(store.recover(201).await.unwrap(), 0);
    let recovered = store.get(call.id).await.unwrap().unwrap();
    assert_eq!(recovered.phase, Phase::Ended);
    assert_eq!(recovered.reason.as_deref(), Some("service_restart"));
    assert_eq!(recovered.ended_at, Some(200));
    store
        .save(
            &Call::offered("2".into(), "inbound".into(), 202),
            "incoming",
            202,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn delivery_retries_preserve_event_identity_and_obey_deadlines() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("phone.sqlite")).await.unwrap();
    let call = Call::offered("1".into(), "inbound".into(), 100);
    let event = store.save(&call, "incoming", 100).await.unwrap();
    let due = store.due(100).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, event.id);
    assert_eq!(due[0].attempts, 0);
    store.retry(event.id, 100).await.unwrap();
    assert!(store.due(104).await.unwrap().is_empty());
    let retried = store.due(105).await.unwrap();
    assert_eq!(retried[0].id, event.id);
    assert_eq!(retried[0].attempts, 1);
    store.delivered(event.id).await.unwrap();
    assert!(store.due(u32::MAX.into()).await.unwrap().is_empty());
}
