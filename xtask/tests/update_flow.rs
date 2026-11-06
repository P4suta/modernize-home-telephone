use xtask::update_flow::{self, Port};

#[derive(Default)]
struct Runtime {
    trace: Vec<&'static str>,
    fail: Option<&'static str>,
    secondary: Option<&'static str>,
}
impl Runtime {
    fn effect(&mut self, name: &'static str) -> anyhow::Result<()> {
        self.trace.push(name);
        if self.fail == Some(name) {
            self.fail = None;
            anyhow::bail!("{name} injected failure");
        }
        if self.fail.is_none() && self.secondary == Some(name) {
            self.secondary = None;
            anyhow::bail!("{name} injected failure");
        }
        Ok(())
    }
}
impl Port for Runtime {
    fn quiesce(&mut self) -> anyhow::Result<()> {
        self.effect("quiesce")
    }
    fn backup(&mut self) -> anyhow::Result<()> {
        self.effect("backup")
    }
    fn stop(&mut self) -> anyhow::Result<()> {
        self.effect("stop")
    }
    fn candidate(&mut self) -> anyhow::Result<()> {
        self.effect("candidate")
    }
    fn health(&mut self) -> anyhow::Result<()> {
        self.effect("health")
    }
    fn commit(&mut self) -> anyhow::Result<()> {
        self.effect("commit")
    }
    fn restore(&mut self) -> anyhow::Result<()> {
        self.effect("restore")
    }
    fn previous(&mut self) -> anyhow::Result<()> {
        self.effect("previous")
    }
    fn resume(&mut self) -> anyhow::Result<()> {
        self.effect("resume")
    }
}
#[test]
fn recovery_errors_preserve_both_the_primary_failure_and_the_failed_recovery_operation() {
    for (primary, secondary) in [
        ("backup", "resume"),
        ("candidate", "restore"),
        ("health", "previous"),
        ("commit", "stop"),
        ("candidate", "resume"),
    ] {
        let mut runtime = Runtime {
            fail: Some(primary),
            secondary: Some(secondary),
            ..Runtime::default()
        };
        let error = update_flow::replace(&mut runtime).unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(
            diagnostic.contains(&format!("{primary} injected failure")),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains(&format!("{secondary} injected failure")),
            "{diagnostic}"
        );
    }
}

#[test]
fn successful_replacement_commits_only_after_health_and_resumes_after_commit() {
    let mut runtime = Runtime::default();
    assert_eq!(
        update_flow::replace(&mut runtime).unwrap(),
        phone_core::update::Phase::Committed
    );
    assert_eq!(
        runtime.trace,
        [
            "quiesce",
            "backup",
            "stop",
            "candidate",
            "health",
            "commit",
            "resume"
        ]
    );
}
#[test]
fn failures_restore_the_previous_installation_before_reopening_the_line() {
    for point in ["stop", "candidate", "health", "commit"] {
        let mut runtime = Runtime {
            fail: Some(point),
            ..Runtime::default()
        };
        assert!(update_flow::replace(&mut runtime).is_err());
        assert!(
            runtime
                .trace
                .ends_with(&["stop", "restore", "previous", "health", "resume"]),
            "{:?}",
            runtime.trace
        );
    }
}
#[test]
fn busy_or_failed_backup_does_not_stop_service_and_committed_calls_are_never_rolled_back() {
    let mut busy = Runtime {
        fail: Some("quiesce"),
        ..Runtime::default()
    };
    assert!(update_flow::replace(&mut busy).is_err());
    assert_eq!(busy.trace, ["quiesce"]);
    let mut backup = Runtime {
        fail: Some("backup"),
        ..Runtime::default()
    };
    assert!(update_flow::replace(&mut backup).is_err());
    assert_eq!(backup.trace, ["quiesce", "backup", "resume"]);
    let mut resume = Runtime {
        fail: Some("resume"),
        ..Runtime::default()
    };
    assert!(update_flow::replace(&mut resume).is_err());
    assert!(!resume.trace.contains(&"restore"));
}
