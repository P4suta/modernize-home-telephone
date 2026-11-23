use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Deserialize)]
struct ReferenceFacts {
    id: String,
    source_line: u32,
    active: bool,
    template_value: Option<String>,
}

pub fn check_references(root: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use sha2::Digest;
    let values: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_slice(bytes)?;
    ensure!(
        values.iter().all(|value| !value.contains_key("alias")),
        "published parameter definitions must omit manufacturer parameter names"
    );
    let facts: Vec<ReferenceFacts> = serde_json::from_slice(bytes)?;
    let ids: BTreeSet<_> = facts.iter().map(|fact| fact.id.as_str()).collect();
    let inventory = ids.iter().map(|id| format!("{id}\n")).collect::<String>();
    ensure!(
        facts.len() == 598
            && ids.len() == facts.len()
            && format!("{:x}", sha2::Sha256::digest(inventory.as_bytes()))
                == "4faa22f73731cc7187f335789cf3f70bb2bb70979b62b6f0ddccd3ff4ca9eb10",
        "parameter definitions are missing, duplicated, or renamed"
    );
    let template = root.join(
        ".agents/skills/grandstream-ht813/references/upstream/ht813-configuration-template.txt",
    );
    if template.try_exists()? {
        let original = parameters(&std::fs::read_to_string(template)?)?;
        ensure!(original.len() == facts.len(), "source inventory differs");
        for fact in facts {
            let parameter = original
                .iter()
                .find(|parameter| parameter.id == fact.id)
                .context("unknown parameter")?;
            ensure!(
                parameter.source_line == fact.source_line
                    && parameter.active == fact.active
                    && fact
                        .template_value
                        .is_none_or(|value| value == parameter.template_value),
                "parameter fact differs from its preserved source"
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub id: String,
    pub active: bool,
    pub source_line: u32,
    pub section: String,
    pub title: String,
    pub domain: String,
    pub template_value: String,
}
pub fn parameters(source: &str) -> anyhow::Result<Vec<Parameter>> {
    ensure!(
        source.len() <= 1_048_576
            && source
                .lines()
                .take(4)
                .any(|line| line.contains("HT813 firmware version 1.0.17.3")),
        "qualify the preserved template edition"
    );
    let mut definitions = vec![];
    let mut ids = BTreeSet::new();
    let mut comments: Vec<String> = vec![];
    let mut previous = String::new();
    let mut section = "advanced";
    for (line_index, line) in source.lines().enumerate() {
        let mut line = line.trim();
        let lower = line.to_ascii_lowercase();
        if lower.contains("fxs port settings") {
            section = "fxs";
        } else if lower.contains("fxo port settings") {
            section = "fxo";
        } else if lower.contains("basic settings") {
            section = "basic";
        } else if lower.contains("advanced settings") {
            section = "advanced";
        }
        let mut active = true;
        if let Some(comment) = line.strip_prefix("#") {
            let comment = comment.trim();
            if comment.split_once('=').is_some_and(|(name, _)| {
                name.trim().strip_prefix('P').is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                })
            }) {
                line = comment;
                active = false;
            } else {
                if !comment.is_empty() && !comment.starts_with('#') && !comment.ends_with('#') {
                    comments.push(comment.to_owned());
                }
                continue;
            }
        }
        if line.is_empty() {
            comments.clear();
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let digits = name.strip_prefix('P').unwrap_or(name);
        ensure!(
            !digits.is_empty()
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && ids.insert(name.to_owned()),
            "invalid or duplicate parameter"
        );
        let title = comments
            .first()
            .cloned()
            .unwrap_or_else(|| previous.clone());
        ensure!(!title.is_empty(), "a parameter has no definition");
        let domain = comments
            .iter()
            .find_map(|comment| {
                comment
                    .strip_prefix("Number:")
                    .or_else(|| comment.strip_prefix("Number :"))
            })
            .unwrap_or("")
            .trim()
            .to_owned();
        previous = title.clone();
        definitions.push(Parameter {
            id: name.into(),
            active,
            source_line: u32::try_from(line_index + 1)?,
            section: section.into(),
            title,
            domain,
            template_value: value.trim().into(),
        });
        comments.clear();
    }
    ensure!(
        !definitions.is_empty() && definitions.len() <= 1024,
        "empty or oversized parameter inventory"
    );
    Ok(definitions)
}
pub fn export(template: &Path, output: &Path) -> anyhow::Result<()> {
    let values = parameters(&std::fs::read_to_string(template)?)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        output,
        format!("{}\n", serde_json::to_string_pretty(&values)?),
    )
    .context("write parameter facts")?;
    Ok(())
}
