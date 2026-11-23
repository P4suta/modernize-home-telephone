use phone_server::{
    config::Config,
    maintenance,
    model::{Call, Phase, Recording, Settings},
    store::Store,
};

#[tokio::test]
async fn manual_retention_keeps_old_audio_and_history_until_explicit_deletion() {
    let directory = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: directory.path().into(),
        ..Config::default()
    };
    let store = Store::open(&directory.path().join("phone.sqlite"))
        .await
        .unwrap();
    store
        .set_settings(&Settings {
            retention_days: Some(0),
            ..Settings::default()
        })
        .await
        .unwrap();
    let mut call = Call::offered("123".into(), "inbound".into(), 10);
    call.phase = Phase::Ended;
    call.ended_at = Some(20);
    call.recording = Recording::Available { bytes: 3 };
    store.save(&call, "ended", 20).await.unwrap();
    let recordings = directory.path().join("recordings");
    tokio::fs::create_dir(&recordings).await.unwrap();
    let path = recordings.join(format!("{}.wav", call.id));
    tokio::fs::write(&path, [1, 2, 3]).await.unwrap();

    maintenance::enforce(&store, &config, 86400 * 400)
        .await
        .unwrap();

    assert_eq!(store.get(call.id).await.unwrap(), Some(call));
    assert_eq!(tokio::fs::read(&path).await.unwrap(), [1, 2, 3]);
}

#[cfg(feature = "plugin-nas")]
#[tokio::test]
async fn nas_archives_ignore_legacy_expiry_and_local_spool_quota() {
    for legacy_days in [0, 30] {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config {
            data_dir: directory.path().into(),
            ..Config::default()
        };
        config.storage.retention_days = 0;
        config.storage.recording_limit_bytes = 536870912;
        config.recording = Some(phone_server::config::CallRecording {
            enabled: true,
            max_seconds: 7200,
            gateway_spool_dir: directory.path().join("spool"),
            nas: Some(phone_server::config::Nas {
                archive_id: uuid::Uuid::new_v4(),
                server: "nas.invalid".into(),
                share: "home".into(),
                directory: "telephone".into(),
                port: 445,
                username_env: "NAS_USERNAME".into(),
                password_env: "NAS_PASSWORD".into(),
            }),
        });
        config.validate().unwrap();
        let store = Store::open(&directory.path().join("phone.sqlite"))
            .await
            .unwrap();
        store
            .set_settings(&Settings {
                retention_days: Some(legacy_days),
                ..Settings::default()
            })
            .await
            .unwrap();
        let mut calls = Vec::new();
        for _ in 0..6 {
            let mut call = Call::offered("123".into(), "inbound".into(), 10);
            call.phase = Phase::Ended;
            call.ended_at = Some(20);
            call.processing = "complete".into();
            call.recording = Recording::Available { bytes: 100000000 };
            store.save(&call, "ended", 20).await.unwrap();
            store.archive_tasks(20).await.unwrap();
            assert!(
                store
                    .commit_archive(
                        call.id,
                        phone_server::store::ArchivedRecording {
                            bytes: 100000000,
                            digest: "a".repeat(64)
                        },
                        21
                    )
                    .await
                    .unwrap()
            );
            calls.push(call);
        }
        maintenance::enforce(&store, &config, 86400 * 400)
            .await
            .unwrap();
        for call in calls {
            assert_eq!(store.get(call.id).await.unwrap(), Some(call.clone()));
            assert!(store.archived_recording(call.id).await.unwrap().is_some());
        }
    }
}
