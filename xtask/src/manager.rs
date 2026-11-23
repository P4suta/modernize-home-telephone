const SHA256_HEX_DIGITS: usize = 64;
const SOURCE_TAG_HEX_DIGITS: usize = 16;
const DEFAULT_HTTP_PORT: u16 = 8787;
const HTTP_OK: u16 = 200;
const HTTP_NO_CONTENT: u16 = 204;
const HTTP_CREATED: u16 = 201;
const HEALTH_ATTEMPTS: usize = 80;
const HEALTH_POLL_MS: u64 = 250;
const MAX_ARTIFACT_DEPTH: u32 = 32;
const MAX_ARTIFACT_FILES: u32 = 100000;
const PRIVATE_FILE_MODE: u32 = 0o600;
const DEFAULT_PUBLIC_ORIGIN: &str = "http://localhost:8787";

use crate::{
    admin_http::{Client, Operation},
    bundle::{self, Kind},
    operations::{private_directory, private_file},
};
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
};
mod rehearsal;
pub use rehearsal::{
    check_report as check_rehearsal_report, evidence as deployment_evidence, run as rehearse,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub reference: String,
    pub fingerprint: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema: u32,
    pub source: String,
    pub architecture: String,
    pub app: Image,
    pub gateway: Image,
}
pub const RUSTERISK_RELEASE_SCHEMA: u32 = 2;

pub fn image_references(source: &str, build: uuid::Uuid) -> anyhow::Result<(String, String)> {
    ensure!(
        source.len() == SHA256_HEX_DIGITS && source.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid source identity"
    );
    let tag = format!("{}-{}", &source[..SOURCE_TAG_HEX_DIGITS], build.simple());
    Ok((
        format!("homephone-app:source-{tag}"),
        format!("homephone-gateway:source-{tag}"),
    ))
}
fn digest(value: &str) -> bool {
    value.len() == SHA256_HEX_DIGITS && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub fn compatible(release: &Release, architecture: &str) -> anyhow::Result<()> {
    ensure!(
        matches!(release.schema, 1 | RUSTERISK_RELEASE_SCHEMA)
            && release.architecture == "arm64"
            && architecture == "arm64",
        "deployment requires a Linux ARM64 package"
    );
    ensure!(digest(&release.source), "invalid source identity");
    for image in [&release.app, &release.gateway] {
        ensure!(
            digest(&image.fingerprint)
                && !image.reference.is_empty()
                && !image.reference.ends_with(":latest")
                && !image.reference.starts_with('-')
                && image
                    .reference
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"./_:-@".contains(&byte)),
            "invalid image identity"
        );
    }
    Ok(())
}
pub fn verify_identity(body: &serde_json::Value, expected: uuid::Uuid) -> anyhow::Result<()> {
    ensure!(
        body.get("instance_id")
            .and_then(|id| id.as_str())
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            == Some(expected),
        "HTTP port belongs to a different installation"
    );
    Ok(())
}
fn unit_quote(path: &Path) -> anyhow::Result<String> {
    let path = path.to_str().context("systemd path must be UTF-8")?;
    ensure!(
        path.starts_with('/') && !path.chars().any(char::is_control),
        "systemd requires an absolute single-line path"
    );
    Ok(format!(
        "\"{}\"",
        path.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$")
    ))
}
pub fn systemd_unit(binary: &Path, directory: &Path, user: &str) -> anyhow::Result<String> {
    systemd_unit_for_engine(binary, directory, user, crate::container::Engine::Docker)
}
pub fn systemd_unit_for_engine(
    binary: &Path,
    directory: &Path,
    user: &str,
    engine: crate::container::Engine,
) -> anyhow::Result<String> {
    ensure!(
        !user.is_empty()
            && user
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
        "invalid systemd user"
    );
    let binary = unit_quote(binary)?;
    let directory = unit_quote(directory)?;
    if engine == crate::container::Engine::Podman {
        return Ok(format!(
            "[Unit]\nDescription=Home Telephone\nAfter=network-online.target\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nEnvironment=HOMEPHONE_CONTAINER_ENGINE=podman\nExecStart={binary} boot --directory {directory}\nExecStop={binary} stop --directory {directory}\nTimeoutStartSec=180\nTimeoutStopSec=90\nUMask=0077\n\n[Install]\nWantedBy=default.target\n"
        ));
    }
    Ok(format!(
        "[Unit]\nDescription=Home Telephone\nRequires=docker.service\nWants=network-online.target\nAfter=network-online.target docker.service\n\n[Service]\nType=oneshot\nRemainAfterExit=yes\nUser={user}\nExecStart={binary} boot --directory {directory}\nExecStop={binary} stop --directory {directory}\nTimeoutStartSec=180\nTimeoutStopSec=90\nUMask=0077\n\n[Install]\nWantedBy=multi-user.target\n"
    ))
}
fn output(command: &mut Command) -> anyhow::Result<Vec<u8>> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "external operation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}
fn inspect(reference: &str) -> anyhow::Result<serde_json::Value> {
    let [image] = serde_json::from_slice::<[_; 1]>(&output(
        crate::container::command()?.args(["image", "inspect", reference]),
    )?)?;
    Ok(image)
}
fn image_fingerprint(image: &serde_json::Value) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    ensure!(
        image["Architecture"] == "arm64" && image["Os"] == "linux",
        "image must target Linux ARM64"
    );
    let identity = serde_json::json!({"architecture":image["Architecture"],"os":image["Os"],"config":image["Config"],"rootfs":image["RootFS"]});
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&identity)?)
    ))
}
pub fn inspect_image(reference: &str) -> anyhow::Result<Image> {
    Ok(Image {
        reference: reference.into(),
        fingerprint: image_fingerprint(&inspect(reference)?)?,
    })
}
fn verified_id(expected: &Image, image: &serde_json::Value) -> anyhow::Result<String> {
    ensure!(
        image_fingerprint(image)? == expected.fingerprint,
        "loaded image differs from the package"
    );
    let id = image["Id"]
        .as_str()
        .context("gateway requires an immutable image ID")?;
    let hex = id.strip_prefix("sha256:").unwrap_or(id);
    ensure!(
        hex.len() == SHA256_HEX_DIGITS
            && hex
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
        "gateway requires an immutable image ID"
    );
    Ok(format!("sha256:{hex}"))
}
pub fn verified_image_id(expected: &Image) -> anyhow::Result<String> {
    verified_id(expected, &inspect(&expected.reference)?)
}
pub fn verify_images(release: &Release) -> anyhow::Result<()> {
    compatible(release, "arm64")?;
    for expected in [&release.app, &release.gateway] {
        verified_image_id(expected)?;
    }
    Ok(())
}
const RELEASE_TAG_PREFIXES: [&str; 2] = ["homephone-app:source-", "homephone-gateway:source-"];
const ENGINE_NAMESPACES: [&str; 2] = ["docker.io/library/", "localhost/"];
fn local_reference(reference: &str) -> &str {
    ENGINE_NAMESPACES
        .iter()
        .find_map(|namespace| reference.strip_prefix(namespace))
        .unwrap_or(reference)
}
pub fn superseded_references(
    listed: &[String],
    in_use: &[String],
    retained: &[&Release],
) -> Vec<String> {
    let kept: std::collections::BTreeSet<&str> = retained
        .iter()
        .flat_map(|release| [&release.app.reference, &release.gateway.reference])
        .chain(in_use)
        .map(|reference| local_reference(reference))
        .collect();
    listed
        .iter()
        .filter(|reference| {
            let local = local_reference(reference);
            RELEASE_TAG_PREFIXES
                .iter()
                .any(|prefix| local.starts_with(prefix))
                && !kept.contains(local)
        })
        .cloned()
        .collect()
}
fn engine_lines(arguments: &[&str]) -> anyhow::Result<Vec<String>> {
    Ok(
        String::from_utf8(output(crate::container::command()?.args(arguments))?)?
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}
fn release_images() -> anyhow::Result<Vec<String>> {
    engine_lines(&["image", "ls", "--format", "{{.Repository}}:{{.Tag}}"])
}
fn container_images() -> anyhow::Result<Vec<String>> {
    engine_lines(&["ps", "--all", "--format", "{{.Image}}"])
}
fn remove_images(references: &[String]) -> anyhow::Result<()> {
    for reference in references {
        output(crate::container::command()?.args(["image", "rm", reference]))?;
    }
    Ok(())
}
fn retire_superseded(running: &Release, previous: &Release) -> anyhow::Result<()> {
    remove_images(&superseded_references(
        &release_images()?,
        &container_images()?,
        &[running, previous],
    ))
    .context("the replacement committed, but removing superseded release images failed")
}
pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let parent = path.parent().context("state file requires a parent")?;
    private_directory(parent)?;
    let temporary = parent.join(format!(".state-{}.partial", uuid::Uuid::new_v4()));
    let operation = (|| {
        private_file(&temporary, &serde_json::to_vec_pretty(value)?)?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if operation.is_err() {
        return crate::operations::finish_cleanup(operation, std::fs::remove_file(temporary));
    }
    operation
}
#[derive(Deserialize)]
struct Configuration {
    instance_id: uuid::Uuid,
    #[serde(default = "listen")]
    listen: SocketAddr,
    data_dir: PathBuf,
    #[serde(default = "token_env")]
    api_token_env: String,
}
fn listen() -> SocketAddr {
    SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, DEFAULT_HTTP_PORT))
}
fn token_env() -> String {
    "HOMEPHONE_API_TOKEN".into()
}
pub struct Installation {
    pub directory: PathBuf,
    pub release: Release,
    pub id: uuid::Uuid,
    client: Client,
}
impl Installation {
    pub fn open(directory: &Path) -> anyhow::Result<Self> {
        let directory = directory.canonicalize()?;
        Self::read(&directory, &directory)
    }
    fn read(directory: &Path, configuration_directory: &Path) -> anyhow::Result<Self> {
        let config: Configuration = toml::from_str(&std::fs::read_to_string(
            configuration_directory.join("config/homephone.toml"),
        )?)?;
        ensure!(
            config.data_dir == directory.join("data"),
            "installation data must reside in its data directory"
        );
        ensure!(
            config.listen.ip().is_loopback(),
            "administration requires loopback HTTP"
        );
        let mut token = None;
        for line in
            std::fs::read_to_string(configuration_directory.join("config/secrets.env"))?.lines()
        {
            if let Some((key, value)) = line.split_once('=')
                && key == config.api_token_env
            {
                ensure!(token.is_none(), "duplicate administration token");
                token = Some(value.to_owned());
            }
        }
        let release = serde_json::from_slice(&std::fs::read(
            configuration_directory.join("config/images.json"),
        )?)?;
        Ok(Self {
            directory: directory.to_owned(),
            release,
            id: config.instance_id,
            client: Client {
                address: config.listen,
                token: token.context("missing administration token")?,
            },
        })
    }
    fn names(&self) -> (String, String) {
        let prefix = format!("homephone-{}", self.id.simple());
        (format!("{prefix}-app"), format!("{prefix}-gateway"))
    }
    pub fn health(&self) -> anyhow::Result<()> {
        let response = self.client.request(Operation::Status)?;
        ensure!(response.status == HTTP_OK, "service status is unavailable");
        verify_identity(
            response.body.as_ref().context("missing service status")?,
            self.id,
        )?;
        ensure!(
            self.client.request(Operation::Ready)?.status == HTTP_NO_CONTENT,
            "gateway application is unavailable"
        );
        Ok(())
    }
    fn wait_health(&self) -> anyhow::Result<()> {
        for _ in 0..HEALTH_ATTEMPTS {
            if self.health().is_ok() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(HEALTH_POLL_MS));
        }
        self.health().context("service did not become ready")
    }
    pub fn maintenance(&self, enabled: bool) -> anyhow::Result<()> {
        let marker = self.marker();
        let created = if enabled && !marker.try_exists()? {
            private_directory(
                marker
                    .parent()
                    .context("maintenance marker requires a parent")?,
            )?;
            private_file(&marker, b"quiesced\n")?;
            true
        } else {
            false
        };
        let status = self
            .client
            .request(if enabled {
                Operation::Quiesce
            } else {
                Operation::Resume
            })?
            .status;
        if status != HTTP_NO_CONTENT && created {
            std::fs::remove_file(&marker)?;
            crate::operations::sync_parent(&marker)?;
        }
        ensure!(
            status == HTTP_NO_CONTENT,
            "maintenance refused with HTTP {status}"
        );
        if !enabled && marker.try_exists()? {
            std::fs::remove_file(&marker)?;
            crate::operations::sync_parent(&marker)?;
        }
        Ok(())
    }
    fn container(&self, name: &str) -> anyhow::Result<Option<serde_json::Value>> {
        if crate::container::absent(name)? {
            return Ok(None);
        }
        let result = crate::container::command()?
            .args(["container", "inspect", name])
            .output()?;
        if !result.status.success() {
            ensure!(
                String::from_utf8_lossy(&result.stderr).contains("No such container"),
                "container inspection failed"
            );
            return Ok(None);
        }
        let values: Vec<serde_json::Value> = serde_json::from_slice(&result.stdout)?;
        let value = values
            .into_iter()
            .next()
            .context("missing container inspection")?;
        ensure!(
            value["Config"]["Labels"]["homephone.instance"] == self.id.to_string(),
            "container belongs to another installation"
        );
        Ok(Some(value))
    }
    pub fn stop_containers(&self) -> anyhow::Result<()> {
        let (app, gateway) = self.names();
        for name in [&app, &gateway] {
            if self.container(name)?.is_some() {
                output(crate::container::command()?.args(["stop", "--time", "20", name]))?;
                output(crate::container::command()?.args(["rm", name]))?;
            }
        }
        Ok(())
    }
    fn marker(&self) -> PathBuf {
        self.directory.join("run/homephone-maintenance")
    }
    fn cold_start(&self, release: &Release) -> anyhow::Result<()> {
        if let Ok(response) = self.client.request(Operation::Status) {
            ensure!(
                response.status == HTTP_OK,
                "HTTP port is occupied by an unrecognized service"
            );
            verify_identity(
                response.body.as_ref().context("missing service identity")?,
                self.id,
            )?;
        }
        verify_images(release)?;
        private_directory(&self.directory.join("run"))?;
        if !self.marker().exists() {
            private_file(&self.marker(), b"quiesced\n")?;
        }
        let (app, gateway) = self.names();
        if let Some(container) = self.container(&gateway)? {
            if container["State"]["Running"] != true {
                output(crate::container::command()?.args(["start", &gateway]))?;
            }
        } else {
            crate::operations::start_gateway(
                &self.directory,
                &release.gateway.reference,
                &gateway,
                release.schema,
            )?;
        }
        if let Some(container) = self.container(&app)? {
            if container["State"]["Running"] != true {
                output(crate::container::command()?.args(["start", &app]))?;
            }
        } else {
            let uid = String::from_utf8(output(Command::new("id").arg("-u"))?)?
                .trim()
                .to_owned();
            let gid = String::from_utf8(output(Command::new("id").arg("-g"))?)?
                .trim()
                .to_owned();
            let mut command = crate::container::command()?;
            command.args([
                "run",
                "--detach",
                "--name",
                &app,
                "--restart",
                "unless-stopped",
                "--network",
                "host",
                "--user",
                &format!("{uid}:{gid}"),
                "--label",
                &format!("homephone.instance={}", self.id),
                "--read-only",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--pids-limit",
                "256",
                "--memory",
                "768m",
                "--cpus",
                "2",
                "--tmpfs",
                "/tmp:rw,noexec,nosuid,size=32m",
            ]);
            command.args(crate::container::selected()?.run_options());
            for (child, readonly) in [("config", true), ("data", false), ("run", false)] {
                let path = self.directory.join(child);
                let path = path.to_str().context("installation path must be UTF-8")?;
                ensure!(
                    !path.contains(','),
                    "Docker bind paths cannot contain commas"
                );
                command.args([
                    "--mount",
                    &format!(
                        "type=bind,source={path},target={path}{}",
                        if readonly { ",readonly" } else { "" }
                    ),
                ]);
            }
            command.args([
                "--env",
                &format!("HOMEPHONE_QUIESCE_FILE={}", self.marker().display()),
                &release.app.reference,
                "--config",
            ]);
            command
                .arg(self.directory.join("config/homephone.toml"))
                .arg("serve");
            output(&mut command)?;
        }
        self.wait_health()
    }
    pub fn resume(&self) -> anyhow::Result<()> {
        let journal = journal_path(&self.directory)?;
        match std::fs::symlink_metadata(&journal) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file() && !metadata.file_type().is_symlink(),
                    "recovery journal must be a regular file"
                );
                std::fs::remove_file(&journal)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        crate::operations::sync_parent(&journal)?;
        self.maintenance(false)
    }
    pub fn backup(&self, destination: &Path) -> anyhow::Result<()> {
        ensure!(
            !destination.exists() && !destination.starts_with(&self.directory),
            "backup destination must be outside the installation and absent"
        );
        let response = self.client.request(Operation::Snapshot)?;
        ensure!(
            response.status == HTTP_CREATED,
            "snapshot refused with HTTP {}",
            response.status
        );
        let id = response
            .body
            .and_then(|body| {
                body.get("id")
                    .and_then(|id| id.as_str())
                    .and_then(|id| uuid::Uuid::parse_str(id).ok())
            })
            .context("missing snapshot identity")?;
        let snapshot = self.directory.join("data/.snapshots").join(id.to_string());
        let parent = destination
            .parent()
            .context("backup destination requires a parent")?;
        private_directory(parent)?;
        let staging = parent.join(format!(".backup-{}.partial", uuid::Uuid::new_v4()));
        private_directory(&staging)?;
        let mut operation = (|| -> anyhow::Result<()> {
            copy_tree(&self.directory.join("config"), &staging.join("config"))?;
            copy_tree(&snapshot, &staging.join("data"))?;
            copy_tree(&self.directory.join("spool"), &staging.join("spool"))?;
            bundle::seal(&staging, Kind::Backup)?;
            bundle::verify(&staging, Kind::Backup)?;
            std::fs::rename(&staging, destination)?;
            crate::operations::sync_parent(destination)?;
            Ok(())
        })();
        if operation.is_err() {
            operation =
                crate::operations::finish_cleanup(operation, std::fs::remove_dir_all(&staging));
        }
        crate::operations::finish_cleanup(operation, std::fs::remove_dir_all(snapshot))
    }
}
pub fn copy_tree(source: &Path, target: &Path) -> anyhow::Result<()> {
    fn copy(source: &Path, target: &Path, depth: u32, count: &mut u32) -> anyhow::Result<()> {
        ensure!(
            depth <= MAX_ARTIFACT_DEPTH,
            "artifact tree exceeds its depth bound"
        );
        let kind = std::fs::symlink_metadata(source)?.file_type();
        ensure!(!kind.is_symlink(), "artifact trees cannot contain symlinks");
        if kind.is_dir() {
            private_directory(target)?;
            for entry in std::fs::read_dir(source)? {
                let entry = entry?;
                copy(
                    &entry.path(),
                    &target.join(entry.file_name()),
                    depth + 1,
                    count,
                )?;
            }
        } else {
            ensure!(kind.is_file(), "artifact trees require regular files");
            *count = count.checked_add(1).context("artifact count overflow")?;
            ensure!(
                *count <= MAX_ARTIFACT_FILES,
                "artifact tree exceeds its file bound"
            );
            let mut input = std::fs::File::open(source)?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(PRIVATE_FILE_MODE);
            }
            let mut output = options.open(target)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            crate::operations::sync_parent(target)?;
        }
        Ok(())
    }
    copy(source, target, 0, &mut 0)
}
fn lock(directory: &Path) -> anyhow::Result<std::fs::File> {
    let parent = directory
        .parent()
        .context("installation requires a parent")?;
    private_directory(parent)?;
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .context("installation requires a UTF-8 name")?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    let file = options.open(parent.join(format!(".{name}.operation.lock")))?;
    file.try_lock()
        .context("another administration operation owns the installation")?;
    Ok(file)
}
fn journal_path(directory: &Path) -> anyhow::Result<PathBuf> {
    let parent = directory.parent().context("missing installation parent")?;
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid installation name")?;
    Ok(parent.join(format!(".{name}.operation.json")))
}
fn require_settled(directory: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(journal_path(directory)?) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => anyhow::bail!(
            "pending recovery journal; recover the interrupted operation with boot first"
        ),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    backup: PathBuf,
    previous: Release,
    committed: bool,
}
pub fn restore_backup(backup: &Path, directory: &Path) -> anyhow::Result<()> {
    bundle::verify(backup, Kind::Backup)?;
    let parent = directory.parent().context("missing restore parent")?;
    let staging = parent.join(format!(".installation-{}.partial", uuid::Uuid::new_v4()));
    bundle::restore(backup, &staging, Kind::Backup)?;
    for child in ["run", "logs", "cache", "spool/recording", "spool/tmp"] {
        private_directory(&staging.join(child))?;
    }
    let retired = parent.join(format!(".retired-{}", uuid::Uuid::new_v4()));
    if directory.exists() {
        std::fs::rename(directory, &retired)?;
    }
    if let Err(error) = std::fs::rename(&staging, directory) {
        if retired.exists()
            && let Err(recovery) = std::fs::rename(&retired, directory)
        {
            return Err(
                anyhow::Error::from(error).context(format!("restore recovery failed: {recovery}"))
            );
        }
        return Err(error.into());
    }
    crate::operations::sync_parent(directory)?;
    if retired.exists() {
        std::fs::remove_dir_all(retired)?;
    }
    Ok(())
}
pub fn boot(directory: &Path) -> anyhow::Result<()> {
    let directory = absolute(directory)?;
    let _lock = lock(&directory)?;
    let journal = journal_path(&directory)?;
    if journal.exists() {
        let state: Journal = serde_json::from_slice(&std::fs::read(&journal)?)?;
        ensure!(state.schema == 1, "unsupported recovery journal");
        if !state.committed {
            let installation = Installation::read(&directory, &state.backup)?;
            installation.stop_containers()?;
            restore_backup(&state.backup, &directory)?;
        }
        let installation = Installation::open(&directory)?;
        installation.cold_start(&installation.release)?;
        installation.resume()?;
        return Ok(());
    }
    let installation = Installation::open(&directory)?;
    if installation.health().is_ok() {
        return installation.resume();
    }
    installation.cold_start(&installation.release)?;
    installation.resume()
}
pub fn absolute(path: &Path) -> anyhow::Result<PathBuf> {
    ensure!(
        !path
            .components()
            .any(|component| component == std::path::Component::ParentDir),
        "administrative paths cannot contain parent traversal"
    );
    let mut existing = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut tail = Vec::new();
    loop {
        match std::fs::symlink_metadata(&existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tail.push(
                    existing
                        .file_name()
                        .context("path has no existing ancestor")?
                        .to_owned(),
                );
                ensure!(existing.pop(), "path has no existing ancestor");
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut resolved = existing.canonicalize()?;
    for component in tail.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}
pub fn stop(directory: &Path) -> anyhow::Result<()> {
    let directory = absolute(directory)?;
    let _lock = lock(&directory)?;
    let installation = Installation::open(&directory)?;
    installation.maintenance(true)?;
    installation.stop_containers()
}
pub fn backup(directory: &Path, destination: &Path) -> anyhow::Result<()> {
    let directory = absolute(directory)?;
    let _lock = lock(&directory)?;
    require_settled(&directory)?;
    let installation = Installation::open(&directory)?;
    installation.maintenance(true)?;
    let result = installation.backup(&absolute(destination)?);
    let resumed = installation.resume();
    crate::update_flow::with_recovery(result, resumed)
}
struct Replacement {
    installation: Installation,
    candidate: Release,
    backup: PathBuf,
    restore_candidate: Option<PathBuf>,
}
impl crate::update_flow::Port for Replacement {
    fn quiesce(&mut self) -> anyhow::Result<()> {
        self.installation.maintenance(true)
    }
    fn backup(&mut self) -> anyhow::Result<()> {
        self.installation.backup(&self.backup)?;
        atomic_json(
            &journal_path(&self.installation.directory)?,
            &Journal {
                schema: 1,
                backup: self.backup.clone(),
                previous: self.installation.release.clone(),
                committed: false,
            },
        )
    }
    fn stop(&mut self) -> anyhow::Result<()> {
        self.installation.stop_containers()
    }
    fn candidate(&mut self) -> anyhow::Result<()> {
        if let Some(backup) = &self.restore_candidate {
            restore_backup(backup, &self.installation.directory)?;
        }
        self.installation.cold_start(&self.candidate)
    }
    fn health(&mut self) -> anyhow::Result<()> {
        self.installation.wait_health()
    }
    fn commit(&mut self) -> anyhow::Result<()> {
        atomic_json(
            &self.installation.directory.join("config/images.json"),
            &self.candidate,
        )?;
        atomic_json(
            &journal_path(&self.installation.directory)?,
            &Journal {
                schema: 1,
                backup: self.backup.clone(),
                previous: self.installation.release.clone(),
                committed: true,
            },
        )
    }
    fn restore(&mut self) -> anyhow::Result<()> {
        restore_backup(&self.backup, &self.installation.directory)
    }
    fn previous(&mut self) -> anyhow::Result<()> {
        self.installation.cold_start(&self.installation.release)
    }
    fn resume(&mut self) -> anyhow::Result<()> {
        self.installation.resume()
    }
}
pub fn update(directory: &Path, package: &Path, backup: &Path) -> anyhow::Result<()> {
    let candidate = load_package(package)?;
    replace(
        directory,
        candidate,
        backup,
        None,
        Retention::RunningAndPrevious,
    )
}
pub fn rollback(directory: &Path, snapshot: &Path, backup: &Path) -> anyhow::Result<()> {
    bundle::verify(snapshot, Kind::Backup)?;
    let candidate: Release =
        serde_json::from_slice(&std::fs::read(snapshot.join("config/images.json"))?)?;
    verify_images(&candidate)?;
    replace(
        directory,
        candidate,
        backup,
        Some(snapshot.to_owned()),
        Retention::RunningAndPrevious,
    )
}
#[derive(Clone, Copy)]
enum Retention {
    RunningAndPrevious,
    AllReleases,
}
fn replace(
    directory: &Path,
    candidate: Release,
    backup: &Path,
    restore_candidate: Option<PathBuf>,
    retention: Retention,
) -> anyhow::Result<()> {
    let directory = absolute(directory)?;
    let _lock = lock(&directory)?;
    require_settled(&directory)?;
    let installation = Installation::open(&directory)?;
    let previous = installation.release.clone();
    let running = candidate.clone();
    crate::update_flow::replace(&mut Replacement {
        installation,
        candidate,
        backup: absolute(backup)?,
        restore_candidate,
    })?;
    match retention {
        Retention::RunningAndPrevious => retire_superseded(&running, &previous),
        Retention::AllReleases => Ok(()),
    }
}
pub fn load_package(directory: &Path) -> anyhow::Result<Release> {
    let release = crate::package::validate(directory)?;
    for archive in ["app.tar", "gateway.tar"] {
        output(
            crate::container::command()?
                .args(["load", "--input"])
                .arg(directory.join(archive)),
        )?;
    }
    verify_images(&release)?;
    Ok(release)
}
pub fn install(package: &Path, directory: &Path) -> anyhow::Result<()> {
    let directory = absolute(directory)?;
    let _lock = lock(&directory)?;
    require_settled(&directory)?;
    ensure!(
        !directory.join("config/images.json").exists(),
        "installation already has images; use update"
    );
    let release = load_package(package)?;
    atomic_json(&directory.join("config/images.json"), &release)
}

pub fn lab(directory: &Path) -> anyhow::Result<()> {
    if !directory.exists() {
        crate::operations::initialize(
            directory,
            std::net::Ipv4Addr::LOCALHOST.into(),
            DEFAULT_PUBLIC_ORIGIN.into(),
            true,
        )?;
    }
    require_lab(directory)?;
    let source = crate::package::source_identity(Path::new("."))?;
    let (app, gateway) = image_references(&source, uuid::Uuid::new_v4())?;
    crate::operations::run(
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
    crate::operations::run(
        "docker",
        &[
            "build",
            "--platform",
            "linux/arm64",
            "-f",
            "deploy/rusterisk/Dockerfile",
            "--target",
            "runtime",
            "-t",
            &gateway,
            ".",
        ],
    )?;
    ensure!(
        crate::package::source_identity(Path::new("."))? == source,
        "sources changed during lab image creation"
    );
    let candidate = Release {
        schema: RUSTERISK_RELEASE_SCHEMA,
        source,
        architecture: "arm64".into(),
        app: inspect_image(&app)?,
        gateway: inspect_image(&gateway)?,
    };
    let metadata = directory.join("config/images.json");
    if !metadata.exists() {
        atomic_json(&metadata, &candidate)?;
        return boot(directory);
    }
    let installation = Installation::open(directory)?;
    if installation.release.app.fingerprint == candidate.app.fingerprint
        && installation.release.gateway.fingerprint == candidate.gateway.fingerprint
    {
        atomic_json(&metadata, &candidate)?;
        return boot(directory);
    }
    let backup = directory
        .parent()
        .context("lab requires a parent")?
        .join("backups")
        .join(uuid::Uuid::new_v4().to_string());
    replace(directory, candidate, &backup, None, Retention::AllReleases)
}
pub fn require_lab(directory: &Path) -> anyhow::Result<()> {
    let config: toml::Value = toml::from_str(&std::fs::read_to_string(
        directory.join("config/homephone.toml"),
    )?)?;
    ensure!(
        config.get("mode").and_then(toml::Value::as_str) == Some("lab")
            && config
                .get("ai")
                .and_then(|value| value.get("provider"))
                .and_then(toml::Value::as_str)
                == Some("mock"),
        "automation requires an explicitly configured mock lab"
    );
    Ok(())
}
pub fn doctor(directory: &Path) -> anyhow::Result<()> {
    let installation = Installation::open(directory)?;
    let images = verify_images(&installation.release).is_ok();
    let healthy = installation.health().is_ok();
    let (app, gateway) = installation.names();
    let containers =
        installation.container(&app)?.is_some() && installation.container(&gateway)?.is_some();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"instance_id":installation.id,"healthy":healthy,"images_verified":images,"containers_present":containers,"interrupted_update":journal_path(&installation.directory)?.exists(),"hardware_acceptance":"pending"})
        )?
    );
    ensure!(
        images && healthy && containers,
        "installation diagnostics found an unavailable component"
    );
    Ok(())
}
pub fn restore(backup: &Path, directory: &Path) -> anyhow::Result<()> {
    let directory = absolute(directory)?;
    let _lock = lock(&directory)?;
    ensure!(
        !directory.exists(),
        "restore requires an absent destination"
    );
    restore_backup(backup, &directory)?;
    let path = directory.join("config/homephone.toml");
    let mut configuration: toml::Value = toml::from_str(&std::fs::read_to_string(&path)?)?;
    configuration["data_dir"] = toml::Value::String(
        directory
            .join("data")
            .to_str()
            .context("restore path must be UTF-8")?
            .into(),
    );
    configuration["instance_id"] = toml::Value::String(uuid::Uuid::new_v4().to_string());
    let temporary = directory.join("config/.homephone.toml.partial");
    private_file(
        &temporary,
        toml::to_string_pretty(&configuration)?.as_bytes(),
    )?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
#[test]
fn gateway_fixture_starts_only_the_inspected_release_image() {
    let id = format!("sha256:{}", "a".repeat(SHA256_HEX_DIGITS));
    let mut image = serde_json::json!({"Id":id, "Architecture":"arm64", "Os":"linux", "Config":{"Cmd":["gateway"]}, "RootFS":{"Layers":["original"]}});
    let expected = Image {
        reference: "gateway:installed".into(),
        fingerprint: image_fingerprint(&image).unwrap(),
    };
    assert_eq!(verified_id(&expected, &image).unwrap(), id);
    image["Id"] = "gateway:installed".into();
    assert!(verified_id(&expected, &image).is_err());
    image["Id"] = "a".repeat(SHA256_HEX_DIGITS).into();
    assert_eq!(verified_id(&expected, &image).unwrap(), id);
    for refused in [
        "a".repeat(SHA256_HEX_DIGITS - 1),
        "A".repeat(SHA256_HEX_DIGITS),
        format!("sha512:{}", "a".repeat(SHA256_HEX_DIGITS)),
    ] {
        image["Id"] = refused.into();
        assert!(verified_id(&expected, &image).is_err());
    }
    image["Id"] = id.into();
    image["RootFS"]["Layers"][0] = "replacement".into();
    assert!(verified_id(&expected, &image).is_err());
}

#[cfg(test)]
#[test]
fn podman_inspection_reproduces_the_fingerprint_recorded_from_docker() {
    let podman = serde_json::json!({
        "Id": "15f496c7123856f1f7092c8fc18826abb429c76c26bdc4d917e0a85e3b7e87ed",
        "Architecture": "arm64",
        "Os": "linux",
        "Config": {
            "User": "10001:10001",
            "Env": ["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],
            "Entrypoint": ["/usr/local/bin/phone-server"],
            "WorkingDir": "/opt/homephone"
        },
        "RootFS": {"Type": "layers", "Layers": [
            "sha256:04e61ff9ed2c84e88795bf7562d69d0980188e2bb6d6152d6fe721383e10db9e",
            "sha256:4a148950ff72db529d512efc5ba2564637666887cab5b47b3efb82532844e62a",
            "sha256:f7e50594272bf245b65dd88a3e63e4533e507794f42827bdba0d091941b194c7",
            "sha256:6ff2b57e53017eb072f778a69f472eb5096c0db0600caa79419b7e4f9781a4fe",
            "sha256:fac08dbb3ef5cdbc7fd35f648df4bed73be6680b1e41d4a15adce2cd6b69b463",
            "sha256:b7e1d69edd3433cd9b096b318b32733c55318143e4acbb1e50a431ad1ab7c145",
            "sha256:af6ed2d6d4b63e27d0b6e78aac258f264da5992af6951445da9d76d395ced81e",
            "sha256:6d9b984ee0aa7a3ca8270f229930bc2bdf539a62281090d76d0013efa496d274",
            "sha256:214f086335fc4da5d128cc23b17027ea8d8decb9f187ae59cc6e81dbd4667489",
            "sha256:f116cccc1d8c09882eb0b2e60ca34e49969ef788a76e125ea385974b0d877c22"
        ]}
    });
    let recorded_with_docker = Image {
        reference: "homephone-app:installed".into(),
        fingerprint: "9ae83d587c833a3a9c0738f85a341c060c5474dd6e34cd1582c6c84ad6074548".into(),
    };
    assert_eq!(
        verified_id(&recorded_with_docker, &podman).unwrap(),
        "sha256:15f496c7123856f1f7092c8fc18826abb429c76c26bdc4d917e0a85e3b7e87ed"
    );
}
