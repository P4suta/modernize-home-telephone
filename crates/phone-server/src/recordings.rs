use crate::{config::Config, store::Store};
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

#[cfg(feature = "plugin-nas")]
pub async fn probe(
    config: &Config,
    credentials: Option<&crate::secrets::NasCredentials>,
) -> anyhow::Result<()> {
    let destination =
        archive_destination(config, credentials).context("NAS recording storage is unavailable")?;
    let id = uuid::Uuid::from_u128(0x00000000_0000_8000_8000_000000000001);
    let source = TemporaryAudio::create(&config.data_dir, None).await?;
    let readback = TemporaryAudio::create(&config.data_dir, None).await?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(
        &mut bytes,
        hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    writer.write_sample(0_i16)?;
    writer.finalize()?;
    tokio::fs::write(source.path(), bytes.into_inner()).await?;
    let result = async {
        destination.ensure_directory().await?;
        destination.delete(id).await?;
        let identity = phone_plugin_nas::identify(source.path(), 4096).await?;
        destination
            .publish(id, source.path(), readback.path(), &identity)
            .await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    destination
        .delete(id)
        .await
        .context("remove NAS storage probe")?;
    result
}

#[cfg(feature = "plugin-nas")]
pub fn archive_destination(
    config: &Config,
    credentials: Option<&crate::secrets::NasCredentials>,
) -> Option<phone_plugin_nas::Destination> {
    let recording = config.recording.as_ref()?;
    let nas = recording.nas.as_ref()?;
    let credentials = credentials?;
    phone_plugin_nas::Destination::new(
        nas.server.clone(),
        nas.share.clone(),
        format!("{}/{}", nas.directory, nas.archive_id),
        nas.port,
        credentials.username.clone(),
        credentials.password.clone(),
        recording.maximum_bytes(),
    )
    .ok()
}

pub async fn cleanup_temporary(data: &Path) -> anyhow::Result<()> {
    let mut entries = match tokio::fs::read_dir(data.join("recording-cache")).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        ensure!(
            entry.file_type().await?.is_file(),
            "recording temporary storage contains an unsupported entry"
        );
        ensure!(
            path.extension()
                .is_some_and(|extension| extension == "partial"),
            "unknown recording temporary file"
        );
        uuid::Uuid::parse_str(
            path.file_stem()
                .and_then(|name| name.to_str())
                .context("invalid recording temporary name")?,
        )?;
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}

pub async fn capacity(config: &Config, store: &Store) -> anyhow::Result<bool> {
    let Some(recording) = &config.recording else {
        return Ok(true);
    };
    let pending = store.pending_recordings().await?;
    let limit = store
        .settings()
        .await?
        .recording_limit_bytes
        .unwrap_or(config.storage.recording_limit_bytes);
    let directories = vec![
        config.data_dir.join("recordings"),
        config.data_dir.join("recording-cache"),
        recording.gateway_spool_dir.clone(),
    ];
    let disk = config.data_dir.clone();
    let (used, free) = tokio::task::spawn_blocking(move || {
        let mut entries = 0;
        let mut used = 0_u64;
        for directory in directories {
            used = used
                .checked_add(directory_bytes(&directory, 0, &mut entries)?)
                .context("recording size overflow")?;
        }
        #[cfg(unix)]
        let free = {
            let stat = rustix::fs::statvfs(disk)?;
            stat.f_bavail
                .checked_mul(stat.f_frsize)
                .context("recording free space overflow")?
        };
        #[cfg(not(unix))]
        let free = {
            let _ = disk;
            anyhow::bail!("recording capacity requires filesystem statistics")
        };
        Ok::<_, anyhow::Error>((used, free))
    })
    .await??;
    Ok(phone_core::recording_capacity(
        used,
        pending,
        recording.maximum_bytes(),
        limit,
        free,
    ))
}

fn directory_bytes(path: &Path, depth: u8, entries: &mut usize) -> anyhow::Result<u64> {
    let directory = match std::fs::read_dir(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let mut used = 0_u64;
    for entry in directory {
        let entry = entry?;
        *entries = entries
            .checked_add(1)
            .context("recording entry count overflow")?;
        ensure!(
            *entries <= 4096,
            "recording spool contains too many entries"
        );
        let kind = entry.file_type()?;
        let bytes = if kind.is_file() {
            entry.metadata()?.len()
        } else if kind.is_dir() && depth < 2 {
            directory_bytes(&entry.path(), depth + 1, entries)?
        } else {
            anyhow::bail!("recording spool contains an unsupported entry")
        };
        used = used
            .checked_add(bytes)
            .context("recording spool size overflow")?;
    }
    Ok(used)
}

pub struct TemporaryAudio {
    path: PathBuf,
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

pub struct PlaybackAudio {
    call: uuid::Uuid,
    bytes: u64,
    digest: String,
    audio: TemporaryAudio,
}

impl PlaybackAudio {
    pub fn matches(&self, id: uuid::Uuid, bytes: u64, digest: &str) -> bool {
        self.call == id && self.bytes == bytes && self.digest == digest
    }
    pub fn path(&self) -> &Path {
        self.audio.path()
    }
}

#[cfg(feature = "plugin-nas")]
pub async fn playback(
    data: &Path,
    store: &Store,
    destination: &phone_plugin_nas::Destination,
    id: uuid::Uuid,
    receipt: &crate::store::ArchivedRecording,
) -> anyhow::Result<std::sync::Arc<PlaybackAudio>> {
    let mut cache = store.playback_cache.lock().await;
    if let Some(audio) = cache.upgrade()
        && audio.matches(id, receipt.bytes, &receipt.digest)
    {
        return Ok(audio);
    }
    let permit = store
        .playback_gate
        .clone()
        .try_acquire_owned()
        .context("another recording is loading")?;
    let audio = TemporaryAudio::create(data, Some(permit)).await?;
    let identity = phone_plugin_nas::Identity::parse(receipt.bytes, &receipt.digest)?;
    destination.fetch(id, audio.path(), &identity).await?;
    let audio = std::sync::Arc::new(PlaybackAudio {
        call: id,
        bytes: receipt.bytes,
        digest: receipt.digest.clone(),
        audio,
    });
    *cache = std::sync::Arc::downgrade(&audio);
    Ok(audio)
}

impl TemporaryAudio {
    pub async fn create(
        data: &Path,
        permit: Option<tokio::sync::OwnedSemaphorePermit>,
    ) -> anyhow::Result<Self> {
        let directory = data.join("recording-cache");
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join(format!("{}.partial", uuid::Uuid::new_v4()));
        let mut options = tokio::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        options.open(&path).await?;
        Ok(Self {
            path,
            _permit: permit,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryAudio {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                operation = "recording_temporary_cleanup",
                code = "remove_failed"
            );
        }
    }
}
