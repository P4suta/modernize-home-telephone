const HTTP_OK: u16 = 200;
const HTTP_ACCEPTED: u16 = 202;
const CALL_COMPLETION_ATTEMPTS: usize = 120;
const CALL_POLL_INTERVAL_MS: u64 = 100;

use super::*;
use std::collections::{BTreeMap, BTreeSet};

const OPERATIONS: [&str; 3] = [
    "successful-update",
    "failed-candidate",
    "interrupted-update",
];

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    schema: u32,
    source: String,
    instance_id: uuid::Uuid,
    sample_id: uuid::Uuid,
    successful_update: bool,
    failed_candidate_recovered: bool,
    journal_recovered: bool,
    restored_installation_booted: bool,
    recording_sha256: String,
    artifacts: String,
    manifests: BTreeMap<String, String>,
}
pub fn check_report(root: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let report: Evidence = serde_json::from_slice(bytes)?;
    ensure!(
        report.schema == 1
            && report.source == crate::package::source_identity(root)?
            && report.successful_update
            && report.failed_candidate_recovered
            && report.journal_recovered
            && report.restored_installation_booted,
        "missing or stale deployment evidence"
    );
    ensure!(
        phone_core::artifact::RelativePath::parse(report.artifacts.as_bytes()).is_some()
            && report.artifacts.starts_with(".cache/deployment/")
            && report.manifests.len() == OPERATIONS.len(),
        "invalid deployment artifact inventory"
    );
    let directory = root.join(report.artifacts);
    for operation in OPERATIONS {
        let backup = directory.join(operation);
        ensure!(
            report.manifests.get(operation)
                == Some(&bundle::digest(&backup.join("manifest.json"))?),
            "deployment manifest changed"
        );
        bundle::verify(&backup, Kind::Backup)?;
    }
    ensure!(
        bundle::digest(
            &directory.join(format!("restored/data/recordings/{}.wav", report.sample_id))
        )? == report.recording_sha256,
        "restored recording changed"
    );
    Ok(())
}
pub fn evidence() -> anyhow::Result<()> {
    check_report(
        Path::new("."),
        &std::fs::read(".cache/assurance/deployment.json")?,
    )
}

fn calls(installation: &Installation) -> anyhow::Result<Vec<serde_json::Value>> {
    let response = installation.client.request(Operation::Calls)?;
    ensure!(response.status == HTTP_OK, "lab history is unavailable");
    Ok(serde_json::from_value(
        response.body.context("missing lab history")?,
    )?)
}
fn sample(installation: &Installation) -> anyhow::Result<uuid::Uuid> {
    let previous = calls(installation)?;
    ensure!(
        previous.iter().all(|call| call["phase"] == "ended"),
        "lab rehearsal requires an idle line"
    );
    let known: BTreeSet<_> = previous
        .iter()
        .filter_map(|call| call["id"].as_str())
        .collect();
    ensure!(
        installation.client.request(Operation::LabIncoming)?.status == HTTP_ACCEPTED,
        "lab carrier refused the sample call"
    );
    for _ in 0..CALL_COMPLETION_ATTEMPTS {
        if let Some(call) = calls(installation)?.iter().find(|call| {
            call["id"].as_str().is_some_and(|id| !known.contains(id))
                && call["phase"] == "ended"
                && call["processing"] == "complete"
                && call["recording"]["status"] == "available"
        }) {
            return Ok(uuid::Uuid::parse_str(
                call["id"].as_str().context("invalid sample identity")?,
            )?);
        }
        std::thread::sleep(std::time::Duration::from_millis(CALL_POLL_INTERVAL_MS));
    }
    anyhow::bail!("the mock call did not finish and capture its recording")
}
pub fn run(directory: &Path) -> anyhow::Result<()> {
    require_lab(directory)?;
    let directory = absolute(directory)?;
    let source = crate::package::source_identity(Path::new("."))?;
    let installation = Installation::open(&directory)?;
    ensure!(
        installation.release.source == source,
        "rebuild the lab for the current sources before deployment verification"
    );
    installation.health()?;
    let sample = sample(&installation)?;
    let recording = PathBuf::from("data/recordings").join(format!("{sample}.wav"));
    let expected = bundle::digest(&directory.join(&recording))?;
    let relative = Path::new(".cache/deployment").join(uuid::Uuid::new_v4().to_string());
    let run = absolute(&relative)?;
    private_directory(&run)?;
    let restored = run.join("restored");
    let operation = (|| -> anyhow::Result<()> {
        replace(
            &directory,
            installation.release.clone(),
            &run.join("successful-update"),
            None,
        )?;
        let mut broken = installation.release.clone();
        broken.app = broken.gateway.clone();
        let failed = replace(&directory, broken, &run.join("failed-candidate"), None);
        ensure!(
            failed.is_err(),
            "the deliberately invalid app unexpectedly passed candidate health"
        );
        Installation::open(&directory)?.health()?;
        ensure!(
            bundle::digest(&directory.join(&recording))? == expected,
            "candidate recovery changed a recording"
        );
        let crash_backup = run.join("interrupted-update");
        backup(&directory, &crash_backup)?;
        installation.maintenance(true)?;
        atomic_json(
            &journal_path(&directory)?,
            &Journal {
                schema: 1,
                backup: crash_backup.clone(),
                previous: installation.release.clone(),
                committed: false,
            },
        )?;
        installation.stop_containers()?;
        std::fs::remove_file(directory.join(&recording))?;
        boot(&directory)?;
        ensure!(
            bundle::digest(&directory.join(&recording))? == expected,
            "journal recovery did not restore the recording"
        );
        restore(&crash_backup, &restored)?;
        let recovered = Installation::open(&restored)?;
        ensure!(
            recovered.id != installation.id,
            "a restored installation must have its own identity"
        );
        stop(&directory)?;
        boot(&restored)?;
        Installation::open(&restored)?.health()?;
        ensure!(
            bundle::digest(&restored.join(&recording))? == expected,
            "restore changed the recording"
        );
        stop(&restored)?;
        boot(&directory)?;
        Ok(())
    })();
    let cleanup = (|| -> anyhow::Result<()> {
        if restored.join("config/images.json").is_file() {
            Installation::open(&restored)?.stop_containers()?;
        }
        boot(&directory)
    })();
    crate::update_flow::with_recovery(operation, cleanup)?;
    ensure!(
        source == crate::package::source_identity(Path::new("."))?,
        "sources changed during deployment rehearsal"
    );
    let report = Evidence {
        schema: 1,
        source,
        instance_id: installation.id,
        sample_id: sample,
        successful_update: true,
        failed_candidate_recovered: true,
        journal_recovered: true,
        restored_installation_booted: true,
        recording_sha256: expected,
        artifacts: relative.to_str().context("invalid deployment path")?.into(),
        manifests: OPERATIONS
            .into_iter()
            .map(|operation| {
                Ok((
                    operation.into(),
                    bundle::digest(&run.join(operation).join("manifest.json"))?,
                ))
            })
            .collect::<anyhow::Result<_>>()?,
    };
    check_report(Path::new("."), &serde_json::to_vec(&report)?)?;
    atomic_json(Path::new(".cache/assurance/deployment.json"), &report)?;
    println!("Deployment rehearsal passed; evidence: .cache/assurance/deployment.json");
    Ok(())
}
