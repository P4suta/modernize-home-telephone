use phone_server::{
    model::{Call, Phase, Recording},
    store::Store,
};

#[tokio::test]
async fn ended_calls_remain_protected_until_gateway_cleanup_releases_ownership() {
    let root = tempfile::tempdir().unwrap();
    let store = Store::open(&root.path().join("db")).await.unwrap();
    let mut call = Call::offered("1".into(), "inbound".into(), 1);
    call.phase = Phase::Ended;
    call.ended_at = Some(2);
    call.recording = Recording::Pending;
    store.save(&call, "ended", 2).await.unwrap();
    store.own_line(Some(call.id));
    assert!(store.delete_finished(call.id, 3).await.is_err());
    store.own_line(None);
    assert!(store.delete_finished(call.id, 3).await.unwrap().is_some());
}
