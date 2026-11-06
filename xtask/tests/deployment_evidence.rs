use xtask::bundle::{self, Kind};

#[test]
fn deployment_evidence_requires_current_sources_complete_operations_and_unchanged_recoverable_files()
 {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "fixture source").unwrap();
    let artifacts = ".cache/deployment/fixture";
    let directory = root.path().join(artifacts);
    let mut manifests = serde_json::Map::new();
    for operation in [
        "successful-update",
        "failed-candidate",
        "interrupted-update",
    ] {
        let backup = directory.join(operation);
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(backup.join("recording.wav"), "preserved recording").unwrap();
        bundle::seal(&backup, Kind::Backup).unwrap();
        manifests.insert(
            operation.into(),
            serde_json::json!(bundle::digest(&backup.join("manifest.json")).unwrap()),
        );
    }
    let sample = uuid::Uuid::new_v4();
    let recording = directory.join(format!("restored/data/recordings/{sample}.wav"));
    std::fs::create_dir_all(recording.parent().unwrap()).unwrap();
    std::fs::write(&recording, "preserved recording").unwrap();
    let report = serde_json::json!({"schema":1,"source":xtask::package::source_identity(root.path()).unwrap(),"instance_id":uuid::Uuid::new_v4(),"sample_id":sample,"successful_update":true,"failed_candidate_recovered":true,"journal_recovered":true,"restored_installation_booted":true,"recording_sha256":bundle::digest(&recording).unwrap(),"artifacts":artifacts,"manifests":manifests});
    let check = |value: &serde_json::Value| {
        xtask::manager::check_rehearsal_report(root.path(), &serde_json::to_vec(value).unwrap())
    };
    check(&report).unwrap();
    for field in [
        "successful_update",
        "failed_candidate_recovered",
        "journal_recovered",
        "restored_installation_booted",
    ] {
        let mut changed = report.clone();
        changed[field] = serde_json::json!(false);
        assert!(check(&changed).is_err(), "{field}");
    }
    assert!(check(&serde_json::json!({})).is_err());
    std::fs::write(&recording, "changed recording").unwrap();
    assert!(check(&report).is_err());
    std::fs::write(&recording, "preserved recording").unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "different source").unwrap();
    assert!(check(&report).is_err());
    std::fs::write(root.path().join("Cargo.toml"), "fixture source").unwrap();
    let backup = directory.join("failed-candidate");
    std::fs::write(backup.join("recording.wav"), "changed backup").unwrap();
    assert!(check(&report).is_err());
    std::fs::remove_file(backup.join("manifest.json")).unwrap();
    bundle::seal(&backup, Kind::Backup).unwrap();
    assert!(check(&report).is_err());
}
