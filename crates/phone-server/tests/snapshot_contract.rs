use phone_server::{
    model::{Call, Phase},
    snapshot,
    store::Store,
};

#[tokio::test]
async fn snapshots_contain_a_consistent_database_and_refuse_live_calls() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let mut call = Call::offered("123".into(), "inbound".into(), 1);
    store.save(&call, "incoming", 1).await.unwrap();
    assert!(snapshot::create(&store, directory.path()).await.is_err());
    call.phase = Phase::Ended;
    call.ended_at = Some(2);
    store.save(&call, "ended", 2).await.unwrap();
    tokio::fs::create_dir_all(directory.path().join("recordings"))
        .await
        .unwrap();
    tokio::fs::write(
        directory
            .path()
            .join("recordings")
            .join(format!("{}.wav", call.id)),
        b"audio bytes",
    )
    .await
    .unwrap();
    let path = snapshot::create(&store, directory.path()).await.unwrap();
    let restored = Store::open(&path.join("phone.sqlite")).await.unwrap();
    assert_eq!(restored.get(call.id).await.unwrap(), Some(call.clone()));
    assert_eq!(
        tokio::fs::read(path.join("recordings").join(format!("{}.wav", call.id)))
            .await
            .unwrap(),
        b"audio bytes"
    );
    assert!(path.join("manifest.json").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
