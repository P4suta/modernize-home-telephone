use anyhow::{Context, ensure};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const PASSWORD: &str = "gatewayAuthenticationFixture";
const ARI_PASSWORD: &str = "ariAuthenticationFixture";
const CALLER: &str = "0312345678";

pub fn check_channel(
    value: &serde_json::Value,
    endpoint: &str,
    context: &str,
) -> anyhow::Result<()> {
    let channels = value.as_array().context("missing ARI channel inventory")?;
    ensure!(
        channels.len() == 1,
        "expected exactly one authenticated SIP channel"
    );
    let channel = &channels[0];
    ensure!(
        channel["name"]
            .as_str()
            .and_then(|name| name.strip_prefix(&format!("PJSIP/{endpoint}-")))
            .is_some_and(
                |suffix| suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
            )
            && channel["dialplan"]["context"] == context
            && channel["caller"]["number"] == CALLER,
        "SIP authentication changed the endpoint, context, or caller number"
    );
    Ok(())
}

fn channels() -> anyhow::Result<serde_json::Value> {
    let result = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "1",
            "--user",
            &format!("homephone:{ARI_PASSWORD}"),
            "http://127.0.0.1:18088/ari/channels",
        ])
        .output()?;
    ensure!(
        result.status.success(),
        "isolated ARI fixture is unavailable"
    );
    Ok(serde_json::from_slice(&result.stdout)?)
}

fn await_empty_channels() -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if channels()?.as_array().is_some_and(Vec::is_empty) {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "SIP authentication case left an admitted call"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

struct Container {
    name: String,
    removed: bool,
}
impl Container {
    fn new(name: String) -> Self {
        Self {
            name,
            removed: false,
        }
    }
    fn remove_with(
        &mut self,
        remove: impl FnOnce(&str) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if !self.removed {
            remove(&self.name)?;
            self.removed = true;
        }
        Ok(())
    }
    fn remove(&mut self) -> anyhow::Result<()> {
        self.remove_with(|name| crate::operations::run("docker", &["rm", "--force", name]))
    }
}
impl Drop for Container {
    fn drop(&mut self) {
        if let Err(error) = self.remove() {
            eprintln!("SIP authentication fixture cleanup failed: {error:#}");
        }
    }
}

fn message(
    method: &str,
    sequence: u8,
    user: &str,
    password: &str,
    authenticated: bool,
    body: bool,
) -> String {
    let from = if method == "REGISTER" { user } else { CALLER };
    let target = if method == "REGISTER" { "" } else { "1@" };
    let to = if method == "REGISTER" { user } else { "1" };
    let tag = if matches!(method, "ACK" | "BYE") {
        "[peer_tag_param]"
    } else {
        ""
    };
    let retransmit = if method == "ACK" {
        ""
    } else {
        " retrans=\"500\""
    };
    let auth = if authenticated {
        format!("[authentication username={user} password={password}]\n")
    } else {
        String::new()
    };
    let content = if body {
        "Content-Type: application/sdp\nContent-Length: [len]\n\nv=0\no=fixture 1 1 IN IP4 127.0.0.1\ns=fixture\nc=IN IP4 127.0.0.1\nt=0 0\nm=audio [media_port] RTP/AVP 0\na=rtpmap:0 PCMU/8000\n"
    } else {
        "Content-Length: 0\n\n"
    };
    format!(
        "<send{retransmit}><![CDATA[\n{method} sip:{target}[remote_ip]:[remote_port] SIP/2.0\nVia: SIP/2.0/UDP [local_ip]:[local_port];branch=z9hG4bK-[call_number]-{sequence}\nFrom: <sip:{from}@[remote_ip]>;tag=[call_number]\nTo: <sip:{to}@[remote_ip]>{tag}\nCall-ID: [call_id]\nCSeq: {sequence} {method}\nContact: <sip:{user}@[local_ip]:[local_port]>\nMax-Forwards: 70\nExpires: 60\n{auth}{content}]]></send>\n"
    )
}

fn scenario(
    user: &str,
    register: bool,
    allowed: bool,
    authenticate: bool,
    password: &str,
) -> String {
    let method = if register { "REGISTER" } else { "INVITE" };
    let mut steps = message(method, 1, user, password, false, !register);
    steps += "<recv response=\"401\" auth=\"true\"/>\n";
    if !register {
        steps += &message("ACK", 1, user, password, false, false);
    }
    if authenticate {
        steps += &message(method, 2, user, password, true, !register);
        if allowed {
            if !register {
                steps += "<recv response=\"100\" optional=\"true\"/>\n<recv response=\"180\" optional=\"true\"/>\n";
            }
            steps += "<recv response=\"200\"/>\n";
            if !register {
                steps += &message("ACK", 2, user, password, false, false);
                steps += "<pause milliseconds=\"1000\"/>\n";
                steps += &message("BYE", 3, user, password, false, false);
                steps += "<recv response=\"200\"/>\n";
            }
        } else {
            steps += "<recv response=\"401\"/>\n";
            steps += &message("ACK", 2, user, password, false, false);
        }
    }
    format!(
        "<?xml version=\"1.0\"?><scenario name=\"Gateway authentication\">\n{steps}</scenario>\n"
    )
}

pub fn run(directory: &Path, gateway: &crate::manager::Image) -> anyhow::Result<Vec<PathBuf>> {
    let gateway_image = crate::manager::verified_image_id(gateway)?;
    let directory = directory.join("gateway-authentication");
    crate::operations::initialize(
        &directory,
        "127.0.0.1".parse()?,
        "http://localhost:8787".into(),
        true,
    )?;
    let config = crate::rusterisk::render(&crate::rusterisk::Configuration {
        address: "127.0.0.1".parse()?,
        ari_password: ARI_PASSWORD,
        browser_password: PASSWORD,
        gateway_password: PASSWORD,
        lab: false,
    })?;
    for (name, body) in config {
        let body = match name.as_str() {
            "pjsip.conf" => body.replace("bind=0.0.0.0:5060", "bind=127.0.0.1:15060"),
            "http.conf" => body.replace("bindport=8088", "bindport=18088"),
            "rtp.conf" => body.replace("10000", "16100").replace("10099", "16199"),
            "extensions.conf" => body
                .replace("Stasis(homephone,inbound)", "Answer()\n same => n,Wait(10)")
                .replace(
                    "Stasis(homephone,outbound,${EXTEN})",
                    "Answer()\n same => n,Wait(10)",
                ),
            _ => body,
        };
        std::fs::write(directory.join("config/rusterisk").join(name), body)?;
    }
    let mut gateway = Container::new(format!(
        "homephone-sip-auth-gateway-{}",
        uuid::Uuid::new_v4()
    ));
    crate::operations::start_gateway(
        &directory,
        &gateway_image,
        &gateway.name,
        crate::manager::RUSTERISK_RELEASE_SCHEMA,
    )?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if channels().is_ok() {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "isolated gateway did not become ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let image = super::image()?;
    use std::os::unix::fs::MetadataExt as _;
    let metadata = std::fs::metadata(&directory)?;
    let user_id = format!("{}:{}", metadata.uid(), metadata.gid());
    let mut artifacts = vec![];
    for (name, user, register, allowed, authenticate, password, context) in [
        (
            "fxo-register",
            "fxo",
            true,
            true,
            true,
            PASSWORD,
            "home-carrier",
        ),
        (
            "handset-register",
            "handset",
            true,
            true,
            true,
            PASSWORD,
            "home-outbound",
        ),
        (
            "fxo-caller",
            "fxo",
            false,
            true,
            true,
            PASSWORD,
            "home-carrier",
        ),
        (
            "handset-caller",
            "handset",
            false,
            true,
            true,
            PASSWORD,
            "home-outbound",
        ),
        (
            "wrong-password",
            "fxo",
            false,
            false,
            true,
            "wrongFixtureCredential",
            "",
        ),
        (
            "unknown-caller",
            "unknown",
            false,
            false,
            true,
            PASSWORD,
            "",
        ),
        ("unauthenticated", "fxo", false, false, false, PASSWORD, ""),
    ] {
        let xml = directory.join(format!("{name}.xml"));
        let dialog = uuid::Uuid::new_v4().simple().to_string();
        crate::operations::private_file(
            &xml,
            scenario(user, register, allowed, authenticate, password)
                .replace("z9hG4bK-[call_number]-", &format!("z9hG4bK-{dialog}-"))
                .as_bytes(),
        )?;
        let log = directory.join(format!("{name}.log"));
        let output = std::fs::File::create(&log)?;
        let mut client = Container::new(format!(
            "homephone-sip-auth-client-{}",
            uuid::Uuid::new_v4()
        ));
        let mount = format!(
            "type=bind,source={},target=/artifacts",
            directory.canonicalize()?.display()
        );
        let mut child = Command::new("docker")
            .args([
                "run",
                "--name",
                &client.name,
                "--user",
                &user_id,
                "--workdir",
                "/tmp",
                "--network",
                "host",
                "--read-only",
                "--tmpfs",
                "/tmp:rw,nosuid,nodev,size=16m",
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
                "--mount",
                &mount,
                &image,
                "127.0.0.1:15060",
                "-sf",
                &format!("/artifacts/{name}.xml"),
                "-i",
                "127.0.0.1",
                "-p",
                "15080",
                "-auth_uri",
                if register {
                    "127.0.0.1:15060"
                } else {
                    "1@127.0.0.1:15060"
                },
                "-mi",
                "127.0.0.1",
                "-mp",
                "16000",
                "-rtp_echo",
                "-m",
                "1",
                "-cid_str",
                &format!("{dialog}-[call_number]@[local_ip]"),
                "-nostdin",
                "-timeout",
                "8s",
                "-timeout_error",
                "-trace_stat",
                "-stf",
                &format!("/artifacts/{name}.csv"),
            ])
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::from(output))
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut observed = false;
        let status = loop {
            if allowed
                && !register
                && let Ok(value) = channels()
                && check_channel(&value, user, context).is_ok()
                && !observed
            {
                let snapshot = directory.join(format!("{name}-channel.txt"));
                crate::operations::private_file(
                    &snapshot,
                    serde_json::to_string(&value)?.as_bytes(),
                )?;
                artifacts.push(snapshot);
                observed = true;
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                client.remove()?;
                child.wait()?;
                anyhow::bail!("SIP authentication fixture exceeded its bound");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        client.remove()?;
        ensure!(
            status.success(),
            "SIP authentication case {name} failed; inspect {}",
            log.display()
        );
        ensure!(
            register || !allowed || observed,
            "SIP authentication case {name} has no matching ARI observation"
        );
        let report = directory.join(format!("{name}.csv"));
        super::check_client_report(&std::fs::read_to_string(&report)?)?;
        await_empty_channels()?;
        artifacts.extend([xml, log, report]);
    }
    gateway.remove()?;
    Ok(artifacts)
}

#[cfg(test)]
#[test]
fn fixture_cleanup_reports_removal_failure_and_confirms_success_once() {
    let mut fixture = Container::new("fixture".into());
    assert!(
        fixture
            .remove_with(|_| anyhow::bail!("removal failed"))
            .is_err()
    );
    fixture
        .remove_with(|name| {
            assert_eq!(name, "fixture");
            Ok(())
        })
        .unwrap();
    fixture
        .remove_with(|_| panic!("container was already removed"))
        .unwrap();
}
