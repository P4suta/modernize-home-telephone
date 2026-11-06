const VENDOR_SOURCE_COUNT: usize = 14;
const SHA256_HEX_DIGITS: usize = 64;

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
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
    fn run(&mut self, program: &str, args: &[&str]) -> anyhow::Result<()> {
        let result = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("start {program}"))?;
        let mut bytes = result.stdout;
        bytes.extend_from_slice(&result.stderr);
        let path = self.directory.join(format!("{:02}.log", self.files.len()));
        crate::operations::private_file(&path, &bytes)?;
        self.files.push(path);
        print!("{}", String::from_utf8_lossy(&bytes));
        ensure!(result.status.success(), "{program} failed or is incomplete");
        Ok(())
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
    let carrier = crate::sip_fixture::Carrier::start(&trace.directory)?;
    let report = PathBuf::from("web/test-results/results.json");
    if report.try_exists()? {
        std::fs::remove_file(&report)?;
    }
    let browser = trace.run("pnpm", &["--dir", "web", "e2e"]);
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
    for (generated, target) in [
        (
            directory.join("openapi.json"),
            Path::new("api/openapi.json"),
        ),
        (
            directory.join("api.d.ts"),
            Path::new("web/src/generated/api.d.ts"),
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
