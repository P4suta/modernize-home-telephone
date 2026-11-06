const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;
const GATEWAY_CONFIG_DIRECTORY: &str = "/etc/asterisk";
const GATEWAY_SPOOL_DIRECTORY: &str = "/var/spool/asterisk";
const GATEWAY_RUNTIME_DIRECTORY: &str = "/var/run/asterisk";
const GATEWAY_LOG_DIRECTORY: &str = "/var/log/asterisk";
const GATEWAY_CACHE_DIRECTORY: &str = "/var/cache/asterisk";

use anyhow::{Context, ensure};

pub fn finish_cleanup<T>(
    result: anyhow::Result<T>,
    cleanup: std::io::Result<()>,
) -> anyhow::Result<T> {
    match cleanup {
        Ok(()) => result,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => result,
        Err(error) => match result {
            Ok(_) => Err(error.into()),
            Err(primary) => Err(primary.context(format!("cleanup failed: {error}"))),
        },
    }
}
use serde::Serialize;
use std::{
    io::Write,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

pub fn run(program: &str, arguments: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(program)
        .args(arguments)
        .status()
        .with_context(|| format!("start {program}"))?;
    ensure!(status.success(), "{program} failed with {status}");
    Ok(())
}

pub fn private_directory(path: &Path) -> anyhow::Result<()> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "private artifact directory cannot be a symlink or file"
        );
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        private_directory(parent)?;
    }
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(PRIVATE_DIRECTORY_MODE);
    }
    builder.create(path)?;
    std::fs::File::open(path)?.sync_all()?;
    sync_parent(path)?;
    Ok(())
}
pub fn sync_parent(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn private_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(PRIVATE_FILE_MODE);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    sync_parent(path)?;
    Ok(())
}

fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[derive(Serialize)]
struct BootConfiguration {
    instance_id: Uuid,
    mode: &'static str,
    data_dir: PathBuf,
    public_origin: String,
    allowed_users: Vec<String>,
    ai: AiConfiguration,
    sip: SipConfiguration,
}
#[derive(Serialize)]
struct SipConfiguration {
    carrier_endpoint: &'static str,
}
#[derive(Serialize)]
struct AiConfiguration {
    provider: &'static str,
}

pub fn initialize(
    directory: &Path,
    address: IpAddr,
    origin: String,
    owner: Option<String>,
    lab: bool,
) -> anyhow::Result<()> {
    initialize_with(directory, address, origin, owner, lab, generate_keys)
}

pub fn generate_keys(keys: &Path) -> anyhow::Result<()> {
    let status = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "rsa:2048", "-keyout"])
        .arg(keys.join("dtls.key"))
        .arg("-out")
        .arg(keys.join("dtls.crt"))
        .args(["-days", "3650", "-nodes", "-subj", "/CN=homephone.invalid"])
        .output()?;
    ensure!(
        status.status.success(),
        "DTLS certificate generation failed"
    );
    let status = Command::new("openssl")
        .args([
            "ecparam",
            "-name",
            "prime256v1",
            "-genkey",
            "-noout",
            "-out",
        ])
        .arg(keys.join("vapid.pem"))
        .output()?;
    ensure!(status.status.success(), "VAPID key generation failed");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["dtls.key", "dtls.crt", "vapid.pem"] {
            std::fs::set_permissions(
                keys.join(name),
                std::fs::Permissions::from_mode(PRIVATE_FILE_MODE),
            )?;
        }
    }
    Ok(())
}

pub fn initialize_with<F: FnOnce(&Path) -> anyhow::Result<()>>(
    directory: &Path,
    address: IpAddr,
    origin: String,
    owner: Option<String>,
    lab: bool,
    keys: F,
) -> anyhow::Result<()> {
    ensure!(
        lab || (origin.starts_with("https://")
            && owner.as_ref().is_some_and(|owner| !owner.is_empty())),
        "production requires an HTTPS origin and an allowed user"
    );
    ensure!(
        !directory.join("config").exists(),
        "configuration already exists; preserve its credentials"
    );
    if directory.exists() {
        ensure!(
            !std::fs::symlink_metadata(directory)?
                .file_type()
                .is_symlink(),
            "installation directory cannot be a symlink"
        );
        ensure!(
            std::fs::read_dir(directory)?.next().is_none(),
            "initialize an empty installation directory"
        );
    }
    let parent = directory
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let directory = parent.join(
        directory
            .file_name()
            .context("installation root must have a name")?,
    );
    let staging = parent.join(format!(".homephone-{}.partial", Uuid::new_v4()));
    private_directory(&staging)?;
    let operation = (|| -> anyhow::Result<()> {
        let configuration_directory = staging.join("config");
        private_directory(&configuration_directory)?;
        private_directory(&configuration_directory.join("keys"))?;
        keys(&configuration_directory.join("keys"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in ["dtls.key", "dtls.crt", "vapid.pem"] {
                std::fs::set_permissions(
                    configuration_directory.join("keys").join(name),
                    std::fs::Permissions::from_mode(PRIVATE_FILE_MODE),
                )?;
            }
        }
        let ari = if lab {
            "lab-ari-password".to_owned()
        } else {
            secret()
        };
        let browser = if lab {
            "lab-sip-password".to_owned()
        } else {
            secret()
        };
        let gateway = secret();
        for (name, body) in crate::asterisk::render(&crate::asterisk::Configuration {
            address,
            ari_password: &ari,
            browser_password: &browser,
            gateway_password: &gateway,
            lab,
        })? {
            private_file(&configuration_directory.join(name), body.as_bytes())?;
        }
        let configuration = BootConfiguration {
            instance_id: Uuid::new_v4(),
            mode: if lab { "lab" } else { "production" },
            data_dir: directory.join("data"),
            public_origin: origin,
            allowed_users: owner.into_iter().collect(),
            ai: AiConfiguration {
                provider: if lab { "mock" } else { "openai" },
            },
            sip: SipConfiguration {
                carrier_endpoint: if lab { "lab-carrier" } else { "fxo" },
            },
        };
        private_file(
            &configuration_directory.join("homephone.toml"),
            toml::to_string_pretty(&configuration)?.as_bytes(),
        )?;
        private_file(&configuration_directory.join("secrets.env"),format!("HOMEPHONE_ARI_PASSWORD={ari}\nHOMEPHONE_SIP_PASSWORD={browser}\nHOMEPHONE_GATEWAY_PASSWORD={gateway}\nHOMEPHONE_API_TOKEN={}\nHOMEPHONE_WEBHOOK_SECRET={}\nHOMEPHONE_VAPID_PRIVATE_KEY=@keys/vapid.pem\n",secret(),secret()).as_bytes())?;
        for child in [
            "data",
            "spool",
            "spool/recording",
            "spool/tmp",
            "run",
            "logs",
            "cache",
        ] {
            private_directory(&staging.join(child))?;
        }
        std::fs::rename(&staging, &directory)?;
        sync_parent(&directory)?;
        Ok(())
    })();
    if operation.is_err() {
        return finish_cleanup(operation, std::fs::remove_dir_all(&staging));
    }
    operation
}

pub fn start_asterisk(directory: &Path, image: &str, name: &str) -> anyhow::Result<()> {
    let directory = directory.canonicalize()?;
    ensure!(
        name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid container name"
    );
    let user = Command::new("id").arg("-u").output()?;
    ensure!(user.status.success(), "cannot determine the service UID");
    let user = String::from_utf8(user.stdout)?.trim().to_owned();
    let group = Command::new("id").arg("-g").output()?;
    ensure!(group.status.success(), "cannot determine the service GID");
    let user = format!("{user}:{}", String::from_utf8(group.stdout)?.trim());
    let mut command = Command::new("docker");
    let configuration: toml::Value = toml::from_str(&std::fs::read_to_string(
        directory.join("config/homephone.toml"),
    )?)?;
    let id = configuration["instance_id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .context("missing installation identity")?;
    command.args([
        "run",
        "--detach",
        "--name",
        name,
        "--label",
        &format!("homephone.instance={id}"),
        "--restart",
        "unless-stopped",
        "--network",
        "host",
        "--user",
        &user,
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--pids-limit",
        "256",
        "--memory",
        "512m",
        "--cpus",
        "2",
        "--log-opt",
        "max-size=10m",
        "--log-opt",
        "max-file=3",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=32m",
    ]);
    for (source, target, readonly) in [
        ("config", GATEWAY_CONFIG_DIRECTORY, true),
        ("spool", GATEWAY_SPOOL_DIRECTORY, false),
        ("run", GATEWAY_RUNTIME_DIRECTORY, false),
        ("logs", GATEWAY_LOG_DIRECTORY, false),
        ("cache", GATEWAY_CACHE_DIRECTORY, false),
    ] {
        let suffix = if readonly { ",readonly" } else { "" };
        command.args([
            "--mount",
            &format!(
                "type=bind,source={},target={target}{suffix}",
                directory.join(source).display()
            ),
        ]);
    }
    let status = command.arg(image).status()?;
    ensure!(status.success(), "Asterisk container did not start");
    Ok(())
}
