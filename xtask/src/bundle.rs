const MAX_BUNDLE_DEPTH: usize = 32;
const MAX_BUNDLE_FILES: usize = 100000;
const MAX_MANIFEST_BYTES: u64 = 16777216;
const PRIVATE_FILE_MODE: u32 = 0o600;

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Kind {
    #[serde(rename = "homephone-backup")]
    Backup,
    #[serde(rename = "homephone-package")]
    Package,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub format: Kind,
    pub files: BTreeMap<String, String>,
}
pub fn digest(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}
fn path(root: &Path, name: &str) -> anyhow::Result<std::path::PathBuf> {
    anyhow::ensure!(
        phone_core::artifact::RelativePath::parse(name.as_bytes()).is_some(),
        "invalid bundle path"
    );
    let mut path = root.to_owned();
    for component in std::path::Path::new(name).components() {
        path.push(component);
        anyhow::ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "bundle paths cannot contain symlinks"
        );
    }
    Ok(path)
}
fn collect(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        directory.strip_prefix(root)?.components().count() <= MAX_BUNDLE_DEPTH,
        "bundle directory depth exceeds its bound"
    );
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file = entry.path();
        let kind = entry.file_type()?;
        anyhow::ensure!(!kind.is_symlink(), "bundles cannot contain symlinks");
        if kind.is_dir() {
            collect(root, &file, files)?;
        } else {
            anyhow::ensure!(kind.is_file(), "bundles require regular files");
            let relative = file
                .strip_prefix(root)?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("non-UTF8 bundle path"))?
                .replace('\\', "/");
            if relative == "manifest.json" {
                continue;
            }
            path(root, &relative)?;
            files.insert(relative, digest(&file)?);
            anyhow::ensure!(
                files.len() <= MAX_BUNDLE_FILES,
                "bundle inventory exceeds its bound"
            );
        }
    }
    Ok(())
}
pub fn seal(root: &Path, kind: Kind) -> anyhow::Result<()> {
    anyhow::ensure!(
        !std::fs::symlink_metadata(root)?.file_type().is_symlink(),
        "bundle root cannot be a symlink"
    );
    let mut files = BTreeMap::new();
    collect(root, root, &mut files)?;
    anyhow::ensure!(!files.is_empty(), "bundle inventory cannot be empty");
    let manifest = Manifest {
        schema: 1,
        format: kind,
        files,
    };
    crate::operations::private_file(
        &root.join("manifest.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )
}
pub fn verify(root: &Path, kind: Kind) -> anyhow::Result<Manifest> {
    anyhow::ensure!(
        !std::fs::symlink_metadata(root)?.file_type().is_symlink(),
        "bundle root cannot be a symlink"
    );
    let manifest_path = path(root, "manifest.json")?;
    anyhow::ensure!(
        std::fs::metadata(&manifest_path)?.len() <= MAX_MANIFEST_BYTES,
        "bundle manifest exceeds its bound"
    );
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(manifest_path)?)?;
    anyhow::ensure!(
        manifest.schema == 1 && manifest.format == kind && !manifest.files.is_empty(),
        "unsupported bundle contract"
    );
    let mut actual = BTreeMap::new();
    collect(root, root, &mut actual)?;
    for name in manifest.files.keys() {
        path(root, name)?;
    }
    anyhow::ensure!(
        actual == manifest.files,
        "bundle inventory or checksums differ"
    );
    Ok(manifest)
}
pub fn restore(root: &Path, destination: &Path, kind: Kind) -> anyhow::Result<()> {
    let manifest = verify(root, kind)?;
    anyhow::ensure!(
        std::fs::symlink_metadata(destination)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "restore destination already exists"
    );
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("restore destination requires a parent"))?;
    let staging = parent.join(format!(".restore-{}.partial", uuid::Uuid::new_v4()));
    crate::operations::private_directory(&staging)?;
    let operation = (|| -> anyhow::Result<()> {
        for name in manifest.files.keys() {
            let source = path(root, name)?;
            let target = staging.join(name);
            if let Some(parent) = target.parent() {
                crate::operations::private_directory(parent)?;
            }
            std::fs::copy(source, &target)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    &target,
                    std::fs::Permissions::from_mode(PRIVATE_FILE_MODE),
                )?;
            }
            std::fs::File::open(target)?.sync_all()?;
            crate::operations::sync_parent(&staging.join(name))?;
        }
        seal(&staging, kind)?;
        verify(&staging, kind)?;
        std::fs::rename(&staging, destination)?;
        crate::operations::sync_parent(destination)?;
        Ok(())
    })();
    if operation.is_err() {
        return crate::operations::finish_cleanup(operation, std::fs::remove_dir_all(staging));
    }
    operation
}
