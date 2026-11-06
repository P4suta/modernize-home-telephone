use anyhow::{Context, ensure};
use serde::Deserialize;
use std::{collections::BTreeSet, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    source: String,
    test: String,
    invariant: String,
    previous_gap: String,
    enforcement: String,
}
fn tests(source: &str) -> anyhow::Result<BTreeSet<String>> {
    Ok(syn::parse_file(source)?
        .items
        .into_iter()
        .filter_map(|item| {
            let syn::Item::Fn(function) = item else {
                return None;
            };
            function
                .attrs
                .iter()
                .any(|attribute| {
                    attribute
                        .path()
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "test")
                })
                .then(|| function.sig.ident.to_string())
        })
        .collect())
}
pub fn check(root: &Path, catalog: &str) -> anyhow::Result<()> {
    let catalog: Catalog = toml::from_str(catalog)?;
    ensure!(!catalog.cases.is_empty(), "fault inventory cannot be empty");
    let mut recorded = BTreeSet::new();
    for case in catalog.cases {
        ensure!(
            phone_core::artifact::RelativePath::parse(case.source.as_bytes()).is_some(),
            "invalid fault source path"
        );
        ensure!(
            [
                &case.test,
                &case.invariant,
                &case.previous_gap,
                &case.enforcement
            ]
            .iter()
            .all(|value| !value.trim().is_empty()),
            "fault evidence must name the invariant, previous gap, and enforcement"
        );
        ensure!(
            tests(&std::fs::read_to_string(root.join(&case.source))?)?.contains(&case.test),
            "fault record refers to a missing test"
        );
        ensure!(
            recorded.insert((case.source, case.test)),
            "duplicate fault evidence"
        );
    }
    for directory in ["crates/phone-server/tests", "xtask/tests"] {
        let directory = root.join(directory);
        if !directory.try_exists()? {
            continue;
        }
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with("_faults.rs"))
            {
                continue;
            }
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("invalid fault test path")?
                .replace('\\', "/");
            for test in tests(&std::fs::read_to_string(&path)?)? {
                ensure!(
                    recorded.contains(&(relative.clone(), test.clone())),
                    "fault test lacks a cause and enforcement record: {relative}::{test}"
                );
            }
        }
    }
    Ok(())
}
