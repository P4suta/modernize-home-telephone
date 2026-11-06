use phone_server::{
    config::Config,
    maintenance,
    model::{Call, Phase, Recording, Settings},
    store::Store,
};

#[tokio::test]
async fn quota_accounts_for_recordings_beyond_the_history_page_and_expiry_never_orphans_a_later_batch()
 {
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
            recording_limit_bytes: Some(1048576),
            ..Settings::default()
        })
        .await
        .unwrap();
    let mut template = Call::offered("123".into(), "inbound".into(), 100);
    template.phase = Phase::Ended;
    template.ended_at = Some(101);
    template.recording = Recording::Deleted {
        reason: "quota".into(),
    };
    let payload = serde_json::to_string(&template).unwrap();
    store
        .with(move |connection| {
            let transaction = connection.transaction()?;
            for _ in 0..10001 {
                let id = uuid::Uuid::new_v4().to_string();
                transaction.execute(
                    "INSERT INTO calls(id,phase,started_at,ended_at,payload) VALUES(?1,'ended',100,101,json_set(?2,'$.id',?1))",
                    rusqlite::params![id, payload],
                )?;
            }
            transaction.commit()?;
            Ok(())
        })
        .await
        .unwrap();
    let recordings = directory.path().join("recordings");
    tokio::fs::create_dir_all(&recordings).await.unwrap();
    let mut ids = vec![];
    for time in 200..405 {
        let mut call = Call::offered("123".into(), "inbound".into(), time);
        call.phase = Phase::Ended;
        call.ended_at = Some(time + 1);
        call.recording = Recording::Available { bytes: 700000 };
        store.save(&call, "ended", time + 1).await.unwrap();
        tokio::fs::write(recordings.join(format!("{}.wav", call.id)), [1, 2, 3])
            .await
            .unwrap();
        ids.push(call.id);
    }
    maintenance::enforce(&store, &config, 1000).await.unwrap();
    assert_eq!(store.recording_usage().await.unwrap(), 3500000);
    maintenance::enforce(&store, &config, 1000).await.unwrap();
    assert_eq!(store.recording_usage().await.unwrap(), 700000);
    store
        .with(|connection| {
            connection.execute("DELETE FROM calls WHERE ended_at=101", [])?;
            Ok(())
        })
        .await
        .unwrap();
    let expired = 86400 * 31;
    for _ in 0..2 {
        maintenance::enforce(&store, &config, expired)
            .await
            .unwrap();
        for id in &ids {
            if store.get(*id).await.unwrap().is_none() {
                assert!(!recordings.join(format!("{id}.wav")).is_file());
            }
        }
    }
    maintenance::enforce(&store, &config, expired)
        .await
        .unwrap();
    assert!(
        tokio::fs::read_dir(recordings)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn recording_quota_evicts_old_finished_audio_and_preserves_the_active_call() {
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
            recording_limit_bytes: Some(1048576),
            ..Settings::default()
        })
        .await
        .unwrap();
    tokio::fs::create_dir_all(directory.path().join("recordings"))
        .await
        .unwrap();
    let mut finished = vec![];
    for time in [10, 20] {
        let mut call = Call::offered("123".into(), "inbound".into(), time);
        call.phase = Phase::Ended;
        call.ended_at = Some(time + 1);
        call.recording = Recording::Available { bytes: 700000 };
        store.save(&call, "ended", time + 1).await.unwrap();
        tokio::fs::write(
            directory
                .path()
                .join("recordings")
                .join(format!("{}.wav", call.id)),
            [1, 2, 3],
        )
        .await
        .unwrap();
        finished.push(call);
    }
    let active = Call::offered("456".into(), "inbound".into(), 30);
    store.save(&active, "incoming", 30).await.unwrap();
    let active_path = directory
        .path()
        .join("recordings")
        .join(format!("{}.wav", active.id));
    tokio::fs::write(&active_path, [1, 2, 3]).await.unwrap();
    maintenance::enforce(&store, &config, 100).await.unwrap();
    assert!(
        matches!(store.get(finished[0].id).await.unwrap().unwrap().recording, Recording::Deleted { reason } if reason == "quota")
    );
    assert!(matches!(
        store.get(finished[1].id).await.unwrap().unwrap().recording,
        Recording::Available { .. }
    ));
    assert!(active_path.exists());
    maintenance::enforce(&store, &config, 86400 * 31)
        .await
        .unwrap();
    assert!(store.get(finished[1].id).await.unwrap().is_none());
    assert!(active_path.exists());
}

#[test]
fn wav_chunks_preserve_samples_and_refuse_unsupported_audio() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recording.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 8000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for sample in [-300_i16, 0, 400, 800] {
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
    let (bytes, count) = phone_server::postcall::wav_chunk(&path, 1)
        .unwrap()
        .unwrap();
    assert_eq!(count, 3);
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(
        reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        [0, 400, 800]
    );
    assert!(
        phone_server::postcall::wav_chunk(&path, 4)
            .unwrap()
            .is_none()
    );
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 2,
            ..spec
        },
    )
    .unwrap();
    writer.write_sample(0_i16).unwrap();
    writer.write_sample(0_i16).unwrap();
    writer.finalize().unwrap();
    assert!(phone_server::postcall::wav_chunk(&path, 0).is_err());
}
