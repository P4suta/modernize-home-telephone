const VENDOR_SOURCE_COUNT: usize = 14;
const SHA256_HEX_DIGITS: usize = 64;
const PROBE_USER: &str = "65534:65534";
const PROBE_MEMORY: &str = "64m";
const PROBE_PROCESSES: &str = "16";

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    schema: u32,
    source: BTreeMap<String, String>,
    artifacts: BTreeMap<String, String>,
}
struct Trace {
    directory: PathBuf,
    source: BTreeMap<String, String>,
    files: Vec<PathBuf>,
}
impl Trace {
    fn new() -> anyhow::Result<Self> {
        let directory = Path::new(".cache/check-runs").join(uuid::Uuid::new_v4().to_string());
        crate::operations::private_directory(&directory)?;
        Ok(Self {
            directory,
            source: crate::package::source_files(Path::new("."))?,
            files: vec![],
        })
    }
    fn record(&mut self, program: &str, args: &[&str]) -> anyhow::Result<(bool, Vec<u8>, Vec<u8>)> {
        let result = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("start {program}"))?;
        let mut bytes = result.stdout.clone();
        bytes.extend_from_slice(&result.stderr);
        let path = self.directory.join(format!("{:02}.log", self.files.len()));
        crate::operations::private_file(&path, &bytes)?;
        self.files.push(path);
        Ok((result.status.success(), result.stdout, bytes))
    }
    fn run(&mut self, program: &str, args: &[&str]) -> anyhow::Result<()> {
        let (success, _, bytes) = self.record(program, args)?;
        print!("{}", String::from_utf8_lossy(&bytes));
        ensure!(success, "{program} failed or is incomplete");
        Ok(())
    }
    fn capture(&mut self, program: &str, args: &[&str]) -> anyhow::Result<Vec<u8>> {
        let (success, stdout, bytes) = self.record(program, args)?;
        ensure!(
            success,
            "{program} failed or is incomplete: {}",
            String::from_utf8_lossy(&bytes)
        );
        Ok(stdout)
    }
    fn finish(self, name: &str) -> anyhow::Result<()> {
        ensure!(
            self.source == crate::package::source_files(Path::new("."))?,
            "sources changed during {name}"
        );
        let artifacts = self
            .files
            .into_iter()
            .map(|path| -> anyhow::Result<_> {
                Ok((
                    path.to_str().context("invalid evidence path")?.into(),
                    crate::bundle::digest(&path)?,
                ))
            })
            .collect::<anyhow::Result<_>>()?;
        crate::manager::atomic_json(
            &Path::new(".cache/assurance").join(format!("{name}.json")),
            &Stamp {
                schema: 1,
                source: self.source,
                artifacts,
            },
        )
    }
}
pub fn evidence(name: &str) -> anyhow::Result<()> {
    let value = std::fs::read(Path::new(".cache/assurance").join(format!("{name}.json")))?;
    check_evidence(Path::new("."), &value)
}

pub fn check_nas_report(report: &str, expected: &[String]) -> anyhow::Result<()> {
    let inventory: BTreeSet<_> = expected.iter().cloned().collect();
    ensure!(
        !inventory.is_empty() && inventory.len() == expected.len(),
        "NAS case inventory is empty or duplicated"
    );
    let mut actual = BTreeSet::new();
    let mut complete = None;
    for line in report.lines() {
        if let Some(name) = line.strip_prefix("NAS PASS ") {
            ensure!(
                complete.is_none() && inventory.contains(name) && actual.insert(name.to_owned()),
                "unknown, duplicated, or late NAS case"
            );
        } else if let Some(count) = line.strip_prefix("NAS COMPLETE ") {
            ensure!(complete.is_none(), "NAS completion was repeated");
            complete = Some(count.parse::<usize>()?);
        } else {
            ensure!(
                !line.starts_with("NAS "),
                "NAS case was skipped, retried, or failed"
            );
        }
    }
    ensure!(
        actual == inventory && complete == Some(inventory.len()),
        "NAS run is incomplete"
    );
    Ok(())
}

pub fn nas() -> anyhow::Result<()> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Inventory {
        cases: Vec<String>,
    }
    let inventory: Inventory = toml::from_str(&std::fs::read_to_string("verification/nas.toml")?)?;
    let mut trace = Trace::new()?;
    trace.run(
        "docker",
        &[
            "build",
            "--platform",
            "linux/arm64",
            "-f",
            "verification/Dockerfile.nas",
            "-t",
            "homephone-verification-nas:local",
            ".",
        ],
    )?;
    let image = Command::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{.Id}}",
            "homephone-verification-nas:local",
        ])
        .output()?;
    ensure!(
        image.status.success(),
        "NAS verification image is unavailable"
    );
    let image = String::from_utf8(image.stdout)?.trim().to_owned();
    ensure!(
        image.strip_prefix("sha256:").is_some_and(
            |value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        ),
        "invalid NAS verification image identity"
    );
    let result = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--pull",
            "never",
            "--network",
            "none",
            "--read-only",
            "--cpus",
            "2",
            "--memory",
            "2g",
            "--memory-swap",
            "2g",
            "--pids-limit",
            "256",
            "--ulimit",
            "core=0",
            "--cap-drop",
            "ALL",
            "--cap-add",
            "SETUID",
            "--cap-add",
            "SETGID",
            "--cap-add",
            "CHOWN",
            "--cap-add",
            "DAC_OVERRIDE",
            "--cap-add",
            "FOWNER",
            "--tmpfs",
            "/tmp:rw,noexec,nosuid,nodev,size=64m",
            "--tmpfs",
            "/run:rw,noexec,nosuid,nodev,size=16m",
            "--tmpfs",
            "/nas-share:rw,noexec,nosuid,nodev,size=16m",
            &image,
        ])
        .output()?;
    let mut bytes = result.stdout;
    bytes.extend_from_slice(&result.stderr);
    let path = trace.directory.join("nas.log");
    crate::operations::private_file(&path, &bytes)?;
    trace.files.push(path);
    print!("{}", String::from_utf8_lossy(&bytes));
    ensure!(
        result.status.success(),
        "NAS verification failed or exceeded its limits"
    );
    check_nas_report(std::str::from_utf8(&bytes)?, &inventory.cases)?;
    trace.finish("nas")
}
pub fn check_evidence(root: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let stamp: Stamp = serde_json::from_slice(bytes)?;
    ensure!(
        stamp.schema == 1
            && !stamp.source.is_empty()
            && stamp.source == crate::package::source_files(root)?
            && !stamp.artifacts.is_empty(),
        "missing or stale verification evidence"
    );
    for (name, hash) in stamp.artifacts {
        ensure!(
            phone_core::artifact::RelativePath::parse(name.as_bytes()).is_some()
                && crate::bundle::digest(&root.join(&name))? == hash,
            "evidence artifact changed"
        );
    }
    Ok(())
}
pub fn check_browser_report(value: &serde_json::Value, names: &[String]) -> anyhow::Result<()> {
    let expected: BTreeSet<_> = names.iter().cloned().collect();
    ensure!(
        !expected.is_empty() && expected.len() == names.len(),
        "browser inventory is empty or duplicated"
    );
    let stats = &value["stats"];
    ensure!(
        stats["expected"].as_u64() == Some(names.len() as u64)
            && ["unexpected", "skipped", "flaky"]
                .iter()
                .all(|name| stats[name] == 0)
            && value["errors"].as_array().is_some_and(Vec::is_empty),
        "browser run is failed, skipped, retried, or incomplete"
    );
    let mut pending: Vec<_> = value["suites"]
        .as_array()
        .context("missing browser suites")?
        .iter()
        .collect();
    let mut actual = BTreeSet::new();
    while let Some(suite) = pending.pop() {
        if let Some(children) = suite["suites"].as_array() {
            pending.extend(children);
        }
        if let Some(specs) = suite["specs"].as_array() {
            for spec in specs {
                let title = spec["title"]
                    .as_str()
                    .context("missing browser test identity")?;
                ensure!(
                    actual.insert(title.to_owned()) && spec["ok"] == true,
                    "duplicate or failed browser test"
                );
                let tests = spec["tests"]
                    .as_array()
                    .context("missing browser executions")?;
                ensure!(
                    tests.len() == 1 && tests[0]["status"] == "expected",
                    "browser test was not executed exactly once"
                );
                let results = tests[0]["results"]
                    .as_array()
                    .context("missing browser results")?;
                ensure!(
                    results.len() == 1 && results[0]["status"] == "passed",
                    "browser result was skipped, retried, or failed"
                );
            }
        }
    }
    ensure!(
        actual == expected,
        "executed browser cases differ from the inventory"
    );
    Ok(())
}
pub fn browser(directory: &Path) -> anyhow::Result<()> {
    crate::manager::require_lab(directory)?;
    crate::write_assets(Path::new(".local/fixtures"))?;
    let installation = crate::manager::Installation::open(directory)?;
    ensure!(
        installation.release.source == crate::package::source_identity(Path::new("."))?,
        "rebuild the lab for the current sources before browser verification"
    );
    installation.health()?;
    let mut trace = Trace::new()?;
    let image = format!("homephone-browser:source-{}", installation.release.source);
    trace.run(
        "docker",
        &[
            "build",
            "--platform",
            "linux/arm64",
            "-f",
            "deploy/app/Dockerfile",
            "--target",
            "browser-test",
            "-t",
            &image,
            ".",
        ],
    )?;
    trace.files.extend(crate::sip_fixture::authenticate_gateway(
        &trace.directory,
        &installation.release.gateway,
    )?);
    let carrier = crate::sip_fixture::Carrier::start(&trace.directory)?;
    let report = PathBuf::from("web/test-results/results.json");
    if report.try_exists()? {
        std::fs::remove_file(&report)?;
    }
    let results = Path::new("web/test-results");
    crate::operations::private_directory(results)?;
    use std::os::unix::fs::MetadataExt as _;
    let metadata = std::fs::metadata(results)?;
    let browser = trace.run(
        "docker",
        &[
            "run",
            "--rm",
            "--network",
            "host",
            "--platform",
            "linux/arm64",
            "--user",
            &format!("{}:{}", metadata.uid(), metadata.gid()),
            "--env",
            "HOME=/tmp",
            "--read-only",
            "--tmpfs",
            "/tmp:rw,nosuid,size=384m",
            "--shm-size",
            "256m",
            "--memory",
            "2g",
            "--cpus",
            "2",
            "--pids-limit",
            "256",
            "--security-opt",
            "no-new-privileges",
            "--cap-drop",
            "ALL",
            "--mount",
            &format!(
                "type=bind,source={},target=/project/.local/fixtures,readonly",
                Path::new(".local/fixtures").canonicalize()?.display()
            ),
            "--mount",
            &format!(
                "type=bind,source={},target=/project/web/test-results",
                results.canonicalize()?.display()
            ),
            &image,
        ],
    );
    let sip = carrier.finish(&trace.directory);
    browser?;
    sip?;
    trace.files.extend([
        trace.directory.join("sip.csv"),
        trace.directory.join("sip.log"),
    ]);
    #[derive(Deserialize)]
    struct Inventory {
        tests: Vec<String>,
    }
    let inventory: Inventory =
        toml::from_str(&std::fs::read_to_string("verification/browser.toml")?)?;
    check_browser_report(
        &serde_json::from_slice(&std::fs::read(&report)?)?,
        &inventory.tests,
    )?;
    trace.files.push(report);
    trace.finish("browser")
}
pub fn footprint(directory: &Path, phase: crate::footprint::Phase) -> anyhow::Result<()> {
    use crate::footprint::{Budgets, Preparation, evaluate, observe};
    crate::manager::require_lab(directory)?;
    let installation = crate::manager::Installation::open(directory)?;
    ensure!(
        installation.release.source == crate::package::source_identity(Path::new("."))?,
        "rebuild the lab for the current sources before footprint verification"
    );
    crate::manager::verify_images(&installation.release)?;
    match phase.preparation() {
        Preparation::Restart => {
            crate::manager::stop(directory)?;
            crate::manager::boot(directory)?;
        }
        Preparation::AfterBrowserCalls => evidence("browser")?,
    }
    installation.health()?;
    let budgets = Budgets::parse(&std::fs::read_to_string("verification/footprint.toml")?)?;
    let mut trace = Trace::new()?;
    let probe = String::from_utf8(trace.capture(
        "docker",
        &[
            "build",
            "--quiet",
            "--platform",
            "linux/arm64",
            "-f",
            "deploy/app/Dockerfile",
            "--target",
            "build",
            ".",
        ],
    )?)?
    .trim()
    .to_owned();
    ensure!(
        probe
            .strip_prefix("sha256:")
            .is_some_and(|digest| digest.len() == SHA256_HEX_DIGITS),
        "the footprint probe requires an immutable image ID"
    );
    let prefix = format!("homephone-{}-", installation.id.simple());
    let containers = String::from_utf8(trace.capture(
        "docker",
        &[
            "ps",
            "--filter",
            &format!("label=homephone.instance={}", installation.id),
            "--format",
            "{{.Names}}",
        ],
    )?)?
    .lines()
    .map(|name| {
        let service = name
            .strip_prefix(&prefix)
            .context("unexpected service container name")?;
        Ok((service.to_owned(), name.to_owned()))
    })
    .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    if phase.preparation() == Preparation::AfterBrowserCalls {
        let evidence_age = std::fs::metadata(".cache/assurance/browser.json")?
            .modified()?
            .elapsed()?
            .as_secs();
        for (service, container) in &containers {
            let age = process_age(&mut trace, &probe, container)?;
            ensure!(
                crate::footprint::follows_evidence(age, evidence_age),
                "{service} restarted after the browser calls; rerun `just browser` before the settled footprint"
            );
        }
    }
    std::thread::sleep(Duration::from_secs(budgets.settle_seconds));
    let mut earlier = BTreeMap::new();
    for (service, container) in &containers {
        let sample = status(&mut trace, &probe, container)?;
        earlier.insert(service.clone(), (sample, Instant::now()));
    }
    std::thread::sleep(Duration::from_secs(budgets.window_seconds));
    let mut services = BTreeMap::new();
    for (service, container) in &containers {
        let (before, completed) = earlier
            .get(service)
            .context("missing earlier footprint sample")?;
        let window = u64::try_from(completed.elapsed().as_millis())?;
        let after = status(&mut trace, &probe, container)?;
        services.insert(service.clone(), observe(before, &after, window)?);
    }
    let mut images = BTreeMap::new();
    for (service, image) in [
        ("app", &installation.release.app),
        ("gateway", &installation.release.gateway),
    ] {
        let size = String::from_utf8(trace.capture(
            "docker",
            &[
                "image",
                "inspect",
                "--format",
                "{{.Size}}",
                &image.reference,
            ],
        )?)?;
        images.insert(
            service.to_owned(),
            size.trim().parse::<u64>().context("invalid image size")?,
        );
    }
    let violations = evaluate(&budgets, phase, &services, &images);
    let report = trace.directory.join("footprint.json");
    let summary = serde_json::to_vec_pretty(&serde_json::json!({
        "phase": phase.name(),
        "settle_seconds": budgets.settle_seconds,
        "window_seconds": budgets.window_seconds,
        "services": services,
        "images": images,
        "violations": violations,
    }))?;
    crate::operations::private_file(&report, &summary)?;
    trace.files.push(report);
    println!("{}", String::from_utf8_lossy(&summary));
    ensure!(
        violations.is_empty(),
        "idle footprint exceeds its budget: {}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    );
    trace.finish(&format!("footprint-{}", phase.name()))
}
fn run_probe(
    trace: &mut Trace,
    image: &str,
    container: &str,
    command: &[&str],
) -> anyhow::Result<String> {
    let pid = format!("container:{container}");
    let mut arguments = vec![
        "run",
        "--rm",
        "--platform",
        "linux/arm64",
        "--pid",
        &pid,
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--user",
        PROBE_USER,
        "--memory",
        PROBE_MEMORY,
        "--pids-limit",
        PROBE_PROCESSES,
        "--entrypoint",
    ];
    let (program, rest) = command.split_first().context("empty probe command")?;
    arguments.extend([*program, image]);
    arguments.extend(rest);
    Ok(String::from_utf8(trace.capture("docker", &arguments)?)?)
}
fn status(
    trace: &mut Trace,
    image: &str,
    container: &str,
) -> anyhow::Result<crate::footprint::Sample> {
    let output = run_probe(
        trace,
        image,
        container,
        &[
            "find",
            "/proc/1/task",
            "-mindepth",
            "2",
            "-maxdepth",
            "2",
            "-name",
            "status",
            "-exec",
            "cat",
            "{}",
            "+",
        ],
    )?;
    Ok(crate::footprint::parse_sample(&output)?)
}
fn process_age(trace: &mut Trace, image: &str, container: &str) -> anyhow::Result<u64> {
    let output = run_probe(
        trace,
        image,
        container,
        &["cat", "/proc/uptime", "/proc/1/stat"],
    )?;
    let (uptime, stat) = output
        .split_once('\n')
        .context("incomplete process age probe")?;
    Ok(crate::footprint::process_age_seconds(uptime, stat)?)
}
pub fn gateway_authentication(directory: &Path) -> anyhow::Result<()> {
    crate::manager::require_lab(directory)?;
    let installation = crate::manager::Installation::open(directory)?;
    crate::manager::verify_images(&installation.release)?;
    let directory = Path::new(".cache/check-runs").join(uuid::Uuid::new_v4().to_string());
    crate::operations::private_directory(&directory)?;
    let artifacts =
        crate::sip_fixture::authenticate_gateway(&directory, &installation.release.gateway)?;
    ensure!(
        !artifacts.is_empty(),
        "missing SIP authentication observations"
    );
    println!(
        "Verified seven SIP authentication cases in {}",
        directory.display()
    );
    Ok(())
}
pub fn schema(update: bool) -> anyhow::Result<()> {
    schema_with(&mut Trace::new()?, update)
}
fn schema_with(trace: &mut Trace, update: bool) -> anyhow::Result<()> {
    let directory = PathBuf::from(".cache/schema-check");
    std::fs::create_dir_all(&directory)?;
    trace.run(
        "cargo",
        &[
            "run",
            "--locked",
            "-p",
            "phone-server",
            "--",
            "schema",
            "--output",
            ".cache/schema-check/openapi.json",
        ],
    )?;
    trace.run(
        "pnpm",
        &[
            "--dir",
            "web/codegen",
            "exec",
            "openapi-typescript",
            "../../.cache/schema-check/openapi.json",
            "-o",
            "../../.cache/schema-check/api.d.ts",
        ],
    )?;
    trace.run(
        "pnpm",
        &[
            "--dir",
            "web",
            "exec",
            "biome",
            "format",
            "--write",
            "../.cache/schema-check/api.d.ts",
        ],
    )?;
    trace.run(
        "cargo",
        &[
            "run",
            "--locked",
            "-p",
            "phone-server",
            "--bin",
            "ht813-console",
            "--",
            "schema",
            "--output",
            ".cache/schema-check/console.openapi.json",
        ],
    )?;
    trace.run(
        "pnpm",
        &[
            "--dir",
            "web/codegen",
            "exec",
            "openapi-typescript",
            "../../.cache/schema-check/console.openapi.json",
            "-o",
            "../../.cache/schema-check/console-api.d.ts",
        ],
    )?;
    trace.run(
        "pnpm",
        &[
            "--dir",
            "console",
            "exec",
            "biome",
            "format",
            "--write",
            "../.cache/schema-check/console-api.d.ts",
        ],
    )?;
    for (generated, target) in [
        (
            directory.join("openapi.json"),
            Path::new("api/openapi.json"),
        ),
        (
            directory.join("api.d.ts"),
            Path::new("web/src/generated/api.d.ts"),
        ),
        (
            directory.join("console.openapi.json"),
            Path::new("api/console.openapi.json"),
        ),
        (
            directory.join("console-api.d.ts"),
            Path::new("console/src/lib/generated/api.d.ts"),
        ),
    ] {
        if update {
            std::fs::copy(generated, target)?;
        } else {
            ensure!(
                std::fs::read(generated)? == std::fs::read(target)?,
                "generated contract differs; run just api"
            );
        }
    }
    Ok(())
}
pub fn fast() -> anyhow::Result<()> {
    let mut trace = Trace::new()?;
    trace.run("actionlint", &[])?;
    trace.run(
        "zizmor",
        &[
            "--offline",
            "--no-progress",
            "--strict-collection",
            "--persona",
            "auditor",
            ".github/workflows",
        ],
    )?;
    crate::assurance::contract_sources()?;
    crate::console::check_references(
        Path::new("."),
        &std::fs::read("console/src/lib/parameters.json")?,
    )?;
    trace.run("cargo", &["fmt", "--all", "--check"])?;
    trace.run(
        "cargo",
        &["fmt", "--manifest-path", "fuzz/Cargo.toml", "--check"],
    )?;
    trace.run(
        "cargo",
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    trace.run("cargo", &["test", "--locked", "--workspace"])?;
    trace.run(
        "cargo",
        &[
            "test",
            "--locked",
            "-p",
            "phone-server",
            "--no-default-features",
        ],
    )?;
    crate::plugins::check(Path::new("."))?;
    for features in crate::plugins::host_feature_sets() {
        let mut args = vec![
            "clippy",
            "--locked",
            "-p",
            "phone-server",
            "--no-default-features",
        ];
        if !features.is_empty() {
            args.extend(["--features", &features]);
        }
        args.extend(["--", "-D", "warnings"]);
        trace.run("cargo", &args)?;
    }
    trace.run(
        "pnpm",
        &[
            "--dir",
            "web",
            "install",
            "--frozen-lockfile",
            "--ignore-scripts",
        ],
    )?;
    trace.run(
        "pnpm",
        &[
            "--dir",
            "web",
            "exec",
            "biome",
            "check",
            "src",
            "test",
            "worker",
            "vite.config.ts",
            "playwright.config.ts",
            "package.json",
            "tsconfig.json",
        ],
    )?;
    trace.run(
        "pnpm",
        &[
            "--dir",
            "web",
            "exec",
            "oxlint",
            "--type-aware",
            "--type-check",
            "--deny-warnings",
            "src",
            "test",
            "worker",
        ],
    )?;
    trace.run("pnpm", &["--dir", "web", "check"])?;
    trace.run("pnpm", &["--dir", "web", "exec", "vite", "build"])?;
    trace.run(
        "pnpm",
        &[
            "--dir",
            "console",
            "install",
            "--frozen-lockfile",
            "--ignore-scripts",
        ],
    )?;
    trace.run("pnpm", &["--dir", "console", "lint"])?;
    trace.run("pnpm", &["--dir", "console", "check"])?;
    trace.run("pnpm", &["--dir", "console", "build"])?;
    schema_with(&mut trace, false)?;
    sources()?;
    crate::fault_catalog::check(
        Path::new("."),
        &std::fs::read_to_string("verification/faults.toml")?,
    )?;
    trace.finish("check")
}
pub fn sources() -> anyhow::Result<()> {
    sources_at(
        Path::new(".agents/skills/grandstream-ht813/references"),
        false,
    )
}
pub fn sources_at(root: &Path, required: bool) -> anyhow::Result<()> {
    let checksum = std::fs::read_to_string(root.join("sources.sha256"))?;
    let mut inventory = BTreeMap::new();
    for line in checksum.lines().filter(|line| !line.trim().is_empty()) {
        let (hash, name) = line
            .split_once("  ")
            .context("invalid vendor source checksum")?;
        ensure!(
            phone_core::artifact::RelativePath::parse(name.as_bytes()).is_some()
                && Path::new(name).parent() == Some(Path::new("upstream")),
            "vendor source path is invalid"
        );
        ensure!(
            hash.len() == SHA256_HEX_DIGITS
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "vendor source hash is invalid"
        );
        ensure!(
            inventory.insert(name, hash).is_none(),
            "duplicate vendor source"
        );
    }
    ensure!(
        inventory.len() == VENDOR_SOURCE_COUNT,
        "vendor source inventory changed"
    );
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == "upstream" {
            ensure!(
                entry.file_type()?.is_dir(),
                "vendor archive must be a real directory"
            );
        }
    }
    let mut present = 0;
    for (name, hash) in inventory {
        let path = root.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(metadata.is_file(), "vendor source must be a regular file");
                ensure!(
                    crate::bundle::digest(&path)? == hash,
                    "preserved vendor source changed: {name}"
                );
                present += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(
        present == 0 || present == VENDOR_SOURCE_COUNT,
        "vendor archive is incomplete"
    );
    ensure!(
        !required || present == VENDOR_SOURCE_COUNT,
        "preserved vendor archive is required for this operation"
    );
    Ok(())
}
pub fn security() -> anyhow::Result<()> {
    sources()?;
    let mut trace = Trace::new()?;
    trace.run("cargo", &["audit", "--deny", "warnings"])?;
    trace.run("pnpm", &["--dir", "web", "audit", "--audit-level", "low"])?;
    trace.run(
        "pnpm",
        &["--dir", "console", "audit", "--audit-level", "low"],
    )?;
    let source = PathBuf::from(".cache/secret-scan").join(uuid::Uuid::new_v4().to_string());
    crate::operations::private_directory(&source)?;
    let result = (|| -> anyhow::Result<()> {
        for name in crate::package::source_files(Path::new("."))?.keys() {
            let target = source.join(name);
            crate::operations::private_directory(
                target.parent().context("source requires a parent")?,
            )?;
            std::fs::copy(name, target)?;
        }
        trace.run(
            "gitleaks",
            &[
                "dir",
                "--redact",
                "--config",
                "verification/gitleaks.toml",
                source.to_str().context("invalid scan directory")?,
            ],
        )?;
        Ok(())
    })();
    crate::operations::finish_cleanup(result, std::fs::remove_dir_all(source))?;
    trace.finish("security")
}

pub fn console_browser() -> anyhow::Result<()> {
    let mut trace = Trace::new()?;
    let report = PathBuf::from("console/test-results/results.json");
    if report.try_exists()? {
        std::fs::remove_file(&report)?;
    }
    trace.run("pnpm", &["--dir", "console", "e2e"])?;
    #[derive(Deserialize)]
    struct Inventory {
        tests: Vec<String>,
    }
    let inventory: Inventory = toml::from_str(&std::fs::read_to_string(
        "verification/console-browser.toml",
    )?)?;
    check_browser_report(
        &serde_json::from_slice(&std::fs::read(&report)?)?,
        &inventory.tests,
    )?;
    trace.files.push(report);
    trace.finish("console_browser")
}
