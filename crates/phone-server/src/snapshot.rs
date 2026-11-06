const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;

use crate::store::Store;
use anyhow::Context;
use std::path::{Path, PathBuf};

pub async fn create(store: &Store, data: &Path) -> anyhow::Result<PathBuf> {
    let _gate = store.recording_gate.write().await;
    anyhow::ensure!(!store.has_active().await?, "snapshot refuses active calls");
    let id = uuid::Uuid::new_v4();
    let base = data.join(".snapshots");
    private_directory(&base)?;
    let temporary = base.join(format!("{id}.partial"));
    let destination = base.join(id.to_string());
    private_directory(&temporary)?;
    let operation = async {
        store.backup(temporary.join("phone.sqlite")).await?;
        let source = data.join("recordings");
        let target = temporary.clone();
        tokio::task::spawn_blocking(move || copy_recordings(&source, &target)).await??;
        tokio::fs::rename(&temporary, &destination).await?;
        Ok::<_, anyhow::Error>(destination)
    }
    .await;
    if operation.is_err()
        && let Err(error) = tokio::fs::remove_dir_all(&temporary).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return operation.context(format!("snapshot cleanup failed: {error}"));
    }
    operation
}

fn private_directory(path: &Path) -> anyhow::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(PRIVATE_DIRECTORY_MODE);
    }
    builder.create(path)?;
    Ok(())
}

fn digest(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn copy_recordings(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let mut manifest = std::collections::BTreeMap::new();
    manifest.insert(
        "phone.sqlite".to_owned(),
        digest(&destination.join("phone.sqlite"))?,
    );
    let recordings = destination.join("recordings");
    private_directory(&recordings)?;
    if source.exists() {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "wav") {
                continue;
            }
            anyhow::ensure!(
                entry.file_type()?.is_file(),
                "recordings cannot contain symlinks"
            );
            let name = entry.file_name();
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid recording name"))?;
            uuid::Uuid::parse_str(stem)?;
            let target = recordings.join(&name);
            std::fs::copy(&path, &target)?;
            let file = std::fs::File::open(&target)?;
            file.sync_all()?;
            manifest.insert(
                format!("recordings/{}", name.to_string_lossy()),
                digest(&target)?,
            );
        }
    }
    let manifest = serde_json::to_vec_pretty(
        &serde_json::json!({"format":"homephone-snapshot","files":manifest}),
    )?;
    std::fs::write(destination.join("manifest.json"), manifest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["manifest.json", "phone.sqlite"] {
            std::fs::set_permissions(
                destination.join(name),
                std::fs::Permissions::from_mode(PRIVATE_FILE_MODE),
            )?;
        }
        std::fs::File::open(destination)?.sync_all()?;
    }
    Ok(())
}
