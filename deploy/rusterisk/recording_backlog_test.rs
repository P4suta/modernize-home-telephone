#[tokio::test]
async fn recording_finish_preserves_a_second_of_buffered_audio() -> Result<(), WorkerError> {
    let directory = tempfile::tempdir()?;
    let mut store = Store::open(directory.path())?;
    let mut recordings = RecordingSet::new(directory.path().to_owned());
    let (commands, mut worker) = mpsc::channel(64);
    let (stored, _stored) = mpsc::channel(16);
    let (wake, _wake) = mpsc::channel(1);
    let (_stop, stop) = watch::channel(false);
    let port = StoragePort {
        commands,
        stored,
        stop,
        wake,
    };
    let name = name()?;
    let (sink, admission) = tokio::join!(port.record(name, 8000), async {
        let Some(super::Command::Record {
            name,
            rate,
            policy,
            input,
            lifetime,
            reply,
        }) = worker.recv().await
        else {
            return Err(WorkerError::Invalid);
        };
        let key = recordings.begin(name, rate, policy, input, lifetime)?;
        recordings.admit(key, reply);
        Ok(())
    });
    admission?;
    let mut sink = sink?;
    for sample in 1..=50 {
        sink.try_write(Frame::new(&[sample; 160], 8000).map_err(StorageError::from)?)?;
    }
    assert_eq!(recordings.entries[&sink.key].input.max_capacity(), 50);
    recordings.pump();
    assert!(recordings.has_pending_work());
    let (finished, published) = tokio::join!(sink.finish(), async {
        let Some(super::Command::FinishRecording {
            key,
            trim_milliseconds,
            reply,
        }) = worker.recv().await
        else {
            return Err(WorkerError::Invalid);
        };
        super::respond(
            reply,
            recordings.finish(&key, &mut store, trim_milliseconds),
        );
        Ok(())
    });
    published?;
    finished?;
    assert!(!recordings.has_pending_work());
    let mut wave =
        rusterisk_storage::WaveReader::open(&directory.path().join("recordings/bounded.wav"))?;
    for sample in 1..=50 {
        assert_eq!(
            wave.read_frame()?.ok_or(WorkerError::Invalid)?.samples(),
            [sample; 160]
        );
    }
    assert!(wave.read_frame()?.is_none());
    Ok(())
}
