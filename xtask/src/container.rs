use anyhow::{Context, ensure};
use clap::ValueEnum;
use std::process::Command;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Engine {
    #[default]
    Docker,
    Podman,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    pub fn run_options(self) -> &'static [&'static str] {
        match self {
            Self::Docker => &["--log-opt", "max-size=10m", "--log-opt", "max-file=3"],
            Self::Podman => &[
                "--userns=keep-id",
                "--log-driver=k8s-file",
                "--log-opt=max-size=10mb",
            ],
        }
    }
}

pub fn selected() -> anyhow::Result<Engine> {
    match std::env::var("HOMEPHONE_CONTAINER_ENGINE") {
        Ok(value) => Engine::from_str(&value, false).map_err(anyhow::Error::msg),
        Err(std::env::VarError::NotPresent) => Ok(Engine::Docker),
        Err(error) => Err(error).context("invalid container engine"),
    }
}

pub fn command() -> anyhow::Result<Command> {
    Ok(Command::new(selected()?.name()))
}

pub fn absent(name: &str) -> anyhow::Result<bool> {
    if selected()? == Engine::Docker {
        return Ok(false);
    }
    let status = command()?.args(["container", "exists", name]).status()?;
    ensure!(
        matches!(status.code(), Some(0 | 1)),
        "container lookup failed"
    );
    Ok(status.code() == Some(1))
}
