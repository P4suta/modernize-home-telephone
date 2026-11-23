const PCM_SAMPLE_RATE: u32 = 8000;
const PCM_SAMPLE_BITS: u16 = 16;
#[cfg(feature = "plugin-ai")]
const PROVIDER_REQUEST_SECONDS: u64 = 120;
const LAST_RETRY_ATTEMPT: u32 = 7;
const MAX_RECORDING_BYTES: u64 = 536870912;
const PRIVATE_FILE_MODE: u32 = 0o600;
#[cfg(feature = "plugin-ai")]
const MAX_SUMMARY_INPUT_BYTES: usize = 1048576;
const CHUNK_SAMPLES_PER_SECOND: usize = 8000;
const CHUNK_SECONDS: usize = 600;
const INFERENCE_ENDPOINT: &str = "https://api.openai.com/v1/";

use crate::{
    config::Config,
    controller::now,
    gateway::Ari,
    maintenance::remove_if_exists,
    model::{Call, Recording, Transcript},
    store::Store,
};
use anyhow::{Context, ensure};
use futures_util::StreamExt;
use reqwest::Method;
use std::{io::Cursor, path::Path};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub struct Processor {
    config: Config,
    ari: Ari,
    store: Store,
    #[cfg(feature = "plugin-ai")]
    key: Option<String>,
    #[cfg(feature = "plugin-ai")]
    client: reqwest::Client,
    api: url::Url,
    #[cfg(feature = "plugin-nas")]
    archive: Option<phone_plugin_nas::Destination>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum JobOutcome {
    Complete,
    Deferred,
}
struct PartialRecording(std::path::PathBuf);
impl Drop for PartialRecording {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                operation = "partial_recording_cleanup",
                code = "remove_failed"
            );
        }
    }
}
fn validate_recording(path: &Path) -> anyhow::Result<()> {
    use std::io::Seek;
    let reader = hound::WavReader::open(path)?;
    let specification = reader.spec();
    ensure!(
        specification.channels == 1
            && specification.sample_rate == PCM_SAMPLE_RATE
            && specification.bits_per_sample == PCM_SAMPLE_BITS
            && specification.sample_format == hound::SampleFormat::Int,
        "unsupported recording format"
    );
    let bytes = u64::from(reader.duration())
        .checked_mul(2)
        .context("recording duration overflow")?;
    let mut file = reader.into_inner();
    let expected = file
        .stream_position()?
        .checked_add(bytes)
        .context("recording offset overflow")?;
    ensure!(
        file.get_ref().metadata()?.len() >= expected,
        "truncated recording"
    );
    Ok(())
}
fn clear_job_error(call: &mut Call) {
    if matches!(
        call.last_error.as_deref(),
        Some("recording_capture_failed" | "post_call_processing_failed")
    ) {
        call.last_error = None;
    }
}
impl Processor {
    pub fn new(
        config: Config,
        ari: Ari,
        store: Store,
        key: Option<String>,
    ) -> anyhow::Result<Self> {
        #[cfg(not(feature = "plugin-ai"))]
        let _ = key;
        Ok(Self {
            config,
            ari,
            store,
            #[cfg(feature = "plugin-ai")]
            key,
            #[cfg(feature = "plugin-ai")]
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(PROVIDER_REQUEST_SECONDS))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            api: url::Url::parse(INFERENCE_ENDPOINT)?,
            #[cfg(feature = "plugin-nas")]
            archive: None,
        })
    }
    pub fn with_archive_credentials(
        mut self,
        credentials: Option<crate::secrets::NasCredentials>,
    ) -> Self {
        #[cfg(feature = "plugin-nas")]
        {
            self.archive =
                crate::recordings::archive_destination(&self.config, credentials.as_ref());
        }
        #[cfg(not(feature = "plugin-nas"))]
        {
            let _ = &mut self;
            let _ = credentials;
        }
        self
    }
    pub fn new_for_loopback(
        config: Config,
        ari: Ari,
        store: Store,
        key: Option<String>,
        address: std::net::SocketAddr,
    ) -> anyhow::Result<Self> {
        ensure!(
            address.ip().is_loopback() && address.port() != 0,
            "test provider endpoint must be loopback"
        );
        let mut processor = Self::new(config, ari, store, key)?;
        processor.api = url::Url::parse(&format!("http://{address}/v1/"))?;
        Ok(processor)
    }
    pub async fn run(self, stop: CancellationToken) {
        let inference = self.clone();
        let archive = self.clone();
        tokio::join!(
            self.capture_loop(stop.clone()),
            inference.inference_loop(stop.clone()),
            archive.archive_loop(stop),
        );
    }

    async fn archive_loop(&self, stop: CancellationToken) {
        if self
            .config
            .recording
            .as_ref()
            .and_then(|recording| recording.nas.as_ref())
            .is_none()
        {
            stop.cancelled().await;
            return;
        }
        #[cfg(feature = "plugin-nas")]
        loop {
            let work = async {
                if self.store.background_paused() {
                    return Ok(());
                }
                if self.release_archived().await.is_err() {
                    tracing::warn!(
                        operation = "recording_archive",
                        code = "cache_cleanup_failed"
                    );
                }
                for task in self.store.archive_tasks(now()).await? {
                    if self.store.background_paused() {
                        break;
                    }
                    if self.archive_one(&task).await.is_err() {
                        self.store
                            .retry_archive(task.id, task.attempts, now())
                            .await?;
                        tracing::warn!(operation = "recording_archive", code = "deferred");
                    }
                }
                Ok::<_, anyhow::Error>(())
            };
            tokio::select! {_=stop.cancelled()=>return,result=work=>{if result.is_err(){tracing::warn!(operation="recording_archive",code="queue_failed");}}}
            tokio::select! {_=stop.cancelled()=>return,_=tokio::time::sleep(std::time::Duration::from_secs(5))=>{}}
        }
        #[cfg(not(feature = "plugin-nas"))]
        stop.cancelled().await;
    }

    #[cfg(feature = "plugin-nas")]
    pub async fn archive_one(&self, task: &crate::store::ArchiveTask) -> anyhow::Result<()> {
        let _archive_guard = self.store.archive_gate.lock().await;
        let destination = self
            .archive
            .as_ref()
            .context("NAS credentials are unavailable")?;
        match task.work {
            crate::store::ArchiveWork::Delete => {
                destination.delete(task.id).await?;
                self.store.archive_deleted(task.id).await?;
            }
            crate::store::ArchiveWork::Publish { bytes } => {
                if self.store.archived_recording(task.id).await?.is_some() {
                    return Ok(());
                }
                let recording = self
                    .config
                    .recording
                    .as_ref()
                    .context("recording configuration is unavailable")?;
                let path = self
                    .config
                    .data_dir
                    .join("recordings")
                    .join(format!("{}.wav", task.id));
                let identity = phone_plugin_nas::identify(&path, recording.maximum_bytes()).await?;
                ensure!(
                    identity.bytes == bytes,
                    "recording size changed before archiving"
                );
                let readback =
                    crate::recordings::TemporaryAudio::create(&self.config.data_dir, None).await?;
                destination.ensure_directory().await?;
                destination
                    .publish(task.id, &path, readback.path(), &identity)
                    .await?;
                let _guard = self.store.recording_gate.read().await;
                if self.store.background_paused() {
                    return Ok(());
                }
                if self
                    .store
                    .commit_archive(
                        task.id,
                        crate::store::ArchivedRecording {
                            bytes,
                            digest: identity.digest(),
                        },
                        now(),
                    )
                    .await?
                {
                    remove_if_exists(&path).await?;
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "plugin-nas")]
    pub async fn release_archived(&self) -> anyhow::Result<()> {
        let _guard = self.store.recording_gate.read().await;
        if self.store.background_paused() {
            return Ok(());
        }
        let mut entries = match tokio::fs::read_dir(self.config.data_dir.join("recordings")).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let mut failures = 0_usize;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "wav") {
                continue;
            }
            if self.release_cached(&entry).await.is_err() {
                failures += 1;
                tracing::warn!(operation = "recording_cache", code = "cleanup_deferred");
            }
        }
        ensure!(failures == 0, "{failures} archived cache copies remain");
        Ok(())
    }

    #[cfg(feature = "plugin-nas")]
    async fn release_cached(&self, entry: &tokio::fs::DirEntry) -> anyhow::Result<()> {
        let path = entry.path();
        ensure!(
            entry.file_type().await?.is_file(),
            "recording cache cannot contain links"
        );
        let id = Uuid::parse_str(
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .context("invalid recording cache name")?,
        )?;
        if let Some(archived) = self.store.archived_recording(id).await? {
            let identity = phone_plugin_nas::identify(&path, archived.bytes).await?;
            ensure!(
                identity.bytes == archived.bytes && identity.digest() == archived.digest,
                "local archive copy changed before cleanup"
            );
            remove_if_exists(&path).await?;
        }
        Ok(())
    }
    async fn capture_loop(&self, stop: CancellationToken) {
        loop {
            let work = async {
                if self.store.background_paused() {
                    return Ok(());
                }
                for (id, attempts) in self.store.captures(now()).await? {
                    match self.capture(id).await {
                        Ok(JobOutcome::Complete) => {
                            self.store.finish_capture(id).await?;
                        }
                        Ok(JobOutcome::Deferred) => continue,
                        Err(_) => {
                            let exhausted = attempts >= LAST_RETRY_ATTEMPT;
                            self.store
                                .update_finished(id, "capture_failed", now(), move |call| {
                                    call.last_error = Some("recording_capture_failed".into());
                                    if exhausted {
                                        call.recording = Recording::Failed {
                                            reason: "download_failed".into(),
                                        };
                                        call.processing = "failed".into();
                                    }
                                })
                                .await?;
                            self.store.retry_capture(id, attempts, now()).await?;
                        }
                    }
                    crate::maintenance::enforce(&self.store, &self.config, now()).await?;
                }
                crate::maintenance::enforce(&self.store, &self.config, now()).await?;
                self.cleanup_recordings().await?;
                Ok::<_, anyhow::Error>(())
            };
            tokio::select! {_=stop.cancelled()=>return,result=work=>{if result.is_err(){tracing::warn!(operation="recording_capture",code="maintenance_failed");}}}
            tokio::select! {_=stop.cancelled()=>return,_=tokio::time::sleep(std::time::Duration::from_secs(2))=>{}}
        }
    }
    async fn inference_loop(&self, stop: CancellationToken) {
        loop {
            let work = async {
                if self.store.background_paused() {
                    return Ok(());
                }
                for (id, attempts) in self.store.jobs(now()).await? {
                    match self.process(id).await {
                        Ok(JobOutcome::Complete) => self.store.finish_job(id).await?,
                        Ok(JobOutcome::Deferred) => continue,
                        Err(_) => {
                            self.store
                                .update_finished(id, "processing_failed", now(), move |call| {
                                    call.last_error = Some("post_call_processing_failed".into());
                                    call.processing = if attempts >= LAST_RETRY_ATTEMPT {
                                        "failed"
                                    } else {
                                        "retrying"
                                    }
                                    .into();
                                })
                                .await?;
                            self.store.retry_job(id, attempts, now()).await?;
                        }
                    }
                }
                Ok::<_, anyhow::Error>(())
            };
            tokio::select! {_=stop.cancelled()=>return,result=work=>{if result.is_err(){tracing::warn!(operation="post_call",code="processing_failed");}}}
            tokio::select! {_=stop.cancelled()=>return,_=tokio::time::sleep(std::time::Duration::from_secs(2))=>{}}
        }
    }

    pub async fn cleanup_recordings(&self) -> anyhow::Result<()> {
        let _guard = self.store.recording_gate.read().await;
        if self.store.background_paused() {
            return Ok(());
        }
        for name in self.store.recording_cleanup(now()).await? {
            if self
                .ari
                .command(Method::DELETE, &["recordings", "stored", &name], &[])
                .await
                .is_ok()
            {
                self.store.recording_cleaned(name).await?;
            } else {
                self.store.retry_recording_cleanup(name, now()).await?;
            }
        }
        Ok(())
    }
    pub async fn capture(&self, id: Uuid) -> anyhow::Result<JobOutcome> {
        let _recording_guard = self.store.recording_gate.read().await;
        if self.store.background_paused() || self.store.owns_line(id) {
            return Ok(JobOutcome::Deferred);
        }
        let Some(call) = self.store.get(id).await? else {
            return Ok(JobOutcome::Complete);
        };
        ensure!(
            call.ended_at.is_some(),
            "post-call work cannot process an active call"
        );
        if call.processing == "complete" {
            return Ok(JobOutcome::Complete);
        }
        if matches!(call.recording, Recording::Pending) {
            let maximum = self
                .config
                .recording
                .as_ref()
                .map_or(MAX_RECORDING_BYTES, |recording| recording.maximum_bytes());
            let directory = self.config.data_dir.join("recordings");
            tokio::fs::create_dir_all(&directory).await?;
            let final_path = directory.join(format!("{id}.wav"));
            let partial = directory.join(format!("{id}.partial"));
            if tokio::fs::try_exists(&final_path).await? {
                let validate = final_path.clone();
                if tokio::task::spawn_blocking(move || validate_recording(&validate))
                    .await?
                    .is_err()
                {
                    remove_if_exists(&final_path).await?;
                }
            }
            if !tokio::fs::try_exists(&final_path).await? {
                let response = self.ari.recording(&format!("hp-{id}")).await?;
                ensure!(
                    response
                        .content_length()
                        .is_none_or(|length| length <= maximum),
                    "recording exceeds download limit"
                );
                let _partial = PartialRecording(partial.clone());
                let mut options = tokio::fs::OpenOptions::new();
                options.create(true).truncate(true).write(true);
                #[cfg(unix)]
                {
                    options.mode(PRIVATE_FILE_MODE);
                }
                let mut file = options.open(&partial).await?;
                let mut stream = response.bytes_stream();
                let mut size = 0_u64;
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    size = size
                        .checked_add(u64::try_from(chunk.len())?)
                        .context("recording size overflow")?;
                    ensure!(size <= maximum, "recording exceeds download limit");
                    file.write_all(&chunk).await?;
                }
                file.sync_all().await?;
                drop(file);
                let validate = partial.clone();
                tokio::task::spawn_blocking(move || validate_recording(&validate)).await??;
                tokio::fs::rename(&partial, &final_path).await?;
                let synced = directory.clone();
                tokio::task::spawn_blocking(move || std::fs::File::open(synced)?.sync_all())
                    .await??;
            }
            let bytes = tokio::fs::metadata(&final_path).await?.len();
            let saved = self
                .store
                .update_finished(id, "recording_ready", now(), move |call| {
                    call.recording = Recording::Available { bytes };
                    call.processing = "waiting".into();
                    clear_job_error(call);
                })
                .await?;
            if saved.is_none() {
                remove_if_exists(&final_path).await?;
                return Ok(JobOutcome::Complete);
            }
        }
        Ok(JobOutcome::Complete)
    }

    pub async fn process(&self, id: Uuid) -> anyhow::Result<JobOutcome> {
        if self.capture(id).await? == JobOutcome::Deferred {
            return Ok(JobOutcome::Deferred);
        }
        if self.store.background_paused() || self.store.owns_line(id) {
            return Ok(JobOutcome::Deferred);
        }
        let Some(call) = self.store.get(id).await? else {
            return Ok(JobOutcome::Complete);
        };
        if call.processing == "complete" {
            return Ok(JobOutcome::Complete);
        }
        if matches!(call.recording, Recording::Available { .. })
            && self.config.ai.provider != "disabled"
        {
            let transcript = if self.config.ai.provider == "mock" {
                vec![Transcript {
                    item_id: "post-call-mock".into(),
                    speaker: "mixed".into(),
                    text: "仮想回線の録音を保存しました。".into(),
                }]
            } else {
                self.transcribe(
                    id,
                    &self
                        .config
                        .data_dir
                        .join("recordings")
                        .join(format!("{id}.wav")),
                )
                .await?
            };
            if self.store.get(id).await?.is_none() {
                return Ok(JobOutcome::Complete);
            }
            let summary = if self.config.ai.provider == "mock" {
                "仮想回線による試験通話。".into()
            } else {
                self.summarize(&transcript).await?
            };
            self.store
                .update_finished(id, "complete", now(), move |call| {
                    call.transcript = transcript;
                    call.summary = Some(summary);
                    call.processing = "complete".into();
                    clear_job_error(call);
                })
                .await?;
        } else {
            self.store
                .update_finished(id, "complete", now(), |call| {
                    call.processing = "complete".into();
                    if matches!(call.recording, Recording::Available { .. }) {
                        clear_job_error(call);
                    }
                })
                .await?;
        }
        Ok(JobOutcome::Complete)
    }

    async fn transcribe(&self, id: Uuid, path: &Path) -> anyhow::Result<Vec<Transcript>> {
        #[cfg(feature = "plugin-ai")]
        {
            let key = self
                .key
                .as_ref()
                .context("missing transcription credentials")?;
            let mut transcript = vec![];
            let mut offset = 0_u64;
            let mut text_bytes = 0_usize;
            loop {
                if self.store.get(id).await?.is_none() {
                    return Ok(vec![]);
                }
                let path = path.to_owned();
                let chunk = tokio::task::spawn_blocking(move || wav_chunk(&path, offset)).await??;
                let Some((bytes, count)) = chunk else {
                    break;
                };
                let text = phone_plugin_ai::inference::transcribe(
                    &self.client,
                    &self.api,
                    key,
                    &self.config.ai.transcription_model,
                    bytes,
                )
                .await?;
                text_bytes = text_bytes
                    .checked_add(text.len())
                    .filter(|total| *total <= MAX_SUMMARY_INPUT_BYTES)
                    .context("transcript exceeded its aggregate bound")?;
                transcript.push(Transcript {
                    item_id: format!("recorded-{offset}"),
                    speaker: "mixed".into(),
                    text,
                });
                offset = offset.checked_add(count).context("audio offset overflow")?;
            }
            Ok(transcript)
        }
        #[cfg(not(feature = "plugin-ai"))]
        {
            let _ = (id, path);
            anyhow::bail!("AI plugin is not installed")
        }
    }

    async fn summarize(&self, transcript: &[Transcript]) -> anyhow::Result<String> {
        #[cfg(feature = "plugin-ai")]
        {
            let key = self.key.as_ref().context("missing summary credentials")?;
            let text = transcript
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            phone_plugin_ai::inference::summarize(
                &self.client,
                &self.api,
                key,
                &self.config.ai.summary_model,
                &text,
            )
            .await
        }
        #[cfg(not(feature = "plugin-ai"))]
        {
            let _ = transcript;
            anyhow::bail!("AI plugin is not installed")
        }
    }
}

pub fn wav_chunk(path: &Path, offset: u64) -> anyhow::Result<Option<(Vec<u8>, u64)>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    ensure!(
        spec.channels == 1
            && spec.sample_rate == PCM_SAMPLE_RATE
            && spec.bits_per_sample == PCM_SAMPLE_BITS
            && spec.sample_format == hound::SampleFormat::Int,
        "unsupported recording format"
    );
    if offset >= u64::from(reader.duration()) {
        return Ok(None);
    }
    reader.seek(u32::try_from(offset)?)?;
    let mut cursor = Cursor::new(vec![]);
    let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
    let mut count = 0_u64;
    for sample in reader
        .samples::<i16>()
        .take(CHUNK_SAMPLES_PER_SECOND * CHUNK_SECONDS)
    {
        writer.write_sample(sample?)?;
        count += 1;
    }
    writer.finalize()?;
    Ok(Some((cursor.into_inner(), count)))
}
