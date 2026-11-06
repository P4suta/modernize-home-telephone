use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};
use syn::visit::Visit;

pub fn write_corpora(root: &Path) -> anyhow::Result<()> {
    const SEEDS: &[(&str, &[&[u8]])] = &[
        (
            "dial_path",
            &[
                b"1",
                b"+819012345678",
                b"recordings/call.wav",
                b"../../phone.sqlite",
            ],
        ),
        (
            "state_sequences",
            &[&[0, 1, 2, 3, 4, 5, 6, 7], &[255, 128, 64, 32, 16, 8, 4, 2]],
        ),
    ];
    for (target, seeds) in SEEDS {
        let directory = root.join(target);
        std::fs::create_dir_all(&directory)?;
        for (index, seed) in seeds.iter().enumerate() {
            std::fs::write(directory.join(format!("seed-{index}")), seed)?;
        }
    }
    Ok(())
}

pub fn check_kani(value: &serde_json::Value, names: &[String]) -> anyhow::Result<()> {
    let expected: BTreeSet<_> = names.iter().cloned().collect();
    ensure!(
        !expected.is_empty() && expected.len() == names.len(),
        "proof inventory is empty or duplicated"
    );
    let summary = &value["verification_results"]["summary"];
    ensure!(
        summary["status"] == "completed"
            && summary["failed"] == 0
            && summary["executed"].as_u64() == Some(names.len() as u64)
            && summary["total_harnesses"] == summary["executed"]
            && summary["successful"] == summary["executed"],
        "proof run is incomplete"
    );
    let results = value["verification_results"]["results"]
        .as_array()
        .context("missing proof results")?;
    let mut actual = BTreeSet::new();
    for result in results {
        let id = result["harness_id"]
            .as_str()
            .context("missing harness identity")?;
        ensure!(
            actual.insert(id.to_owned()) && result["status"] == "Success",
            "duplicate or failed proof"
        );
        let checks = result["checks"]
            .as_array()
            .context("missing checked properties")?;
        ensure!(
            checks.iter().any(|check| check["category"] == "assertion"
                && check["status"] == "Success"
                && check["function"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("proofs::"))),
            "proof has no reachable contract assertion"
        );
        ensure!(
            checks
                .iter()
                .any(|check| check["category"] == "cover" && check["status"] == "Satisfied"),
            "proof has no reachable domain witness"
        );
        for check in checks {
            let own = check["function"]
                .as_str()
                .is_some_and(|name| name.starts_with("proofs::"));
            ensure!(
                matches!(
                    check["status"].as_str(),
                    Some("Success" | "Satisfied" | "Unreachable")
                ),
                "proof property is incomplete"
            );
            ensure!(
                !(own && check["status"] == "Unreachable"),
                "a declared contract assertion is unreachable"
            );
            if check["category"] == "cover" {
                ensure!(
                    check["status"] == "Satisfied",
                    "proof witness is unreachable"
                );
            }
        }
    }
    ensure!(
        actual == expected,
        "executed proofs differ from the declared inventory"
    );
    Ok(())
}
struct RuntimeNames {
    ghost: bool,
}
pub fn check_harnesses(source: &str, names: &[String]) -> anyhow::Result<()> {
    struct Assumptions {
        expressions: Vec<syn::Expr>,
        unsupported: bool,
    }
    impl<'ast> Visit<'ast> for Assumptions {
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            if let syn::Expr::Path(path) = call.func.as_ref()
                && path
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "assume")
            {
                if call.args.len() == 1 {
                    self.expressions.push(call.args[0].clone());
                } else {
                    self.unsupported = true;
                }
            }
            syn::visit::visit_expr_call(self, call);
        }
        fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
            let path = attribute.path();
            if path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "kani")
            {
                self.unsupported |= !path.segments.last().is_some_and(|segment| {
                    matches!(segment.ident.to_string().as_str(), "proof" | "unwind")
                });
            }
        }
    }
    let parsed = syn::parse_file(source)?;
    let mut inventory = BTreeSet::new();
    for item in &parsed.items {
        if let syn::Item::Fn(function) = item {
            if function.sig.ident == "negative_control_rejects_lost_ownership" {
                let expected: syn::Block = syn::parse_quote!({
                    let owner = CallId::new(1).unwrap();
                    let line = Line::default().acquire(owner).unwrap();
                    assert!(line.owner().is_none());
                });
                ensure!(
                    *function.block == expected,
                    "negative ownership control changed"
                );
            }
            let mut assumptions = Assumptions {
                expressions: vec![],
                unsupported: false,
            };
            assumptions.visit_item_fn(function);
            let expected: Vec<syn::Expr> = match function.sig.ident.to_string().as_str() {
                "artifact_paths_cannot_escape_their_root" => vec![syn::parse_str(PATH_ASSUMPTION)?],
                "accepted_numbers_cannot_contain_dialplan_syntax" => {
                    vec![syn::parse_str(NUMBER_ASSUMPTION)?]
                }
                _ => vec![],
            };
            ensure!(
                !assumptions.unsupported && assumptions.expressions == expected,
                "proof assumptions or verifier attributes changed"
            );
            if function.attrs.iter().any(|attribute| {
                attribute.path().is_ident("proof")
                    || attribute
                        .path()
                        .segments
                        .last()
                        .is_some_and(|segment| segment.ident == "proof")
            }) && function.sig.ident != "negative_control_rejects_lost_ownership"
            {
                inventory.insert(format!("proofs::{}", function.sig.ident));
            }
        }
    }
    ensure!(
        !inventory.is_empty()
            && inventory.len() == names.len()
            && inventory == names.iter().cloned().collect(),
        "source proof inventory differs from the contract"
    );
    Ok(())
}
impl<'ast> Visit<'ast> for RuntimeNames {
    fn visit_attribute(&mut self, _attribute: &'ast syn::Attribute) {}
    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        self.ghost |= ident == "_proof_position";
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.ghost |= mac
            .tokens
            .to_string()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|token| token == "_proof_position");
    }
}
pub fn check_ghost_erasure(source: &str) -> anyhow::Result<()> {
    struct Decreases {
        valid: bool,
    }
    impl<'ast> Visit<'ast> for Decreases {
        fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
            use quote::ToTokens;
            if attribute
                .to_token_stream()
                .to_string()
                .contains("loop_decreases")
            {
                let expected: syn::Attribute =
                    syn::parse_quote!(#[cfg_attr(kani, kani::loop_decreases(remaining))]);
                self.valid &= *attribute == expected;
            }
        }
    }
    let parsed = syn::parse_file(source)?;
    let mut decreases = Decreases { valid: true };
    decreases.visit_file(&parsed);
    ensure!(
        decreases.valid,
        "Kani requires the directly updated loop variant"
    );
    let mut found = false;
    let mut wrapper = false;
    let expected: syn::Block = syn::parse_quote!({
        if accepts(bytes, 0) {
            Some(Self(bytes))
        } else {
            None
        }
    });
    for item in parsed.items {
        match item {
            syn::Item::Fn(function) if function.sig.ident == "accepts" => {
                let mut names = RuntimeNames { ghost: false };
                names.visit_block(&function.block);
                ensure!(!names.ghost, "proof observation affects runtime behavior");
                found = true;
            }
            syn::Item::Impl(implementation) => {
                for item in implementation.items {
                    if let syn::ImplItem::Fn(method) = item
                        && method.sig.ident == "parse"
                    {
                        ensure!(
                            method.block == expected,
                            "public parser no longer refines the verified scanner"
                        );
                        wrapper = true;
                    }
                }
            }
            _ => {}
        }
    }
    ensure!(found && wrapper, "missing scanner or public refinement");
    Ok(())
}
pub fn check_length_guards(artifact: &str, numbers: &str) -> anyhow::Result<()> {
    let guard: syn::Stmt = syn::parse_str(PATH_LENGTH_GUARD)?;
    let parsed = syn::parse_file(artifact)?;
    ensure!(
        parsed
            .items
            .contains(&syn::parse_str(PATH_LENGTH_DECLARATION)?),
        "artifact byte limit changed"
    );
    ensure!(parsed.items.iter().any(|item|matches!(item,syn::Item::Fn(function) if function.sig.ident=="accepts" && function.block.stmts.first()==Some(&guard))),"artifact length guard no longer refines the bounded scanner proof");
    let guard: syn::Stmt = syn::parse_str(NUMBER_LENGTH_GUARD)?;
    let parsed = syn::parse_file(numbers)?;
    ensure!(
        parsed
            .items
            .contains(&syn::parse_str(NUMBER_LENGTH_DECLARATION)?),
        "number byte limit changed"
    );
    ensure!(parsed.items.iter().any(|item|match item {
        syn::Item::Impl(implementation) if matches!(implementation.self_ty.as_ref(),syn::Type::Path(path) if path.path.segments.last().is_some_and(|segment|segment.ident=="DialNumber"))=>implementation.items.iter().any(|item|matches!(item,syn::ImplItem::Fn(function) if function.sig.ident=="parse" && function.block.stmts.first()==Some(&guard))),
        _=>false,
    }),"number length guard no longer refines the bounded scanner proof");
    Ok(())
}
pub fn contract_sources() -> anyhow::Result<()> {
    let artifact = std::fs::read_to_string("crates/phone-core/src/artifact.rs")?;
    check_ghost_erasure(&artifact)?;
    check_length_guards(
        &artifact,
        &std::fs::read_to_string("crates/phone-core/src/lib.rs")?,
    )?;
    check_harnesses(
        &std::fs::read_to_string("crates/phone-core/src/proofs.rs")?,
        &names()?,
    )
}
#[derive(Clone, Copy, Debug, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Proof,
    Miri,
    Fuzz,
    Mutants,
    WebMutants,
}
impl Check {
    fn name(self) -> &'static str {
        match self {
            Self::Proof => "proof",
            Self::Miri => "miri",
            Self::Fuzz => "fuzz",
            Self::Mutants => "mutants",
            Self::WebMutants => "web_mutants",
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    schema: u32,
    check: Check,
    source: BTreeMap<String, String>,
    recipe: String,
    report: String,
    report_sha256: String,
    artifacts: BTreeMap<String, String>,
}
fn binding(check: Check) -> anyhow::Result<BTreeMap<String, String>> {
    let files = crate::package::source_files(Path::new("."))?;
    Ok(files
        .into_iter()
        .filter(|(name, _)| {
            if matches!(check, Check::WebMutants) {
                name.starts_with("web/")
                    || name == "verification/Dockerfile.web"
                    || name.starts_with("xtask/")
            } else {
                name.starts_with("crates/phone-core/")
                    || name == "Cargo.toml"
                    || name == "Cargo.lock"
                    || name == "verification/Dockerfile"
                    || name.starts_with("xtask/")
                    || name == "verification/contracts.toml"
                    || (matches!(check, Check::Fuzz) && name.starts_with("fuzz/"))
            }
        })
        .collect())
}
fn run_logged(command: &mut Command, path: &Path) -> anyhow::Result<std::process::ExitStatus> {
    let result = command.output()?;
    let mut bytes = result.stdout;
    bytes.extend_from_slice(&result.stderr);
    crate::operations::private_file(path, &bytes)?;
    if !result.status.success() {
        eprintln!(
            "{}",
            String::from_utf8_lossy(&bytes)
                .lines()
                .rev()
                .take(CACHE_KEY_HEX_DIGITS)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    Ok(result.status)
}
#[derive(Deserialize)]
struct Contracts {
    harnesses: Vec<String>,
}
fn names() -> anyhow::Result<Vec<String>> {
    let contracts: Contracts =
        toml::from_str(&std::fs::read_to_string("verification/contracts.toml")?)?;
    Ok(contracts.harnesses)
}
fn recipe(check: Check) -> &'static str {
    if matches!(check, Check::WebMutants) {
        "verification/Dockerfile.web"
    } else {
        "verification/Dockerfile"
    }
}
fn reject_broken_parser(
    run: &Path,
    name: &str,
    original: &str,
    replacement: &str,
    function: &str,
    category: &str,
) -> anyhow::Result<()> {
    let run = run.canonicalize()?;
    let workspace = run.join(name);
    crate::operations::private_directory(&workspace)?;
    let mut root: toml::Value = toml::from_str(&std::fs::read_to_string("Cargo.toml")?)?;
    root["workspace"]["members"] =
        toml::Value::Array(vec![toml::Value::String("crates/phone-core".into())]);
    crate::operations::private_file(
        &workspace.join("Cargo.toml"),
        toml::to_string(&root)?.as_bytes(),
    )?;
    crate::manager::copy_tree(
        Path::new("crates/phone-core"),
        &workspace.join("crates/phone-core"),
    )?;
    let path = workspace.join("crates/phone-core/src/artifact.rs");
    let source = std::fs::read_to_string(&path)?;
    ensure!(
        source.matches(original).count() == 1,
        "negative implementation control no longer matches its source"
    );
    std::fs::write(path, source.replace(original, replacement))?;
    let report = run.join(format!("{name}.json"));
    let status = run_logged(
        Command::new("cargo")
            .args(["kani", "--manifest-path"])
            .arg(workspace.join("Cargo.toml"))
            .args(["-p", "phone-core", "--target-dir"])
            .arg(run.join(format!("{name}-target")))
            .args([
                "--harness",
                "artifact_paths_cannot_escape_their_root",
                "-Z",
                "loop-contracts",
                "-Z",
                "unstable-options",
                "--export-json",
            ])
            .arg(&report)
            .args(["--cbmc-args", "--object-bits", "10"])
            .env("CARGO_INCREMENTAL", "0"),
        &run.join(format!("{name}.log")),
    )?;
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(report)?)?;
    let results = value["verification_results"]["results"]
        .as_array()
        .context("broken implementation produced no proof results")?;
    ensure!(
        !status.success()
            && results.len() == 1
            && results[0]["harness_id"] == "proofs::artifact_paths_cannot_escape_their_root"
            && results[0]["status"] == "Failure"
            && results[0]["checks"]
                .as_array()
                .is_some_and(
                    |checks| checks.iter().any(|check| check["status"] == "Failure"
                        && check["function"] == function
                        && check["category"] == category)
                ),
        "the broken implementation did not fail its declared property"
    );
    Ok(())
}
pub fn inside(check: Check) -> anyhow::Result<()> {
    ensure!(
        !matches!(check, Check::WebMutants),
        "web mutation runs in its Node image"
    );
    let source = binding(check)?;
    let run = PathBuf::from(".cache/assurance/runs").join(uuid::Uuid::new_v4().to_string());
    crate::operations::private_directory(&run)?;
    let target = run.join("target");
    let report = match check {
        Check::Proof => {
            contract_sources()?;
            let report = run.join("kani.json");
            let status = run_logged(
                Command::new("cargo")
                    .args([
                        "kani",
                        "-p",
                        "phone-core",
                        "--target-dir",
                        target.to_str().context("invalid proof target")?,
                        "-Z",
                        "loop-contracts",
                        "-Z",
                        "unstable-options",
                        "--export-json",
                    ])
                    .env("CARGO_INCREMENTAL", "0")
                    .arg(&report)
                    .args(["--cbmc-args", "--object-bits", "10"]),
                &run.join("kani.log"),
            )?;
            ensure!(
                status.success(),
                "formal verification failed or is inconclusive"
            );
            check_kani(
                &serde_json::from_slice(&std::fs::read(&report)?)?,
                &names()?,
            )?;
            let negative = run.join("negative.json");
            let status = run_logged(
                Command::new("cargo")
                    .args([
                        "kani",
                        "-p",
                        "phone-core",
                        "--target-dir",
                        target.to_str().context("invalid proof target")?,
                        "--features",
                        "proof-negative-control",
                        "--harness",
                        "negative_control_rejects_lost_ownership",
                        "-Z",
                        "loop-contracts",
                        "-Z",
                        "unstable-options",
                        "--export-json",
                    ])
                    .env("CARGO_INCREMENTAL", "0")
                    .arg(&negative),
                &run.join("negative.log"),
            )?;
            let value: serde_json::Value = serde_json::from_slice(&std::fs::read(negative)?)?;
            let results = value["verification_results"]["results"]
                .as_array()
                .context("negative control did not produce proof results")?;
            ensure!(
                !status.success()
                    && results.len() == 1
                    && results[0]["status"] == "Failure"
                    && results[0]["checks"]
                        .as_array()
                        .is_some_and(|checks| checks.iter().any(|check| check["status"]
                            == "Failure"
                            && check["category"] == "assertion"
                            && check["function"]
                                == "proofs::negative_control_rejects_lost_ownership")),
                "the negative control did not fail for its intended assertion"
            );
            reject_broken_parser(
                &run,
                "broken-parser",
                "valid &= valid_position!(bytes, index);",
                "valid = true;",
                "artifact::accepts",
                "loop_invariant_step",
            )?;
            reject_broken_parser(
                &run,
                "broken-alphabet",
                "b'a'..=b'z'",
                "b'a'..=b'{'",
                "proofs::artifact_paths_cannot_escape_their_root",
                "assertion",
            )?;
            report
        }
        Check::Miri => {
            let log = run.join("miri.log");
            let status = run_logged(
                Command::new("cargo")
                    .args([
                        "+nightly-2026-10-01",
                        "miri",
                        "test",
                        "--locked",
                        "-p",
                        "phone-core",
                    ])
                    .arg("--target-dir")
                    .arg(&target)
                    .env("CARGO_INCREMENTAL", "0")
                    .env("MIRIFLAGS", "-Zmiri-strict-provenance"),
                &log,
            )?;
            ensure!(status.success(), "Miri failed or is inconclusive");
            log
        }
        Check::Fuzz => {
            for (target, seed) in [
                ("dial_path", FUZZ_SEEDS[0]),
                ("state_sequences", FUZZ_SEEDS[2]),
            ] {
                let corpus = run.join(format!("corpus-{target}"));
                crate::operations::private_directory(&corpus)?;
                let original = Path::new("fuzz/corpus").join(target);
                let mut seeds = std::collections::BTreeMap::new();
                for entry in std::fs::read_dir(original)? {
                    let entry = entry?;
                    ensure!(
                        entry.file_type()?.is_file(),
                        "fuzz seeds must be regular files"
                    );
                    seeds.insert(entry.file_name(), std::fs::read(entry.path())?);
                }
                ensure!(!seeds.is_empty(), "fuzz corpus is empty");
                for (name, bytes) in seeds {
                    crate::operations::private_file(&corpus.join(name), &bytes)?;
                }
                for seed in [seed, seed + 1] {
                    let log = run.join(format!("{target}-{seed}.log"));
                    let status = run_logged(
                        Command::new("cargo")
                            .env("CARGO_INCREMENTAL", "0")
                            .env(
                                "CARGO_TARGET_DIR",
                                std::fs::canonicalize(&run)?.join("fuzz-target"),
                            )
                            .args(["+nightly-2026-10-01", "fuzz", "run", target])
                            .arg(&corpus)
                            .args([
                                "--",
                                "-runs=10000",
                                &format!("-seed={seed}"),
                                "-max_len=512",
                            ]),
                        &log,
                    )?;
                    ensure!(
                        status.success(),
                        "fixed-run fuzzing failed or is inconclusive"
                    );
                }
            }
            let report = run.join("fuzz.json");
            crate::operations::private_file(
                &report,
                &serde_json::to_vec(
                    &serde_json::json!({"runs":FUZZ_EXECUTIONS,"maximum_input_bytes":FUZZ_MAXIMUM_INPUT_BYTES,"seeds":FUZZ_SEEDS,"sanitizer":"address"}),
                )?,
            )?;
            report
        }
        Check::Mutants => {
            const IMAGE_RESULT_MUTANTS: usize = 2;
            let mut command = Command::new("cargo");
            command.env("CARGO_INCREMENTAL", "0");
            command
                .args([
                    "mutants",
                    "--package",
                    "phone-core",
                    "--package",
                    "xtask",
                    "--jobs",
                    "1",
                    "--build-timeout",
                    "300",
                    "--timeout",
                    "20",
                    "--re",
                    r"crates/phone-core/|replace image ->|replace restore_verification_ownership ->|replace verify ->|replace inside ->|replace browser ->",
                    "--output",
                ])
                .arg(&run);
            for entry in std::fs::read_dir("crates/phone-core/src")? {
                let path = entry?.path();
                if path.extension().is_some_and(|extension| extension == "rs")
                    && path.file_name().is_none_or(|name| name != "proofs.rs")
                {
                    command.arg("--file").arg(path);
                }
            }
            command.args([
                "--file",
                "xtask/src/assurance.rs",
                "--file",
                "xtask/src/check.rs",
            ]);
            let status = run_logged(&mut command, &run.join("mutants.log"))?;
            let report = run.join("mutants.out/outcomes.json");
            let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&report)?)?;
            ensure!(
                status.success()
                    && value["total_mutants"]
                        .as_u64()
                        .is_some_and(|count| count > 0)
                    && value["caught"].as_u64().is_some_and(|count| count > 0)
                    && value["missed"] == 0
                    && value["timeout"] == 0
                    && value["caught"]
                        .as_u64()
                        .zip(value["unviable"].as_u64())
                        .is_some_and(|(caught, unviable)| Some(caught + unviable)
                            == value["total_mutants"].as_u64()),
                "mutants survived or the campaign is incomplete"
            );
            let image_results: Vec<_> = value["outcomes"]
                .as_array()
                .context("missing mutation outcomes")?
                .iter()
                .filter(|outcome| {
                    outcome["scenario"]["Mutant"]["file"] == "xtask/src/assurance.rs"
                        && outcome["scenario"]["Mutant"]["function"]["function_name"] == "image"
                })
                .collect();
            ensure!(
                image_results.len() == IMAGE_RESULT_MUTANTS
                    && image_results
                        .iter()
                        .all(|outcome| outcome["summary"] == "CaughtMutant"),
                "verification image result mutations were not caught"
            );
            for (file, function) in [
                ("xtask/src/assurance.rs", "verify"),
                ("xtask/src/assurance.rs", "restore_verification_ownership"),
                ("xtask/src/assurance.rs", "inside"),
                ("xtask/src/check.rs", "browser"),
            ] {
                let results: Vec<_> = value["outcomes"]
                    .as_array()
                    .context("missing mutation outcomes")?
                    .iter()
                    .filter(|outcome| {
                        outcome["scenario"]["Mutant"]["file"] == file
                            && outcome["scenario"]["Mutant"]["function"]["function_name"]
                                == function
                    })
                    .collect();
                ensure!(
                    results.len() == 1 && results[0]["summary"] == "CaughtMutant",
                    "verification adapter mutation was not caught: {function}"
                );
            }
            report
        }
        Check::WebMutants => unreachable!(),
    };
    ensure!(
        source == binding(check)?,
        "sources changed during verification"
    );
    let receipt = Receipt {
        schema: 1,
        check,
        source,
        recipe: crate::bundle::digest(Path::new(recipe(check)))?,
        report: report.to_str().context("invalid report path")?.into(),
        report_sha256: crate::bundle::digest(&report)?,
        artifacts: std::fs::read_dir(&run)?
            .map(|entry| -> anyhow::Result<Option<(String, String)>> {
                let path = entry?.path();
                Ok(if path.is_file() {
                    Some((
                        path.to_str().context("invalid report path")?.into(),
                        crate::bundle::digest(&path)?,
                    ))
                } else {
                    None
                })
            })
            .filter_map(|entry| entry.transpose())
            .collect::<anyhow::Result<_>>()?,
    };
    crate::manager::atomic_json(
        &Path::new(".cache/assurance").join(format!("{}.json", check.name())),
        &receipt,
    )?;
    println!("{} verified; evidence: {}", check.name(), receipt.report);
    Ok(())
}

fn image(check: Check) -> anyhow::Result<String> {
    use sha2::Digest;
    let name = if matches!(check, Check::WebMutants) {
        "homephone-verification-web:local"
    } else {
        "homephone-verification:local"
    };
    let hash = if matches!(check, Check::WebMutants) {
        format!(
            "{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&binding(check)?)?)
        )
    } else {
        crate::bundle::digest(Path::new(recipe(check)))?
    };
    let inspect = Command::new("docker")
        .args(["image", "inspect", name])
        .output()?;
    let current: Vec<serde_json::Value> =
        serde_json::from_slice(&inspect.stdout).unwrap_or_default();
    if !inspect.status.success()
        || current
            .first()
            .is_none_or(|value| value["Config"]["Labels"]["homephone.recipe"] != hash)
    {
        crate::operations::run(
            "docker",
            &[
                "build",
                "--platform",
                if matches!(check, Check::WebMutants) {
                    "linux/arm64"
                } else {
                    "linux/amd64"
                },
                "-f",
                recipe(check),
                "--label",
                &format!("homephone.recipe={hash}"),
                "-t",
                name,
                ".",
            ],
        )?;
    }
    let inspect = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", name])
        .output()?;
    ensure!(
        inspect.status.success(),
        "verification image is unavailable"
    );
    let id = String::from_utf8(inspect.stdout)?.trim().to_owned();
    ensure!(
        id.starts_with("sha256:") && id.len() == IMAGE_ID_LENGTH,
        "verification requires an immutable image identity"
    );
    Ok(id)
}

#[cfg(test)]
#[cfg(unix)]
mod image_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const PRIVATE_EXECUTABLE_MODE: u32 = 0o700;

    fn observe(id: &str, failure: &str) {
        let directory = tempfile::tempdir().unwrap();
        let docker = directory.path().join("docker");
        std::fs::write(
            &docker,
            r#"#!/bin/sh
case "$*" in
  'image inspect homephone-verification:local')
    printf '%s\n' "$HOMEPHONE_IMAGE_INSPECTION" ;;
  'image inspect --format {{.Id}} homephone-verification:local')
    if [ "$HOMEPHONE_IMAGE_FAILURE" = unavailable ]; then exit 1; fi
    printf '%s\n' "$HOMEPHONE_IMAGE_ID" ;;
  *) exit 71 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(
            &docker,
            std::fs::Permissions::from_mode(PRIVATE_EXECUTABLE_MODE),
        )
        .unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let hash = crate::bundle::digest(&root.join(recipe(Check::Proof))).unwrap();
        let mut paths = vec![directory.path().to_owned()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "assurance::image_tests::image_child"])
            .current_dir(root)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HOMEPHONE_IMAGE_ID", id)
            .env("HOMEPHONE_IMAGE_FAILURE", failure)
            .env(
                "HOMEPHONE_IMAGE_INSPECTION",
                serde_json::json!([{"Config":{"Labels":{"homephone.recipe":hash}}}]).to_string(),
            )
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "image contract failed: {:?}; {:?}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn image_child() {
        let Ok(expected) = std::env::var("HOMEPHONE_IMAGE_ID") else {
            return;
        };
        let failure = std::env::var("HOMEPHONE_IMAGE_FAILURE").unwrap();
        let actual = image(Check::Proof);
        if failure.is_empty() {
            assert_eq!(actual.unwrap(), expected);
        } else {
            assert!(
                format!("{:#}", actual.unwrap_err()).contains(&failure),
                "the intended image failure was not preserved"
            );
        }
    }

    #[test]
    pub(super) fn local_images_preserve_the_engine_identity_without_a_registry_reference() {
        for digit in ['a', 'b'] {
            observe(&format!("sha256:{}", digit.to_string().repeat(64)), "");
        }
    }

    #[test]
    pub(super) fn unavailable_and_mutable_image_identities_are_refused() {
        observe("", "unavailable");
        observe("latest", "immutable");
    }
}

#[cfg(test)]
#[cfg(unix)]
#[test]
fn verification_images_preserve_identity_and_refuse_bad_cli_output() {
    image_tests::local_images_preserve_the_engine_identity_without_a_registry_reference();
    image_tests::unavailable_and_mutable_image_identities_are_refused();
}

fn restore_verification_ownership(image: &str, driver_target: &str) -> anyhow::Result<()> {
    fn caller_id(argument: &str) -> anyhow::Result<u32> {
        let output = Command::new("id").arg(argument).output()?;
        ensure!(output.status.success(), "caller identity is unavailable");
        String::from_utf8(output.stdout)?
            .trim()
            .parse()
            .context("caller identity is not an unsigned integer")
    }
    let owner = format!("{}:{}", caller_id("-u")?, caller_id("-g")?);
    crate::operations::run(
        "docker",
        &[
            "run",
            "--rm",
            "--pull",
            "never",
            "--user",
            "0:0",
            "--network",
            "none",
            "--read-only",
            "--cap-drop",
            "ALL",
            "--cap-add",
            "CHOWN",
            "--cap-add",
            "DAC_OVERRIDE",
            "--security-opt",
            "no-new-privileges",
            "--cpus",
            "1",
            "--memory",
            "64m",
            "--memory-swap",
            "64m",
            "--pids-limit",
            "32",
            "--workdir",
            "/work",
            "--mount",
            &format!(
                "type=bind,source={},target=/work",
                std::env::current_dir()?.display()
            ),
            "--entrypoint",
            "timeout",
            image,
            "--signal=TERM",
            "--kill-after=5s",
            "60s",
            "chown",
            "--no-dereference",
            "--recursive",
            &owner,
            ".cache/assurance",
            driver_target,
        ],
    )
    .context("verification output ownership restoration failed")
}

pub fn verify(check: Check) -> anyhow::Result<()> {
    let source = binding(check)?;
    let image = image(check)?;
    let driver_target = format!("target/verification-tools/{}", uuid::Uuid::new_v4());
    crate::operations::private_directory(Path::new(".cache/assurance"))?;
    crate::operations::private_directory(Path::new(&driver_target))?;
    let arguments: Vec<String> = if matches!(check, Check::WebMutants) {
        vec![
            "pnpm".into(),
            "--dir".into(),
            WEB_VERIFICATION_DIRECTORY.into(),
            "mutate".into(),
        ]
    } else {
        vec![
            "env".into(),
            "CARGO_INCREMENTAL=0".into(),
            "cargo".into(),
            "run".into(),
            "--locked".into(),
            "--target-dir".into(),
            driver_target.clone(),
            "--package".into(),
            "xtask".into(),
            "--".into(),
            "verify".into(),
            check.name().replace('_', "-"),
            "--inside".into(),
        ]
    };
    let owner = Command::new("dotfiles")
        .arg("--help")
        .output()
        .is_ok_and(|result| result.status.success());
    let (container, execution_succeeded) = if owner {
        let output = Command::new("dotfiles")
            .args([
                "bounded",
                "run",
                "--keep",
                "--image",
                &image,
                "--project",
                ".",
                "--",
            ])
            .args(&arguments)
            .output()?;
        let mut bytes = output.stdout;
        bytes.extend_from_slice(&output.stderr);
        let text = String::from_utf8_lossy(&bytes);
        print!("{text}");
        let id = text
            .lines()
            .find_map(|line| line.strip_prefix("Bounded container "))
            .and_then(|line| line.split(':').next())
            .context("bounded runner did not identify its container")?
            .to_owned();
        ensure!(
            id.len() == SHA256_HEX_DIGITS && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid owned container identity"
        );
        (id, output.status.success())
    } else {
        let id = format!("homephone-verification-{}", uuid::Uuid::new_v4());
        let status = Command::new("docker")
            .args([
                "run",
                "--pull",
                "never",
                "--name",
                &id,
                "--memory",
                "2g",
                "--memory-swap",
                "2g",
                "--cpus",
                "2",
                "--pids-limit",
                "256",
                "--security-opt",
                "no-new-privileges",
                "--workdir",
                "/work",
                "--mount",
                &format!(
                    "type=bind,source={},target=/work",
                    std::env::current_dir()?.display()
                ),
                &image,
                "timeout",
                "--signal=TERM",
                "--kill-after=5s",
                "900s",
            ])
            .args(&arguments)
            .status()?;
        (id, status.success())
    };
    let ownership = restore_verification_ownership(&image, &driver_target);
    let execution = if execution_succeeded {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "bounded verification failed or is inconclusive"
        ))
    };
    let ready = match (execution, ownership) {
        (Ok(()), ownership) => ownership,
        (Err(primary), Ok(())) => Err(primary),
        (Err(primary), Err(ownership)) => Err(primary.context(format!(
            "verification output ownership restoration failed: {ownership:#}"
        ))),
    };
    let outcome = ready.and_then(|()| {
        ensure!(
            source == binding(check)?,
            "sources changed during the verification process"
        );
        if matches!(check, Check::WebMutants) {
            let report = PathBuf::from(".cache/assurance")
                .join(format!("web-mutants-{}.json", uuid::Uuid::new_v4()));
            crate::operations::private_directory(
                report.parent().context("missing report parent")?,
            )?;
            let status = Command::new("docker")
                .args([
                    "cp",
                    &format!("{container}:/opt/web/reports/mutation/mutation.json"),
                ])
                .arg(&report)
                .status()?;
            ensure!(status.success(), "mutation report could not be collected");
            let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&report)?)?;
            let files = value["files"]
                .as_object()
                .context("missing mutation files")?;
            let mutants: Vec<_> = files
                .values()
                .filter_map(|file| file["mutants"].as_array())
                .flatten()
                .collect();
            ensure!(
                !mutants.is_empty()
                    && mutants.iter().any(|mutant| mutant["status"] == "Killed")
                    && mutants.iter().all(|mutant| matches!(
                        mutant["status"].as_str(),
                        Some("Killed" | "CompileError")
                    )),
                "web mutants survived or were not checked"
            );
            ensure!(
                binding(check)? == source,
                "web sources changed during mutation"
            );
            crate::manager::atomic_json(
                Path::new(".cache/assurance/web_mutants.json"),
                &Receipt {
                    schema: 1,
                    check,
                    source,
                    recipe: crate::bundle::digest(Path::new(recipe(check)))?,
                    report: report.to_str().context("invalid report path")?.into(),
                    report_sha256: crate::bundle::digest(&report)?,
                    artifacts: [(
                        report.to_str().context("invalid report path")?.into(),
                        crate::bundle::digest(&report)?,
                    )]
                    .into(),
                },
            )?;
        }
        Ok(())
    });
    let cleanup = crate::operations::run("docker", &["rm", &container])
        .context("verification container cleanup failed");
    match (outcome, cleanup) {
        (Ok(()), cleanup) => cleanup,
        (Err(primary), Ok(())) => Err(primary),
        (Err(primary), Err(cleanup)) => Err(primary.context(format!(
            "verification container cleanup failed: {cleanup:#}"
        ))),
    }
}
pub fn receipts() -> anyhow::Result<()> {
    for name in ["check", "security", "browser"] {
        crate::check::evidence(name)?;
    }
    crate::manager::deployment_evidence()?;
    for check in [
        Check::Proof,
        Check::Miri,
        Check::Fuzz,
        Check::Mutants,
        Check::WebMutants,
    ] {
        let receipt: Receipt = serde_json::from_slice(&std::fs::read(format!(
            ".cache/assurance/{}.json",
            check.name()
        ))?)?;
        ensure!(
            receipt.schema == 1
                && receipt.check.name() == check.name()
                && receipt.source == binding(check)?
                && receipt.recipe == crate::bundle::digest(Path::new(recipe(check)))?
                && receipt.report_sha256 == crate::bundle::digest(Path::new(&receipt.report))?,
            "missing or stale {} evidence",
            check.name()
        );
        ensure!(
            !receipt.artifacts.is_empty(),
            "evidence has no checked artifacts"
        );
        for (path, hash) in &receipt.artifacts {
            ensure!(
                crate::bundle::digest(Path::new(path))? == *hash,
                "verification artifact changed: {path}"
            );
        }
    }
    println!(
        "Verified current-source evidence: fast checks, security, browser/SIPp, deployment recovery, {} Kani harnesses with three negative controls, Miri, 40000 fuzz executions, Rust mutation testing, and TypeScript mutation testing.",
        names()?.len()
    );
    Ok(())
}
const PATH_ASSUMPTION: &str = "length <= 257";
const NUMBER_ASSUMPTION: &str = "length <= 33";
const PATH_LENGTH_DECLARATION: &str = "pub const MAX_RELATIVE_PATH_BYTES: usize = 256;";
const NUMBER_LENGTH_DECLARATION: &str = "pub const MAX_DIAL_NUMBER_BYTES: usize = 32;";
const PATH_LENGTH_GUARD: &str =
    "if !crate::bytes::nonempty_within(bytes.len(), MAX_RELATIVE_PATH_BYTES) { return false; }";
const NUMBER_LENGTH_GUARD: &str = "if !bytes::nonempty_within(bytes.len(), MAX_DIAL_NUMBER_BYTES) { return Err(Refusal::InvalidNumber); }";
const CACHE_KEY_HEX_DIGITS: usize = 24;
const IMAGE_ID_LENGTH: usize = 71;
const SHA256_HEX_DIGITS: usize = 64;
const WEB_VERIFICATION_DIRECTORY: &str = "/opt/web";
const FUZZ_SEEDS: [u64; 4] = [8008, 8009, 4801, 4802];
const FUZZ_EXECUTIONS: u64 = 40000;
const FUZZ_MAXIMUM_INPUT_BYTES: usize = 512;
