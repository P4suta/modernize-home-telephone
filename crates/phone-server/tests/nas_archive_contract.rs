#![cfg(target_os = "linux")]

#[test]
fn isolated_smb_archive_preserves_verified_audio_and_recovers_durable_work() {
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_nas-lab"))
        .env_remove("HOMEPHONE_CONFIG")
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "the isolated archive contract failed");
            return;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("the archive did not make progress within its product deadline");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
