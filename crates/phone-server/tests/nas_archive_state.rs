use phone_server::{
    model::{Call, Phase, Recording},
    store::{ArchiveWork, ArchivedRecording, Store},
};

async fn finished(store: &Store) -> Call {
    let mut call = Call::offered("123".into(), "inbound".into(), 10);
    call.phase = Phase::Ended;
    call.ended_at = Some(20);
    call.recording = Recording::Available { bytes: 64 };
    call.processing = "complete".into();
    store.save(&call, "ended", 20).await.unwrap();
    call
}

#[cfg(feature = "plugin-nas")]
#[tokio::test]
async fn one_changed_local_copy_does_not_keep_other_archived_copies() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let recordings = directory.path().join("recordings");
    tokio::fs::create_dir_all(&recordings).await.unwrap();
    let mut released = vec![];
    let mut changed = None;
    for index in 0..6_u8 {
        let call = finished(&store).await;
        let path = recordings.join(format!("{}.wav", call.id));
        tokio::fs::write(&path, [index; 64]).await.unwrap();
        let digest = phone_plugin_nas::identify(&path, 64)
            .await
            .unwrap()
            .digest();
        store.archive_tasks(21).await.unwrap();
        let receipt = ArchivedRecording {
            bytes: 64,
            digest: if index == 0 { "1".repeat(64) } else { digest },
        };
        assert!(store.commit_archive(call.id, receipt, 23).await.unwrap());
        if index == 0 {
            changed = Some(path);
        } else {
            released.push(path);
        }
    }
    let config = phone_server::config::Config {
        data_dir: directory.path().into(),
        ..Default::default()
    };
    let ari = phone_server::gateway::Ari::new(config.ari.clone(), "synthetic".into()).unwrap();
    let processor = phone_server::postcall::Processor::new(config, ari, store, None).unwrap();
    assert!(processor.release_archived().await.is_err());
    assert!(changed.unwrap().exists());
    for path in released {
        assert!(!path.exists(), "{}", path.display());
    }
}

fn identity() -> ArchivedRecording {
    ArchivedRecording {
        bytes: 64,
        digest: "1".repeat(64),
    }
}

#[tokio::test]
async fn archive_deletion_survives_restart_and_refuses_late_publication() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("phone.sqlite");
    let store = Store::open(&database).await.unwrap();
    let call = finished(&store).await;
    let tasks = store.archive_tasks(21).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(matches!(tasks[0].work, ArchiveWork::Publish { bytes: 64 }));
    store.delete_finished(call.id, 22).await.unwrap();
    assert!(!store.commit_archive(call.id, identity(), 23).await.unwrap());
    assert!(store.get(call.id).await.unwrap().is_none());
    drop(store);
    let store = Store::open(&database).await.unwrap();
    let tasks = store.archive_tasks(24).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, call.id);
    assert!(matches!(tasks[0].work, ArchiveWork::Delete));
    store.archive_deleted(call.id).await.unwrap();
    assert!(store.archive_tasks(25).await.unwrap().is_empty());
    assert!(!store.commit_archive(call.id, identity(), 26).await.unwrap());
}

#[tokio::test]
async fn archive_receipts_require_exact_pending_bytes_and_preserve_call_faults() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let call = finished(&store).await;
    assert!(!store.commit_archive(call.id, identity(), 21).await.unwrap());
    store.archive_tasks(22).await.unwrap();
    assert!(
        !store
            .commit_archive(
                call.id,
                ArchivedRecording {
                    bytes: 63,
                    ..identity()
                },
                23
            )
            .await
            .unwrap()
    );
    store
        .update_finished(call.id, "fault", 24, |call| {
            call.last_error = Some("gateway_cleanup_failed".into())
        })
        .await
        .unwrap();
    store.retry_archive(call.id, 1000, 25).await.unwrap();
    assert!(store.archive_tasks(26).await.unwrap().is_empty());
    assert!(store.commit_archive(call.id, identity(), 27).await.unwrap());
    assert!(store.archive_tasks(1000).await.unwrap().is_empty());
    assert_eq!(
        store
            .get(call.id)
            .await
            .unwrap()
            .unwrap()
            .last_error
            .as_deref(),
        Some("gateway_cleanup_failed")
    );
    let receipt = store.archived_recording(call.id).await.unwrap().unwrap();
    assert_eq!(receipt.bytes, 64);
    assert_eq!(receipt.digest, "1".repeat(64));
    assert!(!store.commit_archive(call.id, identity(), 28).await.unwrap());
}

#[tokio::test]
async fn a_successful_archive_clears_its_own_retry_error() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    let call = finished(&store).await;
    store.archive_tasks(21).await.unwrap();
    store.retry_archive(call.id, 0, 22).await.unwrap();
    assert_eq!(
        store
            .get(call.id)
            .await
            .unwrap()
            .unwrap()
            .last_error
            .as_deref(),
        Some("nas_archive_unavailable")
    );
    assert!(store.commit_archive(call.id, identity(), 23).await.unwrap());
    assert!(
        store
            .get(call.id)
            .await
            .unwrap()
            .unwrap()
            .last_error
            .is_none()
    );
}
