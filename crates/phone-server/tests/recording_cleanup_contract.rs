use phone_server::{
    model::{Call, Phase, Recording, Transcript},
    store::Store,
};

#[tokio::test]
async fn finished_copies_and_deleted_history_schedule_durable_gateway_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("phone.sqlite");
    let store = Store::open(&path).await.unwrap();
    let mut call = Call::offered("123".into(), "inbound".into(), 1);
    call.recording = Recording::Pending;
    store.save(&call, "recording", 1).await.unwrap();
    assert!(store.recording_cleanup(1).await.unwrap().is_empty());
    call.phase = Phase::Ended;
    call.ended_at = Some(2);
    call.recording = Recording::Available { bytes: 42 };
    call.transcript.push(Transcript {
        item_id: "text".into(),
        speaker: "mixed".into(),
        text: "private transcript".into(),
    });
    store.save(&call, "recording_ready", 2).await.unwrap();
    assert_eq!(
        store.recording_cleanup(2).await.unwrap(),
        [format!("hp-{}", call.id)]
    );
    assert!(store.calls().await.unwrap()[0].transcript.is_empty());
    assert_eq!(
        store.get(call.id).await.unwrap().unwrap().transcript,
        call.transcript
    );
    assert!(
        store
            .events_after(0)
            .await
            .unwrap()
            .last()
            .unwrap()
            .call
            .as_ref()
            .unwrap()
            .transcript
            .is_empty()
    );
    store.delete_finished(call.id, 3).await.unwrap();
    drop(store);
    let reopened = Store::open(&path).await.unwrap();
    assert_eq!(
        reopened.recording_cleanup(3).await.unwrap(),
        [format!("hp-{}", call.id)]
    );
    reopened
        .retry_recording_cleanup(format!("hp-{}", call.id), 3)
        .await
        .unwrap();
    assert!(reopened.recording_cleanup(62).await.unwrap().is_empty());
    assert_eq!(reopened.recording_cleanup(63).await.unwrap().len(), 1);
    reopened
        .recording_cleaned(format!("hp-{}", call.id))
        .await
        .unwrap();
    assert!(reopened.recording_cleanup(100).await.unwrap().is_empty());
}
