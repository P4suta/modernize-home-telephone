use anyhow::{Context, ensure};

pub fn validate_dependencies(metadata: &serde_json::Value) -> anyhow::Result<()> {
    let packages = metadata["packages"]
        .as_array()
        .context("missing package inventory")?;
    for name in ["phone-core", "phone-plugin-ai"] {
        let matching = packages
            .iter()
            .filter(|package| package["name"] == name)
            .collect::<Vec<_>>();
        ensure!(
            matching.len() == 1,
            "missing or duplicated plugin boundary: {name}"
        );
        for dependency in matching[0]["dependencies"]
            .as_array()
            .context("missing dependency inventory")?
        {
            let target = dependency["name"]
                .as_str()
                .context("invalid dependency name")?;
            ensure!(
                !matches!(target, "phone-server" | "xtask") && !target.starts_with("phone-plugin-"),
                "{name} cannot depend on {target}"
            );
        }
    }
    Ok(())
}

pub fn check(root: &std::path::Path) -> anyhow::Result<()> {
    let result = std::process::Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .output()?;
    ensure!(result.status.success(), "plugin metadata is unavailable");
    validate_dependencies(&serde_json::from_slice(&result.stdout)?)
}
