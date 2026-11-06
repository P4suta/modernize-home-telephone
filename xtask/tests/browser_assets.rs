#![cfg(unix)]

use std::{path::Path, process::Command};

fn observe(mode: &str, provider: &str, stale: bool) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let installation = root.join("installation");
    std::fs::create_dir_all(installation.join("config")).unwrap();
    std::fs::write(
        installation.join("config/homephone.toml"),
        toml::to_string(&serde_json::json!({
            "mode": mode,
            "instance_id": uuid::Uuid::new_v4(),
            "data_dir": installation.join("data"),
            "ai": {"provider": provider}
        }))
        .unwrap(),
    )
    .unwrap();
    if stale {
        std::fs::create_dir_all(root.join(".local/fixtures")).unwrap();
        for name in ["caller", "browser", "hold"] {
            std::fs::write(
                root.join(format!(".local/fixtures/{name}.wav")),
                b"obsolete",
            )
            .unwrap();
        }
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "browser_assets_child"])
        .current_dir(&root)
        .env("HOMEPHONE_BROWSER_ASSETS_MODE", mode)
        .env("HOMEPHONE_BROWSER_ASSETS_PROVIDER", provider)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{:?}; {:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn browser_assets_child() {
    let Ok(mode) = std::env::var("HOMEPHONE_BROWSER_ASSETS_MODE") else {
        return;
    };
    let provider = std::env::var("HOMEPHONE_BROWSER_ASSETS_PROVIDER").unwrap();
    let result = xtask::check::browser(Path::new("installation"));
    let error = format!("{:#}", result.unwrap_err());
    if mode == "lab" && provider == "mock" {
        for name in ["caller", "browser", "hold"] {
            let reader = hound::WavReader::open(format!(".local/fixtures/{name}.wav"))
                .expect("browser verification did not prepare its audio input");
            assert_eq!(reader.spec().sample_rate, 8000);
            assert_eq!(reader.spec().channels, 1);
            assert_eq!(reader.spec().bits_per_sample, 16);
            assert_eq!(reader.duration(), 32000);
        }
    } else {
        assert!(error.contains("explicitly configured mock lab"), "{error}");
        assert!(!Path::new(".local/fixtures").exists());
    }
}

#[test]
fn browser_prepares_fresh_audio_before_inspecting_a_lab_installation() {
    for stale in [false, true] {
        observe("lab", "mock", stale);
    }
}

#[test]
fn browser_audio_preparation_refuses_production_and_paid_providers() {
    for (mode, provider) in [("production", "mock"), ("lab", "openai")] {
        observe(mode, provider, false);
    }
}
