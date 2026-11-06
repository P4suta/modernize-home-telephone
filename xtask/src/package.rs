use crate::{
    bundle::{self, Kind},
    manager::{self, Release},
    operations::{private_directory, private_file, run},
};
use anyhow::{Context, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, process::Command};

pub fn source_files(root: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    fn walk(root: &Path, path: &Path, files: &mut BTreeMap<String, String>) -> anyhow::Result<()> {
        if path.strip_prefix(root)?
            == Path::new(".agents/skills/grandstream-ht813/references/upstream")
        {
            return Ok(());
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid source filename")?;
        if [
            "node_modules",
            "dist",
            "target",
            "test-results",
            "playwright-report",
            "reports",
            "coverage",
            ".stryker-tmp",
        ]
        .contains(&name)
        {
            return Ok(());
        }
        let kind = std::fs::symlink_metadata(path)?.file_type();
        ensure!(
            !kind.is_symlink(),
            "source inventory cannot contain symlinks"
        );
        if kind.is_dir() {
            for entry in std::fs::read_dir(path)? {
                walk(root, &entry?.path(), files)?;
            }
        } else {
            ensure!(kind.is_file(), "source inventory requires regular files");
            let name = path
                .strip_prefix(root)?
                .to_str()
                .context("invalid source path")?
                .replace('\\', "/");
            files.insert(name, bundle::digest(path)?);
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    for child in [
        "Cargo.toml",
        "Cargo.lock",
        "AGENTS.md",
        "mise.toml",
        "justfile",
        "repository.toml",
        "README.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "CODE_OF_CONDUCT.md",
        "THIRD_PARTY_NOTICES.md",
        "renovate.json",
        ".cargo",
        ".dockerignore",
        ".gitignore",
        ".gitattributes",
        ".github",
        ".agents",
        "crates",
        "api",
        "xtask",
        "web",
        "deploy",
        "verification",
        "fuzz",
        "docs",
    ] {
        let path = root.join(child);
        if path.exists() {
            walk(root, &path, &mut files)?;
        }
    }
    ensure!(!files.is_empty(), "source inventory cannot be empty");
    Ok(files)
}
pub fn source_identity(root: &Path) -> anyhow::Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&source_files(root)?)?)
    ))
}
pub fn validate(directory: &Path) -> anyhow::Result<Release> {
    bundle::verify(directory, Kind::Package)?;
    let release: Release = serde_json::from_slice(&std::fs::read(directory.join("release.json"))?)?;
    manager::compatible(&release, "arm64")?;
    for name in [
        "homephone",
        "app.tar",
        "gateway.tar",
        "gateway-source/asterisk.tar.gz",
        "source/Cargo.toml",
        "source/Cargo.lock",
        "source/LICENSE-MIT",
        "source/LICENSE-APACHE",
    ] {
        ensure!(
            std::fs::metadata(directory.join(name))?.len() > 0,
            "package asset is empty: {name}"
        );
    }
    ensure!(
        source_identity(&directory.join("source"))? == release.source,
        "package source differs from the declared release"
    );
    Ok(release)
}
pub fn build(destination: &Path) -> anyhow::Result<()> {
    ensure!(!destination.exists(), "package destination already exists");
    let source = source_identity(Path::new("."))?;
    let (app, gateway) = manager::image_references(&source, uuid::Uuid::new_v4())?;
    run(
        "docker",
        &[
            "build",
            "--platform",
            "linux/arm64",
            "-f",
            "deploy/app/Dockerfile",
            "--target",
            "runtime",
            "-t",
            &app,
            ".",
        ],
    )?;
    run(
        "docker",
        &[
            "build",
            "--platform",
            "linux/arm64",
            "-f",
            "deploy/asterisk/Dockerfile",
            "--target",
            "runtime",
            "-t",
            &gateway,
            ".",
        ],
    )?;
    let parent = destination
        .parent()
        .context("package destination requires a parent")?;
    private_directory(parent)?;
    let staging = parent.join(format!(".package-{}.partial", uuid::Uuid::new_v4()));
    private_directory(&staging)?;
    let result = (|| -> anyhow::Result<()> {
        let status = Command::new("docker")
            .args([
                "build",
                "--platform",
                "linux/arm64",
                "-f",
                "deploy/app/Dockerfile",
                "--target",
                "artifacts",
                "--output",
            ])
            .arg(format!("type=local,dest={}", staging.display()))
            .arg(".")
            .status()?;
        ensure!(status.success(), "administration artifact build failed");
        let sources = staging.join("source");
        private_directory(&sources)?;
        for name in source_files(Path::new("."))?.keys() {
            let target = sources.join(name);
            private_directory(target.parent().context("source requires a parent")?)?;
            std::fs::copy(name, target)?;
        }
        let status = Command::new("docker")
            .args([
                "build",
                "--platform",
                "linux/arm64",
                "-f",
                "deploy/asterisk/Dockerfile",
                "--target",
                "corresponding-source",
                "--output",
            ])
            .arg(format!(
                "type=local,dest={}",
                staging.join("gateway-source").display()
            ))
            .arg(".")
            .status()?;
        ensure!(status.success(), "gateway source export failed");
        for (reference, name) in [(&app, "app.tar"), (&gateway, "gateway.tar")] {
            let status = Command::new("docker")
                .args(["save", "--output"])
                .arg(staging.join(name))
                .arg(reference)
                .status()?;
            ensure!(status.success(), "image export failed");
        }
        let release = Release {
            schema: 1,
            source: source.clone(),
            architecture: "arm64".into(),
            app: manager::inspect_image(&app)?,
            gateway: manager::inspect_image(&gateway)?,
        };
        private_file(
            &staging.join("release.json"),
            &serde_json::to_vec_pretty(&release)?,
        )?;
        ensure!(
            source_identity(Path::new("."))? == source,
            "sources changed during package creation"
        );
        bundle::seal(&staging, Kind::Package)?;
        validate(&staging)?;
        std::fs::rename(&staging, destination)?;
        crate::operations::sync_parent(destination)?;
        Ok(())
    })();
    if result.is_err() {
        return crate::operations::finish_cleanup(result, std::fs::remove_dir_all(staging));
    }
    result
}
