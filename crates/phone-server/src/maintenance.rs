const SECONDS_PER_DAY: u64 = 86400;

use crate::{config::Config, model::Recording, store::Store};
use phone_core::{Purge, Recording as RecordingState, recording_policy};
use std::path::Path;

pub async fn enforce(store: &Store, config: &Config, now: u64) -> anyhow::Result<()> {
    let _recording_guard = store.recording_gate.read().await;
    if store.background_paused() {
        return Ok(());
    }
    let settings = store.settings().await?;
    let retention = u64::from(
        settings
            .retention_days
            .unwrap_or(config.storage.retention_days),
    ) * SECONDS_PER_DAY;
    let limit = settings
        .recording_limit_bytes
        .unwrap_or(config.storage.recording_limit_bytes);
    let calls = store
        .purge_candidates(now.saturating_sub(retention))
        .await?;
    let directory = config.data_dir.join("recordings");
    let mut used = store.recording_usage().await?;
    for call in calls {
        if store.owns_line(call.id) {
            continue;
        }
        let Some(ended_at) = call.ended_at else {
            continue;
        };
        let decision = recording_policy(
            RecordingState::Closed { ended_at },
            now,
            retention,
            used,
            limit,
        );
        if decision == Purge::Keep {
            continue;
        }
        remove_if_exists(&directory.join(format!("{}.wav", call.id))).await?;
        remove_if_exists(&directory.join(format!("{}.partial", call.id))).await?;
        if let Recording::Available { bytes } = call.recording {
            used = used.saturating_sub(bytes);
        }
        if decision == Purge::Expired {
            store.delete_finished(call.id, now).await?;
        } else if !matches!(call.recording, Recording::Deleted { .. }) {
            store
                .update_finished(call.id, "recording_deleted", now, |call| {
                    call.recording = Recording::Deleted {
                        reason: "quota".into(),
                    };
                })
                .await?;
        }
    }
    store.prune_history(now.saturating_sub(retention)).await?;
    Ok(())
}

pub async fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
