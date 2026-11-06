mod packages;

fn main() -> anyhow::Result<()> {
    let context = match (
        std::env::var("DOTFILES_VERIFICATION_SOURCE"),
        std::env::var("DOTFILES_VERIFICATION_PLAN"),
    ) {
        (Ok(source), Ok(plan)) if !source.is_empty() && !plan.is_empty() => Some((source, plan)),
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => None,
        _ => anyhow::bail!("verification metadata is incomplete or invalid"),
    };
    let executed = packages::verify()?;
    anyhow::ensure!(
        executed == 5,
        "the package mutation inventory is incomplete"
    );
    let version = std::process::Command::new("docker")
        .arg("--version")
        .output()?;
    anyhow::ensure!(version.status.success(), "Docker version lookup failed");
    let Some((source, plan)) = context else {
        println!(
            "Verified {executed} package preparation mutations across both pinned image stages."
        );
        return Ok(());
    };
    println!(
        "{}",
        serde_json::json!({
            "version": 1, "source": source, "plan": plan,
            "method": "mutation", "scope": "affected",
            "tool_version": String::from_utf8(version.stdout)?.trim(),
            "target": "linux/arm64",
            "bounds": "Two pinned image stages; five archive, refresh-order, and epoch-filename substitutions; network-disabled resolution; every prefetched package checked against its rejecting controls.",
            "results": [{
                "name": "prefetched_packages_do_not_require_repository_downloads",
                "status": "passed", "executed": executed,
                "failed": 0, "skipped": 0, "inconclusive": 0
            }]
        })
    );
    Ok(())
}
