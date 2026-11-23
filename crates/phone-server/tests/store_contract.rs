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
async fn calls_stored_before_directory_labels_load_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("phone.sqlite");
    let store = Store::open(&path).await.unwrap();
    let mut labeled = Call::offered("0775511234".into(), "inbound".into(), 100);
    labeled.caller_label = Some("さくら病院（病院・診療所）".into());
    store.save(&labeled, "incoming", 100).await.unwrap();
    drop(store);
    let mut legacy = Call::offered("0312345678".into(), "inbound".into(), 50);
    legacy.phase = Phase::Ended;
    legacy.ended_at = Some(51);
    let mut payload = serde_json::to_value(&legacy).unwrap();
    payload.as_object_mut().unwrap().remove("caller_label");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO calls(id,phase,started_at,ended_at,payload) VALUES(?1,'ended',50,51,?2)",
            [legacy.id.to_string(), payload.to_string()],
        )
        .unwrap();
    let reopened = Store::open(&path).await.unwrap();
    assert_eq!(reopened.get(legacy.id).await.unwrap(), Some(legacy));
    assert_eq!(
        reopened
            .get(labeled.id)
            .await
            .unwrap()
            .unwrap()
            .caller_label,
        labeled.caller_label
    );
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

fn ingress(id: &str, created: &str) -> phone_server::gateway::IngressId {
    phone_server::gateway::IngressId::new(&phone_server::gateway::Channel {
        id: id.into(),
        state: "Ring".into(),
        creationtime: Some(created.into()),
        caller: Default::default(),
    })
    .unwrap()
}

#[tokio::test]
async fn ingress_consumption_survives_deletion_retention_and_restart() {
    use phone_server::{gateway::CallRoot, store::Admission};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    let store = Store::open(&path).await.unwrap();
    let identity = ingress("reused-channel", "2026-11-15T05:32:13.000+0900");
    let mut first = Call::offered("1".into(), "inbound".into(), 100);
    assert!(matches!(
        store
            .admit(&first, &CallRoot::Gateway(identity.clone()), 100)
            .await
            .unwrap(),
        Admission::Created(_)
    ));
    first.phase = Phase::Ended;
    first.ended_at = Some(101);
    store.save(&first, "ended", 101).await.unwrap();
    store.clear_resources().await.unwrap();
    store.delete_finished(first.id, 102).await.unwrap();
    store.prune_history(103).await.unwrap();
    drop(store);
    let store = Store::open(&path).await.unwrap();
    let canonical = ingress("reused-channel", "2026-11-14T20:32:13Z");
    assert!(store.ingress_admitted(&canonical).await.unwrap());
    let fresh = Call::offered("1".into(), "inbound".into(), 200);
    assert!(matches!(
        store
            .admit(&fresh, &CallRoot::Gateway(canonical), 200)
            .await
            .unwrap(),
        Admission::Replayed
    ));
    assert!(store.calls().await.unwrap().is_empty());
    assert!(store.resources().await.unwrap().is_empty());
    assert!(store.events_after(0).await.unwrap().is_empty());
    let generation = ingress("reused-channel", "2026-11-14T20:32:13.001Z");
    assert!(matches!(
        store
            .admit(&fresh, &CallRoot::Gateway(generation), 200)
            .await
            .unwrap(),
        Admission::Created(_)
    ));
    assert_eq!(store.calls().await.unwrap().len(), 1);
    let stored: (String, i64) = store
        .with(|connection| {
            Ok(connection.query_row(
                "SELECT typeof(fingerprint),length(fingerprint) FROM call_ingress LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(stored, ("blob".into(), 32));
    let backup = directory.path().join("admission-backup.sqlite");
    store.backup(backup.clone()).await.unwrap();
    let restored = Store::open(&backup).await.unwrap();
    assert!(
        restored
            .ingress_admitted(&ingress("reused-channel", "2026-11-14T20:32:13Z"))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn ingress_event_and_resource_admission_roll_back_together() {
    use phone_server::{gateway::CallRoot, store::Admission};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let identity = ingress("transactional-ingress", "2026-11-14T20:32:13Z");
    let call = Call::offered("1".into(), "inbound".into(), 100);
    for table in ["events", "resources"] {
        store.with(move |connection| {
            connection.execute_batch(&format!("CREATE TRIGGER reject_admission BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'synthetic admission fault'); END;"))?;
            Ok(())
        }).await.unwrap();
        assert!(
            store
                .admit(&call, &CallRoot::Gateway(identity.clone()), 100)
                .await
                .is_err()
        );
        assert!(!store.ingress_admitted(&identity).await.unwrap());
        assert!(store.calls().await.unwrap().is_empty());
        assert!(store.events_after(0).await.unwrap().is_empty());
        assert!(store.due(100).await.unwrap().is_empty());
        assert!(store.resources().await.unwrap().is_empty());
        store
            .with(|connection| {
                connection.execute_batch("DROP TRIGGER reject_admission")?;
                Ok(())
            })
            .await
            .unwrap();
    }
    assert!(matches!(
        store
            .admit(&call, &CallRoot::Gateway(identity), 100)
            .await
            .unwrap(),
        Admission::Created(_)
    ));
    assert_eq!(store.resources().await.unwrap(), ["transactional-ingress"]);
    assert_eq!(store.events_after(0).await.unwrap().len(), 1);
}

#[tokio::test]
async fn ingress_schema_migration_preserves_previous_calls_and_refuses_future_versions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    let store = Store::open(&path).await.unwrap();
    let mut call = Call::offered("1".into(), "inbound".into(), 100);
    call.phase = Phase::Ended;
    call.ended_at = Some(101);
    store.save(&call, "ended", 101).await.unwrap();
    store
        .with(|connection| {
            connection.execute_batch("DROP TABLE call_ingress; PRAGMA user_version=1;")?;
            Ok(())
        })
        .await
        .unwrap();
    drop(store);
    let store = Store::open(&path).await.unwrap();
    assert_eq!(store.get(call.id).await.unwrap(), Some(call));
    let version: u32 = store
        .with(|connection| {
            Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
        })
        .await
        .unwrap();
    assert_eq!(version, 3);
    assert!(
        !store
            .ingress_admitted(&ingress("fresh-after-migration", "2026-11-14T20:32:13Z"))
            .await
            .unwrap()
    );
    store
        .with(|connection| {
            connection.pragma_update(None, "user_version", 4)?;
            Ok(())
        })
        .await
        .unwrap();
    drop(store);
    assert!(Store::open(&path).await.is_err());
}

#[tokio::test]
async fn concurrent_ingress_admission_commits_one_initial_event() {
    use phone_server::{gateway::CallRoot, store::Admission};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let root = CallRoot::Gateway(ingress("concurrent-ingress", "2026-11-14T20:32:13Z"));
    let first = Call::offered("1".into(), "inbound".into(), 100);
    let second = Call::offered("1".into(), "inbound".into(), 100);
    let (first, second) = tokio::join!(
        store.admit(&first, &root, 100),
        store.admit(&second, &root, 100)
    );
    let results = [first.unwrap(), second.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Admission::Created(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Admission::Replayed))
            .count(),
        1
    );
    assert_eq!(store.calls().await.unwrap().len(), 1);
    assert_eq!(store.events_after(0).await.unwrap().len(), 1);
    assert_eq!(store.due(100).await.unwrap().len(), 1);
    assert_eq!(store.resources().await.unwrap(), ["concurrent-ingress"]);
}

#[tokio::test]
async fn a_failed_ingress_migration_preserves_the_previous_schema() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    let store = Store::open(&path).await.unwrap();
    let mut call = Call::offered("1".into(), "inbound".into(), 100);
    call.phase = Phase::Ended;
    call.ended_at = Some(101);
    store.save(&call, "ended", 101).await.unwrap();
    store.with(|connection| {
        connection.execute_batch("DROP TABLE call_ingress; PRAGMA user_version=1; UPDATE calls SET payload='malformed';")?;
        Ok(())
    }).await.unwrap();
    drop(store);
    assert!(Store::open(&path).await.is_err());
    let database = rusqlite::Connection::open(&path).unwrap();
    let version: u32 = database
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    let tables: i64 = database
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='call_ingress'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
}
