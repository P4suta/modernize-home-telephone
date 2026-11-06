const IMAGE_ID_LENGTH: usize = 71;
const IMAGE_ID_PREFIX_LENGTH: usize = 7;
const READINESS_ATTEMPTS: usize = 50;
const POLL_INTERVAL_MS: u64 = 100;
const COMPLETION_ATTEMPTS: usize = 60;
const CONTAINER_WORK_DIRECTORY: &str = "/tmp";
const LOOPBACK_ADDRESS: &str = "127.0.0.1";

use anyhow::{Context, ensure};
use std::{collections::BTreeSet, path::Path, process::Command};

pub fn check_report(report: &str) -> anyhow::Result<()> {
    let mut lines = report.lines();
    let header: Vec<_> = lines
        .next()
        .context("missing SIPp counters")?
        .split(';')
        .collect();
    ensure!(
        header
            .iter()
            .filter(|name| !name.is_empty())
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            == header.iter().filter(|name| !name.is_empty()).count(),
        "duplicate SIPp counters"
    );
    let mut last = None;
    for line in lines {
        let fields: Vec<_> = line.split(';').collect();
        ensure!(fields.len() == header.len(), "truncated SIPp counters");
        last = Some(fields);
    }
    let last = last.context("missing SIPp observations")?;
    for (name, expected) in [
        ("IncomingCall(C)", 1),
        ("OutgoingCall(C)", 0),
        ("SuccessfulCall(C)", 1),
        ("FailedCall(C)", 0),
        ("CurrentCall", 0),
    ] {
        let index = header
            .iter()
            .position(|field| *field == name)
            .context("missing SIPp counter")?;
        ensure!(
            last[index].trim().parse::<u64>()? == expected,
            "SIPp counter {name} differs from {expected}"
        );
    }
    Ok(())
}

pub struct Carrier {
    name: String,
}
impl Carrier {
    pub fn start(directory: &Path) -> anyhow::Result<Self> {
        use sha2::{Digest as _, Sha256};
        use std::os::unix::fs::MetadataExt as _;
        crate::operations::private_directory(directory)?;
        let inputs = [
            "verification/Dockerfile.sip",
            "deploy/apt-trust.conf",
            "deploy/debian.sources",
        ]
        .map(|path| crate::bundle::digest(Path::new(path)))
        .into_iter()
        .collect::<anyhow::Result<Vec<_>>>()?;
        let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&inputs)?));
        let image = "homephone-verification-sip:local";
        let current = Command::new("docker")
            .args([
                "image",
                "inspect",
                "--format",
                "{{index .Config.Labels \"homephone.recipe\"}}",
                image,
            ])
            .output()?;
        if !current.status.success() || String::from_utf8_lossy(&current.stdout).trim() != hash {
            crate::operations::run(
                "docker",
                &[
                    "build",
                    "--platform",
                    "linux/arm64",
                    "--file",
                    "verification/Dockerfile.sip",
                    "--label",
                    &format!("homephone.recipe={hash}"),
                    "--tag",
                    image,
                    ".",
                ],
            )?;
        }
        let identity = Command::new("docker")
            .args(["image", "inspect", "--format", "{{.Id}}", image])
            .output()?;
        ensure!(identity.status.success(), "SIPp image is unavailable");
        let identity = String::from_utf8(identity.stdout)?.trim().to_owned();
        ensure!(
            identity.len() == IMAGE_ID_LENGTH
                && identity.starts_with("sha256:")
                && identity[IMAGE_ID_PREFIX_LENGTH..]
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit()),
            "SIPp requires an immutable image identity"
        );
        let metadata = std::fs::metadata(directory)?;
        let name = format!("homephone-sip-{}", uuid::Uuid::new_v4());
        let fixture = Self { name };
        crate::operations::run(
            "docker",
            &[
                "run",
                "--detach",
                "--name",
                &fixture.name,
                "--network",
                "host",
                "--read-only",
                "--tmpfs",
                "/tmp:rw,nosuid,nodev,size=16m",
                "--workdir",
                CONTAINER_WORK_DIRECTORY,
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--cpus",
                "1",
                "--memory",
                "128m",
                "--memory-swap",
                "128m",
                "--pids-limit",
                "64",
                "--user",
                &format!("{}:{}", metadata.uid(), metadata.gid()),
                "--mount",
                &format!(
                    "type=bind,source={},target=/artifacts",
                    std::fs::canonicalize(directory)?.display()
                ),
                &identity,
                "-sn",
                "uas",
                "-i",
                LOOPBACK_ADDRESS,
                "-bind_local",
                "-p",
                "5070",
                "-ci",
                LOOPBACK_ADDRESS,
                "-cp",
                "5072",
                "-mi",
                LOOPBACK_ADDRESS,
                "-mp",
                "6000",
                "-rtp_echo",
                "-nostdin",
                "-m",
                "1",
                "-timeout",
                "120s",
                "-timeout_error",
                "-trace_stat",
                "-fd",
                "1",
                "-stf",
                "/artifacts/sip.csv",
            ],
        )?;
        for _ in 0..READINESS_ATTEMPTS {
            let state = fixture.state()?;
            ensure!(state["Running"] == true, "SIPp exited before readiness");
            if directory
                .join("sip.csv")
                .metadata()
                .is_ok_and(|metadata| metadata.len() > 0)
            {
                return Ok(fixture);
            }
            std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        }
        anyhow::bail!("SIPp did not publish its initial counters")
    }
    fn state(&self) -> anyhow::Result<serde_json::Value> {
        let output = Command::new("docker")
            .args(["inspect", "--format", "{{json .State}}", &self.name])
            .output()?;
        ensure!(output.status.success(), "SIPp container inspection failed");
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    pub fn finish(&self, directory: &Path) -> anyhow::Result<()> {
        let mut completed = None;
        for _ in 0..COMPLETION_ATTEMPTS {
            let state = self.state()?;
            if state["Running"] == false {
                completed = Some(state);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        }
        let output = Command::new("docker").args(["logs", &self.name]).output()?;
        let mut bytes = output.stdout;
        bytes.extend_from_slice(&output.stderr);
        crate::operations::private_file(&directory.join("sip.log"), &bytes)?;
        let state = completed.context("SIPp did not complete its dialog")?;
        ensure!(
            state["ExitCode"] == 0 && state["OOMKilled"] == false,
            "SIPp failed or did not complete"
        );
        check_report(&std::fs::read_to_string(directory.join("sip.csv"))?)
    }
}
impl Drop for Carrier {
    fn drop(&mut self) {
        if let Err(error) = crate::operations::run("docker", &["rm", "--force", &self.name]) {
            eprintln!("SIPp fixture cleanup failed: {error:#}");
        }
    }
}
