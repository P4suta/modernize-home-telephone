use anyhow::{Context as _, ensure};
use std::{
    fs::File,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

pub fn new_path(path: &Path) -> anyhow::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    ensure!(
        std::fs::symlink_metadata(&absolute)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "the output path must not already exist"
    );
    let parent = absolute
        .parent()
        .context("missing output directory")?
        .canonicalize()?;
    Ok(parent.join(absolute.file_name().context("missing output filename")?))
}

#[cfg(unix)]
fn owned(metadata: &std::fs::Metadata) -> anyhow::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    ensure!(
        metadata.uid() == rustix::process::geteuid().as_raw() && metadata.mode() & 0o077 == 0,
        "private files must belong to the current account and deny group and other access"
    );
    Ok(())
}

#[cfg(unix)]
pub fn read(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    use rustix::fs::{Mode, OFlags};
    let file: File = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into();
    let metadata = file.metadata()?;
    owned(&metadata)?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "private input is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "private input exceeds its byte limit"
    );
    Ok(bytes)
}

#[cfg(not(unix))]
pub fn read(_path: &Path, _limit: u64) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("private CLI files require Unix filesystem permissions")
}

#[cfg(unix)]
pub fn write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use rustix::fs::{Mode, OFlags};
    let mut file: File = rustix::fs::open(
        path,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?
    .into();
    file.write_all(bytes)?;
    file.sync_all()?;
    File::open(path.parent().context("missing private output directory")?)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
pub fn write(_path: &Path, _bytes: &[u8]) -> anyhow::Result<()> {
    anyhow::bail!("private CLI files require Unix filesystem permissions")
}

#[cfg(unix)]
pub fn claims(session: &Path) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt as _;
    let directory = session
        .parent()
        .context("missing session directory")?
        .join(format!(".ht813-claims-{}", uuid::Uuid::new_v4().simple()));
    std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
    Ok(directory)
}

#[cfg(not(unix))]
pub fn claims(_session: &Path) -> anyhow::Result<PathBuf> {
    anyhow::bail!("private CLI files require Unix filesystem permissions")
}

#[cfg(unix)]
pub fn claim(directory: &Path, id: uuid::Uuid) -> anyhow::Result<()> {
    use rustix::fs::{Mode, OFlags};
    let root: File = rustix::fs::open(
        directory,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into();
    owned(&root.metadata()?)?;
    let file: File = rustix::fs::openat(
        &root,
        format!("{}.claimed", id.simple()),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .context("the plan was already submitted or its claim directory is unavailable")?
    .into();
    file.sync_all()?;
    root.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
pub fn claim(_directory: &Path, _id: uuid::Uuid) -> anyhow::Result<()> {
    anyhow::bail!("private CLI files require Unix filesystem permissions")
}

pub fn secret(bytes: Vec<u8>) -> anyhow::Result<String> {
    let text =
        String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("secret input is not UTF-8"))?;
    let text = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(&text);
    ensure!(
        !text.is_empty() && !text.contains(['\r', '\n']),
        "secret input must contain one nonempty line"
    );
    Ok(text.to_owned())
}
