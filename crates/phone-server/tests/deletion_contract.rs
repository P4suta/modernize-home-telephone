use phone_server::{
    model::{Call, Phase},
    store::Store,
};

#[tokio::test]
async fn deleting_history_cannot_be_undone_by_a_late_post_call_worker() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut call = Call::offered("12345".into(), "inbound".into(), 10);
    store.save(&call, "incoming", 10).await.unwrap();
    assert!(store.delete_finished(call.id, 11).await.is_err());
    call.phase = Phase::Ended;
    call.ended_at = Some(12);
    store.save(&call, "ended", 12).await.unwrap();
    let event = store
        .update_finished(call.id, "summary", 13, |call| {
            call.summary = Some("Private message.".into())
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        event.call.unwrap().summary.as_deref(),
        Some("Private message.")
    );
    let deleted = store.delete_finished(call.id, 14).await.unwrap().unwrap();
    assert_eq!(deleted.kind, "deleted");
    assert!(
        store
            .update_finished(call.id, "summary", 15, |call| call.summary =
                Some("Late response.".into()))
            .await
            .unwrap()
            .is_none()
    );
    assert!(store.get(call.id).await.unwrap().is_none());
    let serialized = serde_json::to_string(&store.events_after(0).await.unwrap()).unwrap();
    assert!(!serialized.contains("Private message"));
    assert!(!serialized.contains("12345"));
    assert_eq!(store.due(100).await.unwrap().len(), 1);
}
