use anyhow::{Context, ensure};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

#[derive(Clone, Copy, PartialEq)]
enum Mutation {
    None,
    OmitArchives,
    RefreshAfterArchives,
    OmitEpochFilename,
}

fn fixture(source: &str, stage: &str, mutation: Mutation) -> anyhow::Result<String> {
    let boundary = source
        .find("\nFROM trust AS build\n")
        .context("missing package build stage")?;
    let inputs = source[..boundary]
        .lines()
        .filter(|line| !line.starts_with("ADD ") || line.ends_with(".deb"))
        .collect::<Vec<_>>()
        .join("\n");
    let from = source
        .lines()
        .find(|line| line.starts_with("FROM ") && line.ends_with(&format!(" AS {stage}")))
        .context("missing package stage")?;
    let start = source.find(from).context("missing stage offset")?;
    let (header, commands) = source[start..]
        .split_once("\nRUN ")
        .context("missing installation command")?;
    let command = commands.lines().next().context("empty installation")?;
    let (preparation, installation) = command
        .split_once("apt-get install ")
        .context("missing package installation")?;
    let arguments = installation
        .split_once(" && rm -rf ")
        .map(|(arguments, _)| arguments)
        .context("missing package installation")?;
    let commands: Vec<_> = preparation
        .split(" && ")
        .filter(|command| !command.is_empty())
        .collect();
    ensure!(
        commands
            .iter()
            .filter(|command| **command == "apt-get update")
            .count()
            == 1,
        "missing authenticated index refresh"
    );
    let changed: Vec<_> = match mutation {
        Mutation::None => commands.clone(),
        Mutation::OmitArchives => commands
            .iter()
            .copied()
            .filter(|command| !command.starts_with("cp ") && !command.starts_with("mv "))
            .collect(),
        Mutation::RefreshAfterArchives => commands
            .iter()
            .copied()
            .filter(|command| *command != "apt-get update")
            .chain(["apt-get update"])
            .collect(),
        Mutation::OmitEpochFilename => commands
            .iter()
            .copied()
            .filter(|command| !command.starts_with("mv "))
            .collect(),
    };
    ensure!(
        mutation == Mutation::None || changed != commands,
        "the preparation mutation changed nothing"
    );
    let preparation = changed.join(" && ");
    Ok(format!(
        "{inputs}\n\n{header}\nRUN {preparation}\nRUN --network=none apt-get --print-uris install {arguments} > /package-plan.txt || (cat /package-plan.txt; false)\n\nFROM scratch AS package-plan\nCOPY --from={stage} /package-plan.txt /package-plan.txt\n"
    ))
}

fn plan(source: &str) -> anyhow::Result<String> {
    let directory = tempfile::tempdir()?;
    let mut child = Command::new("docker")
        .args([
            "build",
            "--no-cache",
            "--progress=plain",
            "--platform=linux/arm64",
            "--target=package-plan",
            "--output",
        ])
        .arg(format!("type=local,dest={}", directory.path().display()))
        .args(["-f", "-", "."])
        .current_dir(root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("missing Docker input")?
        .write_all(source.as_bytes())?;
    let result = child.wait_with_output()?;
    ensure!(
        result.status.success(),
        "package plan failed: {}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(std::fs::read_to_string(
        directory.path().join("package-plan.txt"),
    )?)
}

fn downloads(plan: &str, package: &str) -> bool {
    plan.lines().any(|line| {
        (line.starts_with("'https://") || line.starts_with("'http://"))
            && line.contains(&format!("/{package}_"))
    })
}

pub fn verify() -> anyhow::Result<usize> {
    let source = std::fs::read_to_string(root().join("deploy/asterisk/Dockerfile"))?;
    let mut caught = 0;
    for (stage, packages) in [
        (
            "build",
            &["libedit-dev", "libnspr4-dev", "libmd-dev", "libsrtp2-1"][..],
        ),
        ("runtime", &["libsrtp2-1", "libspeexdsp1", "libxml2"][..]),
    ] {
        let prepared = plan(&fixture(&source, stage, Mutation::None)?)?;
        for package in packages {
            assert!(
                !downloads(&prepared, package),
                "prefetched package still requires a repository download: {stage}/{package}\n{prepared}"
            );
        }
        for mutation in [Mutation::OmitArchives, Mutation::RefreshAfterArchives] {
            let unprepared = plan(&fixture(&source, stage, mutation)?)?;
            for package in packages {
                assert!(
                    downloads(&unprepared, package),
                    "breaking preparation did not restore the repository download: {stage}/{package}\n{unprepared}"
                );
            }
            caught += 1;
        }
        if stage == "build" {
            let incorrect_epoch = plan(&fixture(&source, stage, Mutation::OmitEpochFilename)?)?;
            for package in packages {
                assert_eq!(
                    downloads(&incorrect_epoch, package),
                    *package == "libnspr4-dev",
                    "incorrect epoch filename: {package}\n{incorrect_epoch}"
                );
            }
            caught += 1;
        }
    }
    Ok(caught)
}
