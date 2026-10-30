use crate::checks::Checks;
use crate::obligations::{self, KaniFlag, Obligation};
use crate::proof_budget_rules;
use crate::proof_budget_rules::ProofCampaign;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub const KANI_VERSION: &str = "0.68.0";
pub(crate) const KANI_CBMC_VERSION: &str = "6.11.0";
const KANI_OBJECT_BITS: u32 = 12;
const KANI_PARALLELISM: u16 = 1;

/// Every proof covers a reachable acceptance and a reachable refusal.
const WITNESSES: usize = 2;
const OVERFLOW_PROBE: &str = "reject_an_overflow_under_checked_mir";
const CHECKED_ADD_OVERFLOW: &str = "attempt to add with overflow";
/// The most harnesses sharing one bounded report.
pub const KANI_BATCH_SIZE: usize = 16;
const KANI_BATCH_TEST: &str =
    "a_kani_batch_verifies_every_harness_and_rejects_every_counterexample";
const KANI_SUBJECT_TESTS: [&str; 3] = [
    "skill_proofs::tests::notify_chain_decisions_and_their_counterexamples_verify_against_production",
    "skill_proofs::tests::freshness_report_decisions_and_their_counterexamples_verify_against_production",
    "skill_proofs::tests::shape_decisions_and_their_counterexamples_verify_against_production",
];
const MUTATION_CAMPAIGN_CASES: usize = 2;

/// Run native checks and the family's required campaign or measurement budgets.
///
/// # Errors
///
/// Returns all failed checks after every selected check finishes.
pub fn verify_push(
    family: rules::hosts_rules::Family,
    native: impl FnOnce() -> Result<()>,
    proofs: impl FnOnce() -> Result<()>,
    benchmarks: impl FnOnce() -> Result<()>,
) -> Result<()> {
    use rules::hosts_rules::{Verification, verification};
    let mut checks = Checks::default();
    checks.run("check", native);
    match verification(family) {
        Verification::Native => {}
        Verification::Proofs => checks.run("proof campaign", proofs),
        Verification::Benchmarks => checks.run("fixed Linux benchmarks", benchmarks),
    }
    checks.finish()
}

/// Compares completed Kani results with the expected harnesses and properties.
///
/// # Errors
///
/// Returns an error when evidence is incomplete or empty, the harness set differs from `expected`, or a completed result contradicts `counterexample`.
/// A normal run must pass every proof with an assertion, two satisfied cover witnesses, and every property verified, and a counterexample run must fail every harness on an assertion.
pub fn validate_results(value: &Value, expected: &[&str], counterexample: bool) -> Result<()> {
    validate_results_reported(value, expected, counterexample, |result, verdict| {
        let outcome = match verdict {
            proof_budget_rules::Verdict::Proven => "proved",
            proof_budget_rules::Verdict::Failed => "refuted",
            proof_budget_rules::Verdict::Inconclusive => "incomplete",
        };
        println!("Kani {}: {outcome}", result["harness_id"]);
        if let Some(millis) = result["duration_ms"].as_u64() {
            println!("Kani {}: {millis} ms (advisory)", result["harness_id"]);
        }
    })
}

fn validate_results_reported(
    value: &Value,
    expected: &[&str],
    counterexample: bool,
    mut report: impl FnMut(&Value, proof_budget_rules::Verdict),
) -> Result<()> {
    let summary = &value["verification_results"]["summary"];
    let complete = summary["status"] == "completed"
        && summary["executed"].as_u64() == Some(expected.len() as u64)
        && !expected.is_empty();
    if !complete {
        report_incomplete(expected);
    }
    ensure!(
        complete,
        "proof did not complete: incomplete or empty execution for [{}] (missing evidence, inconclusive)",
        expected.join(", ")
    );
    let results = value["verification_results"]["results"]
        .as_array()
        .context("missing proof results")?;
    let names: BTreeSet<_> = results
        .iter()
        .map(|result| {
            result["harness_id"]
                .as_str()
                .context("missing harness identity")
        })
        .collect::<Result<_>>()?;
    ensure!(
        results.len() == expected.len() && names == expected.iter().copied().collect(),
        "unexpected, missing, or duplicate proof harness"
    );
    let mut checks = Checks::default();
    if results
        .iter()
        .all(|result| completed_verdict(result) != proof_budget_rules::Verdict::Inconclusive)
    {
        if counterexample {
            checks.run("counterexample summary", || {
                ensure!(
                    summary["failed"] == expected.len(),
                    "counterexample was not rejected"
                );
                Ok(())
            });
        } else {
            checks.run("production summary", || {
                ensure!(
                    summary["failed"] == 0 && summary["successful"] == expected.len(),
                    "required proof failed"
                );
                Ok(())
            });
        }
    }
    for result in results {
        let name = result["harness_id"].as_str().unwrap_or_default();
        checks.run(&format!("harness {name}"), || {
            validate_reported_result(result, counterexample, &mut report)
        });
    }
    checks.finish()
}

fn report_incomplete(expected: &[&str]) {
    for name in expected {
        println!("Kani {name}: incomplete (batch not validated)");
    }
}

fn completed_verdict(result: &Value) -> proof_budget_rules::Verdict {
    let properties = result["checks"].as_array().filter(|checks| {
        !checks.is_empty()
            && checks.iter().all(|check| {
                matches!(
                    check["status"].as_str(),
                    Some("Success" | "Failure" | "Unreachable" | "Satisfied" | "Unsatisfiable")
                )
            })
    });
    let completed = match (result["status"].as_str(), properties) {
        (Some("Success"), Some(_)) => Some(true),
        (Some("Failure"), Some(checks))
            if checks.iter().any(|check| check["status"] == "Failure") =>
        {
            Some(false)
        }
        _ => None,
    };
    proof_budget_rules::verdict(completed)
}

fn validate_reported_result(
    result: &Value,
    counterexample: bool,
    report: &mut impl FnMut(&Value, proof_budget_rules::Verdict),
) -> Result<()> {
    let verdict = completed_verdict(result);
    let validation = (|| {
        ensure!(
            verdict != proof_budget_rules::Verdict::Inconclusive,
            "proof did not complete: {} (missing evidence, inconclusive)",
            result["harness_id"]
        );
        if counterexample {
            validate_counterexample(result)
        } else {
            validate_proof(result)
        }
    })();
    let outcome = if validation.is_ok() {
        proof_budget_rules::verdict(Some(!counterexample))
    } else if verdict == proof_budget_rules::Verdict::Failed
        && validate_counterexample(result).is_ok()
    {
        proof_budget_rules::Verdict::Failed
    } else {
        proof_budget_rules::Verdict::Inconclusive
    };
    report(result, outcome);
    validation
}

fn validate_counterexample(result: &Value) -> Result<()> {
    ensure!(
        result["checks"].as_array().is_some_and(|checks| checks
            .iter()
            .any(|check| check["category"] == "assertion" && check["status"] == "Failure")),
        "expected assertion counterexample is missing"
    );
    if result["harness_id"]
        .as_str()
        .is_some_and(|name| name.rsplit("::").next() == Some(OVERFLOW_PROBE))
    {
        ensure!(
            result["checks"]
                .as_array()
                .is_some_and(|checks| checks.iter().any(|check| {
                    check["category"] == "assertion"
                        && check["description"] == CHECKED_ADD_OVERFLOW
                        && check["status"] == "Failure"
                })),
            "checked MIR did not reject arithmetic overflow"
        );
    }
    Ok(())
}

fn validate_proof(result: &Value) -> Result<()> {
    ensure!(
        result["status"] == "Success",
        "proof was unsupported, skipped, or failed"
    );
    let checks = result["checks"]
        .as_array()
        .context("missing proof properties")?;
    ensure!(
        checks
            .iter()
            .any(|check| check["category"] == "assertion" && check["status"] == "Success"),
        "proof has no successful reachable assertion"
    );
    ensure!(
        checks
            .iter()
            .filter(|check| check["category"] == "cover" && check["status"] == "Satisfied")
            .count()
            >= WITNESSES,
        "proof lacks reachable acceptance and refusal witnesses"
    );
    let unverified: Vec<String> = checks
        .iter()
        .filter(|check| {
            use proof_budget_rules::Check;
            let status = match check["status"].as_str() {
                Some("Success") => Check::Proven,
                Some("Unreachable") => Check::Impossible,
                Some("Satisfied") => Check::Witness,
                _ => Check::Unverified,
            };
            !proof_budget_rules::verified(check["category"] == "cover", status)
        })
        .map(|check| {
            format!(
                "{} {}: {}",
                check["category"], check["status"], check["description"]
            )
        })
        .collect();
    ensure!(
        unverified.is_empty(),
        "proof property is unverified:\n{}",
        unverified.join("\n")
    );
    Ok(())
}

/// The unstable features a Kani run enables: stubbing always, and the ones an obligation adds.
fn features(flags: &BTreeSet<&'static str>) -> Vec<&'static str> {
    flags.iter().flat_map(|flag| ["-Z", flag]).collect()
}

fn standalone_kani() -> Result<std::process::Command> {
    installed_kani_command()
}

fn checked_kani(mut command: std::process::Command) -> std::process::Command {
    // Inlining and jump threading duplicate cover intrinsics along unsatisfiable paths.
    const CHECKED_MIR: &str = "--edition=2024 -C overflow-checks=yes -C debug-assertions=yes -Z mir-opt-level=3 -Z mir-enable-passes=-Inline,-JumpThreading -Zcrate-attr=feature(stmt_expr_attributes,proc_macro_hygiene)";
    let mut flags = std::env::var_os("RUSTFLAGS").unwrap_or_default();
    if !flags.is_empty() {
        flags.push(" ");
    }
    flags.push(CHECKED_MIR);
    command.env("RUSTFLAGS", flags);
    command
}

/// Verifies every named harness in bounded compilation batches and validates its individual result.
///
/// # Errors
///
/// Refuses an empty set, an incomplete inventory, an unexpected exit, or an unverified property.
pub fn verify_file(
    source: &Path,
    directory: &Path,
    expected: &[&str],
    counterexample: bool,
    flags: &BTreeSet<&'static str>,
) -> Result<()> {
    verify_file_recorded(source, directory, expected, counterexample, flags).map(|_| ())
}

fn verify_file_recorded(
    source: &Path,
    directory: &Path,
    expected: &[&str],
    counterexample: bool,
    flags: &BTreeSet<&'static str>,
) -> Result<Vec<crate::phase_evidence::KaniBatch>> {
    ensure!(
        !expected.is_empty(),
        "proof execution is incomplete or empty"
    );
    ensure!(
        expected.iter().copied().collect::<BTreeSet<_>>().len() == expected.len(),
        "duplicate proof harness"
    );
    let mut checks = Checks::default();
    let mut reports = Vec::new();
    for batch in expected.chunks(KANI_BATCH_SIZE) {
        checks.run(
            &format!("{} [{}]", source.display(), batch.join(", ")),
            || {
                let document = verify_batch(source, directory, batch, counterexample, flags)?;
                reports.push(crate::phase_evidence::KaniBatch {
                    names: batch.iter().map(|name| (*name).to_owned()).collect(),
                    counterexample,
                    document,
                });
                Ok(())
            },
        );
    }
    checks.finish()?;
    Ok(reports)
}

fn verify_batch(
    source: &Path,
    directory: &Path,
    expected: &[&str],
    counterexample: bool,
    flags: &BTreeSet<&'static str>,
) -> Result<Value> {
    verify_batch_command(
        standalone_kani()?,
        source,
        directory,
        expected,
        counterexample,
        flags,
    )
}

fn verify_batch_command(
    mut command: std::process::Command,
    source: &Path,
    directory: &Path,
    expected: &[&str],
    counterexample: bool,
    flags: &BTreeSet<&'static str>,
) -> Result<Value> {
    let output = directory.join("results.json");
    fs::write(&output, "")?;
    command
        .current_dir(directory)
        .arg(source)
        .arg("--target-dir")
        .arg(directory)
        .args(["--output-format", "terse", "-Z", "unstable-options"])
        .args(features(flags))
        .arg("--exact")
        .args(["--jobs", &KANI_PARALLELISM.to_string()])
        .arg("--harness-timeout")
        .arg(format!("{}s", proof_budget_rules::TIMEOUT_SECONDS))
        .arg("--export-json")
        .arg(&output);
    for name in expected {
        command.args(["--harness", name]);
    }
    command
        .args(["--cbmc-args", "--object-bits"])
        .arg(KANI_OBJECT_BITS.to_string());
    let output = crate::phase_evidence::output(&mut command, "Kani batch")?;
    let status = output.status;
    crate::phase_evidence::retain(
        "Kani batch",
        "receipt",
        &fs::read(directory.join("results.json"))?,
    )?;
    let result: Value = skill_ops::read_json(&directory.join("results.json")).with_context(|| {
        report_incomplete(expected);
        format!(
            "proof did not complete for [{}]: missing Kani report (missing evidence, inconclusive)",
            expected.join(", ")
        )
    })?;
    validate_results(&result, expected, counterexample)?;
    ensure!(
        status.success() != counterexample,
        "Kani returned an unexpected outcome: {status}"
    );
    Ok(result)
}

const KANI_TOOL: &str = "cargo:kani-verifier";
const KANI_HOME: &str = "KANI_HOME";
const KANI_DIRECTORY: &str = ".kani";
const KANI_DRIVER: &str = "bin/kani-driver";
const KANI_TOOLCHAIN: &str = "rust-toolchain-version";
const KANI_VALID_SMOKE: &str = "#[kani::proof] fn valid() { let value: bool = kani::any(); assert_eq!(value, !(!value)); kani::cover!(value); kani::cover!(!value); }\n";
const KANI_REJECT_SMOKE: &str =
    "#[kani::proof] fn rejected() { let value: bool = kani::any(); assert!(value); }\n";

/// Returns the existing installed verifier driver without preparing or installing it.
///
/// # Errors
///
/// Refuses an absent or incomplete pinned Kani distribution.
pub fn kani_environment_driver() -> Result<std::path::PathBuf> {
    Ok(installed_kani_directory()?.join(KANI_DRIVER))
}

/// Prepares Kani natively in CI or checks the prepared owner-side image.
///
/// # Errors
///
/// Refuses unsupported pins, failed explicit installation or setup, and an unavailable prepared verifier.
pub fn prepare_kani_environment(root: &Path) -> Result<()> {
    if cfg!(windows) || dotfiles::bounded::requires_container() {
        ensure_linux_engine()?;
        return require_image(&proof_image(root)?);
    }
    if kani_environment_version(root).is_ok() {
        return Ok(());
    }
    let config: toml::Value = toml::from_str(&fs::read_to_string(root.join(MISE_CONFIG))?)?;
    ensure!(
        config["tools"][KANI_TOOL]["version"].as_str() == Some(KANI_VERSION),
        "Kani installation pin differs from the verified driver"
    );
    let mut installation = Tool::Mise.command();
    installation.current_dir(root).args(["install", KANI_TOOL]);
    let status = platform::process::mise_install(installation)?.status()?;
    ensure!(
        status.success(),
        "install the pinned Kani launcher: {status}"
    );
    let status = Tool::Mise
        .command()
        .current_dir(root)
        .args([
            "x",
            &format!("{KANI_TOOL}@{KANI_VERSION}"),
            "--",
            "kani",
            "setup",
        ])
        .status()?;
    ensure!(
        status.success(),
        "prepare the pinned Kani distribution: {status}"
    );
    kani_environment_version(root)?;
    Ok(())
}

fn installed_kani_directory() -> Result<std::path::PathBuf> {
    let home = std::env::var_os(KANI_HOME)
        .map(std::path::PathBuf::from)
        .map_or_else(
            || {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(KANI_DIRECTORY))
                    .context("Kani native home")
            },
            Ok,
        )?;
    ensure!(home.is_absolute(), "Kani home must be absolute");
    let directory = home.join(format!("kani-{KANI_VERSION}"));
    ensure!(
        directory.join(KANI_DRIVER).is_file() && directory.join(KANI_TOOLCHAIN).is_file(),
        "the existing Kani distribution is incomplete; run its explicit preparation command"
    );
    Ok(directory)
}

/// Reads the existing driver's version without invoking Kani's proxy that installs tools.
///
/// # Errors
///
/// Refuses a missing distribution, an unsuccessful version query, or another installed release.
pub fn kani_environment_version(root: &Path) -> Result<String> {
    ensure!(
        !cfg!(windows),
        "Windows Kani metadata uses the existing Linux image route"
    );
    let output = crate::phase_evidence::output(
        installed_kani_command()?.current_dir(root).arg("--version"),
        "existing Kani driver version",
    )?;
    let version = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        output.status.success() && kani_environment_version_matches(&version),
        "the existing Kani driver has another version or cannot run"
    );
    Ok(version)
}

/// Validates the pinned standalone driver's complete version response.
#[must_use]
pub fn kani_environment_version_matches(version: &str) -> bool {
    version.trim()
        == format!("Kani Rust Verifier {KANI_VERSION} (standalone)\nCBMC {KANI_CBMC_VERSION}")
}

pub(crate) fn container_kani_version_arguments() -> [String; 3] {
    [
        "sh".to_owned(),
        "-ec".to_owned(),
        format!(
            r#"directory="${{KANI_HOME:-$HOME/{KANI_DIRECTORY}}}/kani-{KANI_VERSION}"; export PATH="$directory/bin:$directory/pyroot/bin:$PATH"; exec "$directory/{KANI_DRIVER}" --version"#
        ),
    ]
}

fn installed_kani_command() -> Result<std::process::Command> {
    ensure!(
        !dotfiles::bounded::requires_container(),
        "native Kani refuses on owner machines; use `just proofs` or `dotfiles bounded run`"
    );
    let directory = installed_kani_directory()?;
    let mut command = checked_kani(platform::process::command(directory.join(KANI_DRIVER)));
    let mut path = vec![directory.join("bin"), directory.join("pyroot/bin")];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut python = vec![directory.join("pyroot")];
    python.extend(std::env::split_paths(
        &std::env::var_os("PYTHONPATH").unwrap_or_default(),
    ));
    command
        .env("PATH", std::env::join_paths(path)?)
        .env("PYTHONPATH", std::env::join_paths(python)?)
        .env(
            "RUSTUP_TOOLCHAIN",
            fs::read_to_string(directory.join(KANI_TOOLCHAIN))?.trim(),
        );
    for variable in ["LD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH"] {
        if let Some(paths) = std::env::var_os(variable) {
            let paths = std::env::split_paths(&paths).filter(|path| {
                !path
                    .components()
                    .any(|component| component.as_os_str() == "toolchains")
            });
            command.env(variable, std::env::join_paths(paths)?);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.arg0("kani");
    }
    Ok(command)
}

/// Runs one reachable valid proof and one assertion-defect control with the prepared verifier.
///
/// # Errors
///
/// Refuses missing prepared assets, incomplete receipts, an unverified proof, or a missing assertion rejection.
pub fn smoke_kani_environment(root: &Path) -> Result<()> {
    let scope = tempfile::tempdir()?;
    let flags = BTreeSet::from(["stubbing"]);
    if cfg!(windows) || dotfiles::bounded::requires_container() {
        return smoke_kani_container(root, scope.path(), &flags);
    }
    kani_environment_version(root)?;
    for (name, body, counterexample) in [
        ("valid", KANI_VALID_SMOKE, false),
        ("rejected", KANI_REJECT_SMOKE, true),
    ] {
        let source = scope.path().join(format!("{name}.rs"));
        fs::write(&source, body)?;
        verify_batch_command(
            installed_kani_command()?,
            &source,
            scope.path(),
            &[name],
            counterexample,
            &flags,
        )?;
    }
    Ok(())
}

fn smoke_kani_container(
    root: &Path,
    directory: &Path,
    flags: &BTreeSet<&'static str>,
) -> Result<()> {
    ensure_linux_engine()?;
    let image = proof_image(root)?;
    ensure!(
        crate::container::amd64_image_present(&image)?,
        "the existing Kani smoke image is unavailable; run its explicit preparation command"
    );
    let image = verified_image_id(&image)?;
    for (name, body, counterexample) in [
        ("valid", KANI_VALID_SMOKE, false),
        ("rejected", KANI_REJECT_SMOKE, true),
    ] {
        let source = directory.join(format!("{name}.rs"));
        fs::write(&source, body)?;
        fs::write(directory.join("results.json"), [])?;
        let mount = format!(
            "type=bind,\"source={}\",target=/work",
            crate::canonical(directory)?.display()
        );
        let mut command = Tool::Docker.command();
        command
            .args([
                "run",
                "--rm",
                "--pull",
                "never",
                "--network",
                "none",
                "--platform",
                crate::container::LINUX_AMD64,
                "--mount",
                &mount,
                "--workdir",
                "/work",
                &image,
                "kani",
            ])
            .arg(format!("{name}.rs"))
            .args(["--output-format", "terse", "-Z", "unstable-options"])
            .args(features(flags))
            .args([
                "--exact",
                "--harness",
                name,
                "--harness-timeout",
                &format!("{}s", proof_budget_rules::TIMEOUT_SECONDS),
                "--export-json",
                "results.json",
            ]);
        let status = crate::phase_evidence::output(&mut command, "bounded Kani smoke")?.status;
        ensure!(
            status.success() != counterexample,
            "Kani smoke returned another outcome: {status}"
        );
        validate_results(
            &skill_ops::read_json(&directory.join("results.json"))?,
            &[name],
            counterexample,
        )?;
    }
    Ok(())
}

/// Collects the same compiler harness metadata as Kani list while directing generated artifacts outside protected Source.
fn verify_inventory(
    source: &Path,
    directory: &Path,
    flags: &BTreeSet<&'static str>,
) -> Result<BTreeSet<String>> {
    let metadata = directory
        .join(source.file_stem().context("Kani inventory crate name")?)
        .with_extension("kani-metadata.json");
    fs::write(&metadata, "")?;
    let output = crate::phase_evidence::output(
        standalone_kani()?
            .current_dir(directory)
            .arg(source)
            .arg("--target-dir")
            .arg(directory)
            .arg("--only-codegen")
            .arg("--keep-temps")
            .args(features(flags)),
        "Kani compiler harness inventory",
    )?;
    ensure!(
        output.status.success(),
        "cannot compile required proof harness inventory"
    );
    let bytes = fs::read(&metadata)?;
    crate::phase_evidence::retain("Kani compiler harness inventory", "inventory", &bytes)?;
    compiled_harnesses(&bytes)
}

#[derive(serde::Deserialize)]
struct CompiledInventory {
    proof_harnesses: Vec<CompiledHarness>,
}

#[derive(serde::Deserialize)]
struct CompiledHarness {
    pretty_name: String,
    attributes: CompiledAttributes,
}

#[derive(serde::Deserialize)]
struct CompiledAttributes {
    kind: CompiledKind,
}

#[derive(serde::Deserialize)]
enum CompiledKind {
    Proof,
    ProofForContract { target_fn: String },
    Test,
}

fn compiled_harnesses(bytes: &[u8]) -> Result<BTreeSet<String>> {
    let inventory: CompiledInventory = serde_json::from_slice(bytes)?;
    let mut names = BTreeSet::new();
    for harness in inventory.proof_harnesses {
        match harness.attributes.kind {
            CompiledKind::Test => continue,
            CompiledKind::Proof => {}
            CompiledKind::ProofForContract { target_fn } => {
                ensure!(!target_fn.is_empty(), "empty Kani contract target");
            }
        }
        ensure!(
            !harness.pretty_name.is_empty() && names.insert(harness.pretty_name),
            "empty or duplicate Kani compiler harness identity"
        );
    }
    ensure!(
        !names.is_empty(),
        "Kani compiler metadata holds no proof harness"
    );
    Ok(names)
}

/// The digest bytes an image or volume name carries.
const SHORT_DIGEST_BYTES: usize = 8;

fn short_digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes)[..SHORT_DIGEST_BYTES])
}

#[must_use]
pub fn container_image(definition: &[u8]) -> String {
    format!("dotfiles-proofs:{}", short_digest(definition))
}

/// The definition of the Linux verification image.
pub const PROOF_IMAGE: &str = "xtask/proofs/Dockerfile";
const MISE_CONFIG: &str = "mise.toml";
const MISE_LOCK: &str = "mise.lock";
const POWERSHELL_PLATFORM: &str = "platforms.linux-x64";
const SHA256_HEX_DIGITS: usize = 64;
const MISE_RELEASES: &str = "home/.chezmoidata/versions.toml";
const LINUX_ASSET: &str = "linux-x64";

#[derive(Debug)]
struct ImagePins {
    powershell: String,
    powershell_sha256: String,
    mise: String,
    mise_sha256: String,
}

impl ImagePins {
    fn load(root: &Path) -> Result<Self> {
        let config: toml::Value = toml::from_str(&fs::read_to_string(root.join(MISE_CONFIG))?)?;
        let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join(MISE_LOCK))?)?;
        let pin = lock
            .get("tools")
            .and_then(|tools| tools.get("powershell"))
            .and_then(toml::Value::as_array)
            .and_then(|entries| entries.first())
            .context("PowerShell has no locked release")?;
        let version = pin
            .get("version")
            .and_then(toml::Value::as_str)
            .context("PowerShell has no version")?;
        ensure!(
            config
                .get("tools")
                .and_then(|tools| tools.get("powershell"))
                .and_then(toml::Value::as_str)
                == Some(version),
            "PowerShell configuration and lockfile versions differ"
        );
        let digest = pin
            .get(POWERSHELL_PLATFORM)
            .and_then(|platform| platform.get("checksum"))
            .and_then(toml::Value::as_str)
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .context("PowerShell has no Linux SHA-256")?;
        ensure!(
            digest.len() == SHA256_HEX_DIGITS
                && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "PowerShell has an invalid Linux SHA-256"
        );
        let releases: model::data::Data =
            toml::from_str(&fs::read_to_string(root.join(MISE_RELEASES))?)?;
        let mise = releases.mise.context("mise has no release pin")?;
        let mise_sha256 = mise
            .sha256
            .and_then(|mut hashes| hashes.remove(LINUX_ASSET))
            .context("mise has no Linux SHA-256")?;
        ensure!(
            mise_sha256.len() == SHA256_HEX_DIGITS
                && mise_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "mise has an invalid Linux SHA-256"
        );
        Ok(Self {
            powershell: version.to_owned(),
            powershell_sha256: digest.to_owned(),
            mise: mise.version.context("mise has no version")?,
            mise_sha256,
        })
    }

    fn arguments(&self) -> [String; 6] {
        [
            format!("MISE_VERSION={}", self.mise),
            format!("MISE_SHA256={}", self.mise_sha256),
            format!("POWERSHELL_VERSION={}", self.powershell),
            format!("POWERSHELL_SHA256={}", self.powershell_sha256),
            format!("ANALYZER_VERSION={}", platform::script_analyzer::VERSION),
            format!(
                "ANALYZER_SHA256={}",
                platform::script_analyzer::ASSEMBLY_SHA256
            ),
        ]
    }
}

/// The same checked release arguments used by the current image identity.
///
/// # Errors
/// Refuses missing, malformed, mismatched, or unlocked image pins.
pub(crate) fn proof_image_arguments(root: &Path) -> Result<[String; 6]> {
    Ok(ImagePins::load(root)?.arguments())
}

/// The image identity covers its definition and every release pin its build uses.
///
/// # Errors
///
/// Returns an error when reading a pin or the image definition fails.
pub fn proof_image(root: &Path) -> Result<String> {
    let mut definition = fs::read(root.join(PROOF_IMAGE))?;
    for argument in ImagePins::load(root)?.arguments() {
        definition.extend_from_slice(argument.as_bytes());
        definition.push(b'\n');
    }
    Ok(container_image(&definition))
}

/// Requires an explicitly prepared Linux image.
///
/// # Errors
///
/// Refuses an absent or incompatible image and a failed Docker command.
pub fn require_image(image: &str) -> Result<()> {
    ensure!(
        crate::container::amd64_image_present(image)?,
        "the pinned linux/amd64 image {image} is unavailable; run `just --command cargo xtask verification-env prepare-image --plan` before explicit preparation"
    );
    Ok(())
}

#[must_use]
pub fn container_target_volume(root: &Path) -> String {
    format!(
        "dotfiles-proofs-target-{}",
        short_digest(root.as_os_str().as_encoded_bytes())
    )
}

/// Kani writes temporary files beside the source it compiles, so the gate runs on a container-local copy of the read-only checkout.
const CONTAINER_SOURCE: &str = "set -o pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -;";

#[must_use]
pub fn container_run_arguments(root: &Path, image: &str, target_volume: &str) -> Vec<OsString> {
    container_campaign_arguments(root, image, target_volume, ProofCampaign::Full, &[])
}

fn container_campaign_arguments(
    root: &Path,
    image: &str,
    target_volume: &str,
    campaign: ProofCampaign,
    sources: &[String],
) -> Vec<OsString> {
    // Docker reads `--mount` as one CSV record, so quoting keeps a comma in the path inside the field, and Windows forbids the quote character in paths.
    let mut mount = OsString::from("type=bind,\"source=");
    mount.push(root);
    mount.push("\",target=/source,readonly");
    // The same image also runs the gungraun benchmarks, whose setarch disables address-space randomization through a personality change the default seccomp profile refuses.
    let mut arguments: Vec<OsString> = [
        "run",
        "--rm",
        "--pull=never",
        "--platform",
        crate::container::LINUX_AMD64,
        "--security-opt=seccomp=unconfined",
        "--mount",
    ]
    .map(OsString::from)
    .into();
    arguments.push(mount);
    arguments.extend(dotfiles::bounded::resource_arguments());
    let mut action = if sources.is_empty() && campaign == ProofCampaign::Ci {
        "proofs-ci"
    } else {
        "proofs"
    }
    .to_owned();
    if !sources.is_empty() {
        for source in sources {
            action.push_str(" --source '");
            action.push_str(&source.replace('\'', "'\\''"));
            action.push('\'');
        }
    }
    arguments.extend(
        [
            "--volume",
            &format!("{target_volume}:/cache"),
            "--volume",
            crate::container::CARGO_REGISTRY_VOLUME,
            "--env",
            "CARGO_TARGET_DIR=/cache/target",
            "--env",
            "DOTFILES_HOST_CACHE=/cache",
            "--env",
            &format!("{INSIDE_CONTAINER}=1"),
            "--workdir",
            "/work",
            image,
            "bash",
            "-ec",
            &format!(
                "{CONTAINER_SOURCE} exec cargo run --locked --manifest-path xtask/Cargo.toml -- {action}"
            ),
        ]
        .map(OsString::from),
    );
    arguments
}

/// Runs the unchanged proof gate in a bounded Linux container with the checkout read-only.
fn verify_in_container(root: &Path, campaign: ProofCampaign) -> Result<()> {
    let root = crate::canonical(root)?;
    crate::phase_evidence::run(&root, "bounded Linux proof container", 1, || {
        verify_container_measured(&root, campaign, &[])
    })
}

fn verify_container_measured(
    root: &Path,
    campaign: ProofCampaign,
    sources: &[String],
) -> Result<()> {
    verify_container_window(root, campaign, sources, None)
}

fn verify_container_window(
    root: &Path,
    campaign: ProofCampaign,
    sources: &[String],
    window: Option<std::num::NonZeroU32>,
) -> Result<()> {
    ensure_linux_engine().context("Kani proofs on this host run in a Linux container")?;
    let image = proof_image(root)?;
    require_image(&image)?;
    let image_id = verified_image_id(&image)?;
    crate::container::prepare_cargo(root, &image_id)?;
    let manifest = selected_manifest(root, sources)?;
    let (harnesses, overhead) = campaign_workload(root, &manifest)?;
    let expected = campaign_harnesses(root, &manifest)?;
    let execution = dotfiles::bounded::run_campaign(
        &container_campaign_arguments(
            root,
            &image_id,
            &container_target_volume(root),
            campaign,
            sources,
        ),
        &image_id,
        harnesses,
        KANI_PARALLELISM,
        overhead,
        window,
    );
    let output = match execution {
        Ok(output) => output,
        Err(error) => {
            report_campaign_incomplete(&expected);
            return Err(error.context("proof campaign has no complete validated report"));
        }
    };
    crate::phase_evidence::retain("Linux proof container campaign", "stdout", &output.stdout)?;
    crate::phase_evidence::retain("Linux proof container campaign", "stderr", &output.stderr)?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        report_campaign_incomplete(&expected);
    }
    ensure!(
        output.status.success(),
        "the containerized proof gate failed: {}",
        output.status
    );
    Ok(())
}

fn report_campaign_incomplete(expected: &[String]) {
    for name in expected {
        println!("Kani {name}: incomplete (campaign report not validated)");
    }
}

fn campaign_harnesses(root: &Path, manifest: &obligations::Manifest) -> Result<Vec<String>> {
    let mut expected = Vec::new();
    for obligation in &manifest.obligations {
        if let Obligation::Kani { source, probes, .. } = obligation {
            let production = obligations::module(&fs::read_to_string(root.join(source))?)?;
            expected.extend(
                production
                    .harnesses
                    .into_iter()
                    .map(|name| format!("{source}::{name}")),
            );
            expected.extend(
                obligations::probe_harnesses(probes)?
                    .into_iter()
                    .map(|name| format!("{source}::counterexample::{name}")),
            );
        }
    }
    ensure!(
        !expected.is_empty(),
        "Kani campaign has no required harnesses"
    );
    Ok(expected)
}

fn campaign_workload(root: &Path, manifest: &obligations::Manifest) -> Result<(u32, u32)> {
    let mut harnesses = 0_u32;
    let mut modules = 0_u32;
    for obligation in &manifest.obligations {
        if let Obligation::Kani { source, probes, .. } = obligation {
            let production = obligations::module(&fs::read_to_string(root.join(source))?)?;
            let rejected = obligations::probe_harnesses(probes)?;
            let count = u32::try_from(production.harnesses.len() + rejected.len())?;
            harnesses = harnesses
                .checked_add(count)
                .context("proof inventory overflow")?;
            modules = modules
                .checked_add(1)
                .context("proof module count overflow")?;
        }
    }
    let phase_seconds = dotfiles::bounded::Policy::load(None)?.limits.seconds;
    let phases = u32::try_from(crate::proof_ci_phase::Phase::ALL.len())?;
    let overhead = modules
        .checked_add(phases)
        .and_then(|workloads| workloads.checked_mul(phase_seconds))
        .context("proof preparation safety cap overflow")?;
    Ok((harnesses, overhead))
}

pub(crate) fn verified_image_id(image: &str) -> Result<String> {
    let metadata = crate::phase_evidence::output(
        Tool::Docker
            .command()
            .args(["image", "inspect", "--format", "{{json .}}", image]),
        "proof container image identity",
    )?;
    ensure!(
        metadata.status.success(),
        "inspect the proof container image identity"
    );
    let metadata: Value = serde_json::from_slice(&metadata.stdout)?;
    ensure!(
        metadata["Os"] == "linux" && metadata["Architecture"] == "amd64",
        "proof container image must preserve Linux amd64 target semantics"
    );
    Ok(metadata["Id"]
        .as_str()
        .context("proof container image ID")?
        .to_owned())
}

/// Runs the complete proof campaign through the owner's bounded route or native CI.
///
/// # Errors
///
/// Returns an error when the pinned Kani version or Docker engine can't run, Docker can't build the verification image, or any proof or counterexample check fails.
pub fn verify(root: &Path) -> Result<()> {
    verify_campaign(root, ProofCampaign::Full)
}

/// Run selected subjects or the full campaign within an explicit owner time window.
///
/// # Errors
///
/// Refuses native execution, invalid subjects, an excessive window, or incomplete evidence.
pub fn verify_window(root: &Path, sources: &[String], window: std::num::NonZeroU32) -> Result<()> {
    ensure!(
        dotfiles::bounded::requires_container() || cfg!(windows),
        "an owner proof window requires the bounded container route"
    );
    selected_manifest(root, sources)?;
    let campaign = if sources.is_empty() {
        ProofCampaign::Full
    } else {
        ProofCampaign::Ci
    };
    crate::native_refinement::with_source(root, || {
        crate::quality::with_locked_workspaces(root, || {
            crate::phase_evidence::run(root, "owner-window bounded proof campaign", 1, || {
                verify_container_window(root, campaign, sources, Some(window))
            })
        })
    })
}

fn selected_manifest(root: &Path, sources: &[String]) -> Result<obligations::Manifest> {
    let mut manifest = obligations::manifest(root)?;
    if !sources.is_empty() {
        let selected: BTreeSet<_> = sources.iter().collect();
        ensure!(
            selected.len() == sources.len(),
            "duplicate selected proof source"
        );
        manifest.obligations.retain(|obligation| {
            matches!(obligation, Obligation::Kani { source, .. } if selected.contains(source))
        });
        ensure!(
            manifest.obligations.len() == selected.len(),
            "every selected proof source must name a registered Kani rule module"
        );
    }
    Ok(manifest)
}

/// Verify selected production rule modules and their complete rejecting inventories.
///
/// # Errors
///
/// Refuses unknown or duplicate sources and propagates incomplete or failed verification.
pub fn verify_sources(root: &Path, sources: &[String]) -> Result<()> {
    ensure!(!sources.is_empty(), "selected proof inventory is empty");
    let manifest = selected_manifest(root, sources)?;
    crate::native_refinement::with_source(root, || {
        crate::quality::with_locked_workspaces(root, || {
            if dotfiles::bounded::requires_container() || cfg!(windows) {
                crate::phase_evidence::run(
                    root,
                    "selected bounded Kani subjects",
                    sources.len(),
                    || verify_container_measured(root, ProofCampaign::Ci, sources),
                )
            } else {
                verify_kani(root, &manifest)
            }
        })
    })
}

/// Runs the CI campaign, which retains the proof checks and defers mutation to the full native campaign.
///
/// # Errors
///
/// Returns every failed proof, counterexample, model, or measurement check after the independent checks finish.
pub fn verify_ci(root: &Path) -> Result<()> {
    verify_campaign(root, ProofCampaign::Ci)
}

fn verify_campaign(root: &Path, campaign: ProofCampaign) -> Result<()> {
    crate::native_refinement::with_source(root, || {
        crate::quality::with_locked_workspaces(root, || verify_campaign_prepared(root, campaign))
    })
}

fn verify_campaign_prepared(root: &Path, campaign: ProofCampaign) -> Result<()> {
    let mut checks = Checks::default();
    if campaign == ProofCampaign::Ci {
        println!(
            "CI proof campaign: Kani, Miri, fuzz, benchmarks, and models; mutation and its fixture are deferred to the full native campaign."
        );
    }
    if cfg!(any(target_os = "macos", target_os = "linux"))
        && !dotfiles::bounded::requires_container()
    {
        checks.run("native proofs", || verify_native(root, campaign));
    } else {
        checks.run("container proofs", || verify_in_container(root, campaign));
    }
    if std::env::var_os(INSIDE_CONTAINER).is_some() {
        // The image pins Valgrind and the gungraun runner, so the budgets run here with them natively.
        checks.run("benchmarks", || {
            crate::phase_evidence::run(
                root,
                "fixed Linux benchmarks",
                crate::benchmarks::FUNCTIONS.len(),
                || crate::benchmarks::verify(root),
            )
        });
        return checks.finish();
    }
    // The instruction ceilings depend on the native x86_64 CPU's libc dispatch and the image.
    if cfg!(target_os = "linux") {
        checks.run("benchmarks", || {
            crate::phase_evidence::run(
                root,
                "fixed Linux benchmarks",
                crate::benchmarks::FUNCTIONS.len(),
                || crate::benchmarks::verify_in_container(root),
            )
        });
    }
    checks.run("models", || verify_models(root));
    checks.finish()
}

fn verify_models(root: &Path) -> Result<()> {
    let manifest = obligations::manifest(root)?;
    let mut models = Checks::default();
    for (name, workload, verifier) in [
        (
            "Quint models",
            manifest
                .obligations
                .iter()
                .filter(|obligation| matches!(obligation, Obligation::Quint(_)))
                .count(),
            crate::quint::verify as fn(&Path, &obligations::Manifest) -> Result<()>,
        ),
        (
            "Alloy models",
            manifest
                .obligations
                .iter()
                .filter(|obligation| matches!(obligation, Obligation::Alloy(_)))
                .count(),
            crate::alloy::verify,
        ),
        (
            "Lean packages",
            manifest
                .obligations
                .iter()
                .filter(|obligation| matches!(obligation, Obligation::Lean(_)))
                .count(),
            crate::lean::verify,
        ),
    ] {
        models.run(name, || {
            crate::phase_evidence::run(root, name, workload, || verifier(root, &manifest))
        });
    }
    models.finish()
}

/// Set inside the Kani container, whose gate leaves the Quint models to the host that started it.
pub const INSIDE_CONTAINER: &str = "DOTFILES_PROOFS_CONTAINER";

/// Requires a running Docker engine with Linux containers.
///
/// # Errors
///
/// Returns an error when the host lacks Docker or its engine serves another operating system.
pub fn ensure_linux_engine() -> Result<()> {
    let engine = Tool::Docker
        .command()
        .args(["version", "--format", "{{.Server.Os}}"])
        .stderr(Stdio::null())
        .output()
        .context("the proof gates need Docker with Linux containers; install Docker")?;
    ensure!(
        engine.status.success() && String::from_utf8(engine.stdout)?.trim() == "linux",
        "the proof gates need a running Docker engine with Linux containers"
    );
    Ok(())
}

fn run_native_checks(
    campaign: ProofCampaign,
    kani: impl FnOnce() -> Result<()>,
    miri: impl FnOnce() -> Result<()>,
    fuzz: impl FnOnce() -> Result<()>,
    mutation: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let mut checks = Checks::default();
    checks.run("Kani", kani);
    checks.run("Miri", miri);
    checks.run("fuzz", fuzz);
    if proof_budget_rules::mutation_required(campaign) {
        checks.run("mutation", mutation);
    }
    checks.finish()
}

fn verify_native(root: &Path, campaign: ProofCampaign) -> Result<()> {
    with_fuzz_compilation(
        || compile_fuzz_targets(root),
        || verify_native_prepared(root, campaign),
    )
}

fn verify_native_prepared(root: &Path, campaign: ProofCampaign) -> Result<()> {
    let manifest = obligations::manifest(root);
    run_native_checks(
        campaign,
        || {
            let manifest = manifest
                .as_ref()
                .map_err(|error| anyhow::anyhow!("{error:#}"))?;
            verify_kani(root, manifest)
        },
        || verify_miri(root),
        || verify_fuzz(root),
        || {
            let manifest = manifest
                .as_ref()
                .map_err(|error| anyhow::anyhow!("{error:#}"))?;
            verify_mutants(root, manifest)
        },
    )
}

pub(crate) fn verify_kani(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    let workload = manifest
        .obligations
        .iter()
        .filter(|obligation| matches!(obligation, Obligation::Kani { .. }))
        .count();
    crate::phase_evidence::run(root, "Kani subjects", workload, || {
        verify_kani_measured(root, manifest)
    })
}

/// Verify the answer gate's registered subjects under the installed or explicitly selected bounded policy.
///
/// # Errors
///
/// Refuses missing obligations, incomplete proofs, admitted counterexamples, and a failed corpus replay.
pub fn verify_answer(root: &Path, policy: Option<&Path>) -> Result<()> {
    let root = crate::canonical(root)?;
    if dotfiles::bounded::requires_container() {
        return verify_answer_container(&root, &dotfiles::bounded::Policy::load(policy)?);
    }
    ensure!(
        policy.is_none(),
        "an explicit policy requires the owner-side container route"
    );
    let mut manifest = obligations::manifest(&root)?;
    manifest.obligations.retain(|obligation| matches!(obligation, Obligation::Kani { source, .. } if source == "crates/rules/src/answer_rules.rs"));
    ensure!(
        manifest.obligations.len() == 1,
        "answer gate needs exactly one registered rule obligation"
    );
    crate::phase_evidence::run(&root, "answer policy subjects", 1, || {
        kani_environment_version(&root)?;
        let verifier = crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
        let engine = crate::phase_evidence::engine_digest(&root)?;
        let (proved, rejected) = run_rule_subjects(
            &manifest,
            |source, probes| {
                prepare_rules(
                    &root,
                    source,
                    probes,
                    &flags(&manifest, source),
                    &verifier,
                    &engine,
                )
            },
            execute_rules,
        )?;
        println!(
            "Verified {proved} answer policy harnesses and rejected {rejected} counterexamples."
        );
        let host = nightly_host(&root)?;
        let output = replay(&root, FUZZ_DIRECTORY, "answer_plan", &host)?;
        ensure!(
            output.status.success(),
            "answer parser corpus replay failed"
        );
        println!("Replayed the committed answer parser corpus.");
        Ok(())
    })
}

fn verify_answer_container(root: &Path, policy: &dotfiles::bounded::Policy) -> Result<()> {
    ensure_linux_engine()?;
    let image = proof_image(root)?;
    require_image(&image)?;
    let image = verified_image_id(&image)?;
    crate::container::prepare_cargo(root, &image)?;
    let mut arguments = container_run_arguments(root, &image, &container_target_volume(root));
    *arguments.last_mut().context("answer proof container omits its command")? = format!(
        "{CONTAINER_SOURCE} exec cargo run --locked --manifest-path xtask/Cargo.toml -- answer-proofs"
    ).into();
    arguments.splice(
        1..1,
        [
            OsString::from("--network"),
            OsString::from("none"),
            OsString::from("--env"),
            OsString::from("CARGO_BUILD_JOBS=1"),
        ],
    );
    crate::phase_evidence::run(root, "bounded answer proof subjects", 1, || {
        let output = dotfiles::bounded::run_with_policy(policy, &arguments, &image)?;
        crate::phase_evidence::retain("answer proof container", "stdout", &output.stdout)?;
        crate::phase_evidence::retain("answer proof container", "stderr", &output.stderr)?;
        std::io::stdout().write_all(&output.stdout)?;
        std::io::stderr().write_all(&output.stderr)?;
        ensure!(
            output.status.success(),
            "answer proof subjects are incomplete: {}",
            output.status
        );
        Ok(())
    })
}

fn verify_kani_measured(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    kani_environment_version(root)?;
    verify_kani_driver(root)?;
    verify_kani_subject_regressions(root)?;
    let verifier_hash = crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
    let engine_hash = crate::phase_evidence::engine_digest(root)?;
    let (verified, rejected) = run_rule_subjects(
        manifest,
        |source, probes| {
            prepare_rules(
                root,
                source,
                probes,
                &flags(manifest, source),
                &verifier_hash,
                &engine_hash,
            )
        },
        execute_rules,
    )?;
    println!(
        "Verified all {verified} production policy harnesses with their reachable outcomes, and rejected all {rejected} counterexamples"
    );
    Ok(())
}

fn run_rule_subjects<T>(
    manifest: &obligations::Manifest,
    mut prepare: impl FnMut(&str, &str) -> Result<T>,
    mut execute: impl FnMut(T) -> Result<(usize, usize)>,
) -> Result<(usize, usize)> {
    let mut prepared = Vec::new();
    let mut checks = Checks::default();
    for obligation in &manifest.obligations {
        if let Obligation::Kani { source, probes, .. } = obligation {
            checks.run(source, || {
                prepared.push((source, prepare(source, probes)?));
                Ok(())
            });
        }
    }
    checks.finish()?;
    ensure!(
        !prepared.is_empty(),
        "Kani campaign has no required rule subjects"
    );
    let mut verified = 0;
    let mut rejected = 0;
    let mut checks = Checks::default();
    for (source, subject) in prepared {
        checks.run(source, || {
            let (proved, refused) = execute(subject)?;
            verified += proved;
            rejected += refused;
            Ok(())
        });
    }
    checks.finish()?;
    Ok((verified, rejected))
}

fn verify_kani_driver(root: &Path) -> Result<()> {
    let output = crate::phase_evidence::output(
        Tool::Cargo.command().current_dir(root).args([
            "test",
            "--locked",
            "--package",
            "xtask",
            "--test",
            "container_proofs",
            "--",
            "--ignored",
            "--exact",
            KANI_BATCH_TEST,
        ]),
        "Kani driver regression",
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    ensure!(
        output.status.success()
            && stdout
                .lines()
                .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;")),
        "the Kani driver regression did not pass"
    );
    Ok(())
}

fn verify_kani_subject_regressions(root: &Path) -> Result<()> {
    ensure!(
        !dotfiles::bounded::requires_container(),
        "Kani subject regressions require the bounded proof campaign or native CI"
    );
    for test in KANI_SUBJECT_TESTS {
        let output = crate::phase_evidence::output(
            Tool::Cargo.command().current_dir(root).args([
                "test",
                "--locked",
                "--package",
                "xtask",
                "--lib",
                "--",
                "--ignored",
                "--exact",
                test,
            ]),
            "Kani subject regression",
        )?;
        ensure!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;")),
            "Kani subject regression {test} did not pass"
        );
    }
    Ok(())
}

/// The cargo-fuzz release that `mise.toml` and the proof image pin.
pub const FUZZ_VERSION: &str = "0.13.2";

/// The cargo-fuzz directory of this repository's parser targets, which `assure.toml` names as their owners.
pub const FUZZ_DIRECTORY: &str = "fuzz";
const FUZZ_REPLAY_CODEGEN_UNITS: &str = "16";
const FUZZ_COMPILE_OPERATION: &str = "fuzz target compilation prerequisite";
const FUZZ_COMPILE_MANIFEST: &str = "fuzz/Cargo.toml";
const FUZZ_SHARED_SOURCE: &str = "crates/dotfiles/examples/support";

#[derive(Debug)]
struct FuzzCompileInputs {
    workload: dotfiles::artifacts::CargoWorkload,
    targets: Vec<String>,
    closure: [String; 2],
}

impl FuzzCompileInputs {
    fn capture(root: &Path) -> Result<Self> {
        let targets = fuzz_targets(&root.join(FUZZ_DIRECTORY))?;
        ensure!(!targets.is_empty(), "{FUZZ_DIRECTORY} holds no fuzz target");
        Ok(Self {
            workload: dotfiles::artifacts::CargoWorkload::capture(root)?,
            targets,
            closure: Self::closure(root)?,
        })
    }

    fn closure(root: &Path) -> Result<[String; 2]> {
        Ok([
            crate::phase_evidence::model_inputs(root)?,
            crate::phase_evidence::verifier_digest(&root.join(FUZZ_SHARED_SOURCE))?,
        ])
    }

    fn require_current(&self, root: &Path) -> Result<()> {
        self.workload.require_current(root)?;
        ensure!(
            self.targets == fuzz_targets(&root.join(FUZZ_DIRECTORY))?
                && self.closure == Self::closure(root)?,
            "Source changed during the fuzz compilation prerequisite"
        );
        Ok(())
    }
}

pub(crate) fn with_fuzz_compilation<T>(
    compile: impl FnOnce() -> Result<()>,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    compile()?;
    body()
}

pub(crate) fn compile_fuzz_targets(root: &Path) -> Result<()> {
    let inputs = FuzzCompileInputs::capture(root)?;
    crate::phase_evidence::run(root, FUZZ_COMPILE_OPERATION, inputs.targets.len(), || {
        let output = crate::phase_evidence::output(
            nightly_cargo(root)
                .args([
                    "check",
                    "--locked",
                    "--offline",
                    "--bins",
                    "--jobs",
                    "2",
                    "--manifest-path",
                ])
                .arg(root.join(FUZZ_COMPILE_MANIFEST)),
            FUZZ_COMPILE_OPERATION,
        )?;
        require_fuzz_compilation(&output)?;
        inputs.require_current(root)
    })
}

fn require_fuzz_compilation(output: &std::process::Output) -> Result<()> {
    ensure!(
        output.status.success(),
        "{FUZZ_COMPILE_OPERATION} failed: {}; stdout={}; stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// A cargo-fuzz directory whose one target panics on its corpus, which the replay must refuse.
const FUZZ_FIXTURE: &str = "xtask/proofs/fuzz-fixture";

/// The name of every target under `<directory>/fuzz_targets`.
///
/// # Errors
///
/// Returns an error when reading the directory fails.
pub fn fuzz_targets(directory: &Path) -> Result<Vec<String>> {
    let mut targets = Vec::new();
    for entry in fs::read_dir(directory.join("fuzz_targets"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && let Some(name) = path.file_stem().and_then(|name| name.to_str())
        {
            targets.push(name.to_owned());
        }
    }
    targets.sort();
    Ok(targets)
}

fn nightly_host(root: &Path) -> Result<String> {
    let output = crate::phase_evidence::output(
        Tool::Rustup.command().current_dir(root).args([
            "run",
            MIRI_TOOLCHAIN,
            Tool::Rustc.program(),
            "-vV",
        ]),
        "pinned nightly host",
    )?;
    ensure!(
        output.status.success(),
        "cannot resolve the pinned nightly host:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    let mut hosts = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("host: "));
    let host = hosts.next().context("the pinned nightly has no host")?;
    ensure!(
        !host.is_empty() && hosts.next().is_none(),
        "the pinned nightly must name exactly one nonempty host"
    );
    Ok(host.to_owned())
}

/// Replays the committed corpus of `target` once for the pinned compiler's host, with no new input.
fn replay(root: &Path, directory: &str, target: &str, host: &str) -> Result<std::process::Output> {
    let mut command = nightly_cargo(root);
    command.args(["fuzz", "run", "--codegen-units", FUZZ_REPLAY_CODEGEN_UNITS]);
    if directory == FUZZ_DIRECTORY && target == "answer_plan" {
        command.arg("--no-default-features");
    }
    crate::phase_evidence::output(
        command
            .args(["--target", host, "--fuzz-dir", directory, target])
            .arg(format!("{directory}/corpus/{target}"))
            .args(["--", "-runs=0"]),
        &format!("fuzz {directory}/{target} for {host}"),
    )
}

/// Replays every parser target over its committed corpus and refuses a fixture whose corpus makes its target panic.
pub(crate) fn verify_fuzz(root: &Path) -> Result<()> {
    ensure!(
        !dotfiles::bounded::requires_container(),
        "native fuzzing refuses on owner machines; use `just proofs` or `dotfiles bounded run`"
    );
    let workload = fuzz_targets(&root.join(FUZZ_DIRECTORY))?.len() + 1;
    crate::phase_evidence::run(root, "fuzz targets and rejecting fixture", workload, || {
        verify_fuzz_measured(root)
    })
}

fn verify_fuzz_measured(root: &Path) -> Result<()> {
    let reports = collect_fuzz(root)?;
    validate_fuzz_reports(&fuzz_inventory(root, &reports.host)?, &reports)?;
    println!(
        "Replayed the corpus of every fuzz target, {}, and refused its fixture",
        reports
            .runs
            .iter()
            .map(|run| run.target.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FuzzRun {
    target: String,
    corpus_hash: String,
    result: crate::phase_evidence::RawResult,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FuzzReports {
    version: crate::phase_evidence::RawResult,
    host: String,
    runs: Vec<FuzzRun>,
    fixture_corpus_hash: String,
    fixture: crate::phase_evidence::RawResult,
}

#[derive(Debug)]
pub(crate) struct FuzzInventory {
    host: String,
    targets: Vec<(String, String)>,
    fixture_corpus_hash: String,
}

pub(crate) fn fuzz_inventory(root: &Path, host: &str) -> Result<FuzzInventory> {
    let names = fuzz_targets(&root.join(FUZZ_DIRECTORY))?;
    let mut targets = Vec::new();
    for name in names {
        let corpus = root.join(FUZZ_DIRECTORY).join("corpus").join(&name);
        ensure!(
            fs::read_dir(&corpus).is_ok_and(|mut entries| entries.next().is_some()),
            "fuzz corpus is empty"
        );
        targets.push((name, crate::phase_evidence::verifier_digest(&corpus)?));
    }
    Ok(FuzzInventory {
        host: host.into(),
        targets,
        fixture_corpus_hash: crate::phase_evidence::verifier_digest(
            &root.join(FUZZ_FIXTURE).join("corpus/crash"),
        )?,
    })
}

pub(crate) fn validate_fuzz_reports(expected: &FuzzInventory, reports: &FuzzReports) -> Result<()> {
    ensure!(
        reports.version.success()
            && reports
                .version
                .stdout()?
                .contains(&format!("cargo-fuzz {FUZZ_VERSION}")),
        "fuzz tool version is absent or differs"
    );
    ensure!(
        reports.host == expected.host && !expected.host.is_empty(),
        "fuzz target semantics differ from the pinned nightly host"
    );
    ensure!(
        !expected.targets.is_empty() && reports.runs.len() == expected.targets.len(),
        "fuzz target inventory is empty, missing, repeated, or unexpected"
    );
    for (run, (target, corpus_hash)) in reports.runs.iter().zip(&expected.targets) {
        ensure!(
            run.target == *target && run.result.success(),
            "fuzz target identity or outcome differs"
        );
        ensure!(run.corpus_hash == *corpus_hash, "fuzz corpus changed");
    }
    ensure!(
        reports.fixture_corpus_hash == expected.fixture_corpus_hash,
        "fuzz rejecting corpus changed"
    );
    ensure!(
        reports.fixture.failure() && reports.fixture.stderr()?.contains("panicked"),
        "fuzz did not reject its crashing corpus"
    );
    Ok(())
}

pub(crate) fn collect_fuzz(root: &Path) -> Result<FuzzReports> {
    let version = crate::phase_evidence::output(
        Tool::Cargo.command().args(["fuzz", "--version"]),
        "cargo-fuzz version",
    )?;
    ensure!(
        version.status.success()
            && std::str::from_utf8(&version.stdout)?
                .contains(&format!("cargo-fuzz {FUZZ_VERSION}")),
        "the pinned cargo-fuzz {FUZZ_VERSION} is unavailable"
    );
    let targets = fuzz_targets(&root.join(FUZZ_DIRECTORY))?;
    ensure!(!targets.is_empty(), "{FUZZ_DIRECTORY} holds no fuzz target");
    let host = nightly_host(root)?;
    let mut checks = Checks::default();
    let mut runs = Vec::new();
    for target in &targets {
        checks.run(&format!("fuzz target {target}"), || {
            let corpus = root.join(FUZZ_DIRECTORY).join("corpus").join(target);
            ensure!(
                fs::read_dir(&corpus).is_ok_and(|mut entries| entries.next().is_some()),
                "the fuzz target {target} has no corpus under {}",
                corpus.display()
            );
            let corpus_hash = crate::phase_evidence::verifier_digest(&corpus)?;
            let output = replay(root, FUZZ_DIRECTORY, target, &host)?;
            ensure!(
                output.status.success(),
                "replaying the corpus of {target} failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            runs.push(FuzzRun {
                target: target.clone(),
                corpus_hash,
                result: output.into(),
            });
            Ok(())
        });
    }
    let mut fixture = None;
    let fixture_corpus_hash =
        crate::phase_evidence::verifier_digest(&root.join(FUZZ_FIXTURE).join("corpus/crash"))?;
    checks.run("fuzz fixture", || {
        let output = replay(root, FUZZ_FIXTURE, "crash", &host)?;
        ensure!(
            !output.status.success() && String::from_utf8_lossy(&output.stderr).contains("panicked"),
            "the corpus replay admitted {FUZZ_FIXTURE}, whose corpus makes its target panic, so the gate proves nothing"
        );
        fixture = Some(output.into());
        Ok(())
    });
    checks.finish()?;
    Ok(FuzzReports {
        version: version.into(),
        host,
        runs,
        fixture_corpus_hash,
        fixture: fixture.context("fuzz rejecting fixture did not complete")?,
    })
}

/// The cargo-mutants release that `mise.toml` and the proof image pin.
pub const MUTANTS_VERSION: &str = "27.1.0";

/// The crate whose specification lost its assertion, which the mutation gate must refuse.
const MUTANTS_FIXTURE: &str = "xtask/proofs/mutants-fixture";
const MUTANTS_FIXTURE_PACKAGE: &str = "mutants-fixture";

/// The package whose tests must catch every mutant of its code.
const MUTATED_PACKAGE: &str = "rules";

/// Each mutation worker runs its tests serially while the campaign schedules workers.
const MUTANT_TEST_THREADS: &str = "1";

const MUTANT_OUTPUT_DIRECTORY: &str = "mutation-evidence";
const MUTANT_OUTPUT_PREFIX: &str = "campaign-";
const MUTANT_SOURCE_PREFIX: &str = "mutation-source-";
const MUTANT_OWNER_SCHEMA: &str = "dotfiles-mutation-temp-owner-v1";
const MUTANT_OWNER_MARKER: &str = "owner.json";
const MUTANT_OWNER_LOCK: &str = "owner.lock";
const MUTANT_SOURCE_DIRECTORY: &str = "source";
const MUTANT_TEMP_DIRECTORY: &str = "temporary";
const MUTANT_TEMP_VARIABLES: [&str; 3] = ["TMPDIR", "TMP", "TEMP"];
#[cfg(test)]
const MUTANT_WORKER_PREFIX: &str = "cargo-mutants-";
#[cfg(unix)]
const MUTANT_SOURCE_FILE_MODE: u32 = 0o600;
#[cfg(unix)]
const MUTANT_SOURCE_EXECUTE_BITS: u32 = 0o111;
const HOST_CACHE_VARIABLE: &str = "DOTFILES_HOST_CACHE";

struct MutationSource {
    directory: PathBuf,
    temporary: PathBuf,
    lease: fs::File,
}

impl MutationSource {
    fn claim(campaign: &Path) -> Result<Self> {
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(campaign.join(MUTANT_OWNER_LOCK))?;
        lease
            .try_lock()
            .context("claim mutation campaign ownership")?;
        fs::write(
            campaign.join(MUTANT_OWNER_MARKER),
            serde_json::to_vec(&serde_json::json!({
                "schema": MUTANT_OWNER_SCHEMA,
                "kept": false,
                "role": "scratch",
            }))?,
        )?;
        let directory = campaign.join(MUTANT_SOURCE_DIRECTORY);
        let temporary = campaign.join(MUTANT_TEMP_DIRECTORY);
        fs::create_dir(&directory)?;
        fs::create_dir(&temporary)?;
        Ok(Self {
            directory,
            temporary,
            lease,
        })
    }
}

impl Drop for MutationSource {
    fn drop(&mut self) {
        platform::cleanup::report(self.lease.unlock(), "release mutation campaign ownership");
    }
}

fn with_mutant_source<T>(
    root: &Path,
    check: impl FnOnce(&MutationSource) -> Result<T>,
) -> Result<T> {
    ensure!(
        fs::symlink_metadata(root)?.file_type().is_dir(),
        "mutation source must be a physical directory"
    );
    let mut files = Vec::new();
    crate::profiles::source_files(root, &mut files)?;
    ensure!(!files.is_empty(), "mutation source has no inputs");
    let scope = tempfile::Builder::new()
        .prefix(MUTANT_SOURCE_PREFIX)
        .tempdir()?;
    let result = (|| {
        let source = MutationSource::claim(scope.path())?;
        for file in files {
            let metadata = fs::symlink_metadata(&file)?;
            ensure!(metadata.is_file(), "mutation source contains a non-file");
            let target = source.directory.join(file.strip_prefix(root)?);
            fs::create_dir_all(target.parent().context("mutation source parent")?)?;
            let bytes = fs::read(&file)?;
            fs::write(&target, &bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(
                    &target,
                    fs::Permissions::from_mode(
                        MUTANT_SOURCE_FILE_MODE
                            | (metadata.permissions().mode() & MUTANT_SOURCE_EXECUTE_BITS),
                    ),
                )?;
            }
            ensure!(fs::read(&target)? == bytes, "mutation source copy differs");
        }
        check(&source)
    })();
    platform::cleanup::report(scope.close(), "remove writable mutation source");
    result
}

fn mutants(directory: &Path, output: &Path, temporary: &Path) -> std::process::Command {
    mutants_with_cores(
        directory,
        output,
        temporary,
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
    )
}

fn mutants_with_cores(
    directory: &Path,
    output: &Path,
    temporary: &Path,
    available: usize,
) -> std::process::Command {
    let mut command = Tool::Cargo.command();
    let jobs = proof_budget_rules::mutant_jobs(available).to_string();
    // Each mutant builds in its own copy of the tree, which a shared target directory would serialize.
    command
        .env_remove("CARGO_TARGET_DIR")
        .env("RUST_TEST_THREADS", MUTANT_TEST_THREADS)
        .envs(MUTANT_TEMP_VARIABLES.map(|variable| (variable, temporary)))
        .args(["mutants", "--no-shuffle", "--jobserver=true", "--jobs"])
        .arg(&jobs)
        .arg("--jobserver-tasks")
        .arg(&jobs)
        .arg("--dir")
        .arg(directory)
        .arg("--output")
        .arg(output);
    command
}

fn mutant_output_root(root: &Path, cache: Option<&Path>) -> std::path::PathBuf {
    cache
        .map_or_else(|| root.join("target"), Path::to_path_buf)
        .join(MUTANT_OUTPUT_DIRECTORY)
}

fn with_mutant_output(directory: &Path, check: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    fs::create_dir_all(directory)?;
    let output = tempfile::Builder::new()
        .prefix(MUTANT_OUTPUT_PREFIX)
        .tempdir_in(directory)?;
    match check(output.path()) {
        Ok(()) => {
            let removal = format!(
                "remove successful mutation evidence at {}",
                output.path().display()
            );
            platform::cleanup::report(output.close(), removal);
            Ok(())
        }
        Err(error) => {
            let retained = output.keep();
            Err(error.context(format!(
                "mutation evidence retained at {}",
                retained.display()
            )))
        }
    }
}

fn mutation_campaign(root: &Path, check: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let cache = std::env::var_os(HOST_CACHE_VARIABLE)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    let directory = mutant_output_root(root, cache.as_deref());
    with_mutant_output(&directory, check)
}

fn lines(text: &str) -> u64 {
    text.lines().filter(|line| !line.trim().is_empty()).count() as u64
}

#[derive(Debug, serde::Deserialize)]
struct MutationIdentity {
    name: String,
    package: String,
}

#[derive(Debug, serde::Deserialize)]
enum MutationScenario {
    Baseline,
    Mutant(MutationIdentity),
}

#[derive(Debug, serde::Deserialize)]
enum MutationStage {
    Build,
    Test,
}

#[derive(Debug, serde::Deserialize)]
enum MutationStatus {
    Success,
    Failure(i32),
    Timeout,
}

impl MutationStatus {
    fn phase(&self) -> Result<proof_budget_rules::MutationPhase> {
        use proof_budget_rules::MutationPhase;
        match self {
            Self::Success => Ok(MutationPhase::Passed),
            Self::Failure(code) if *code != 0 => Ok(MutationPhase::Failed),
            Self::Timeout => Ok(MutationPhase::TimedOut),
            Self::Failure(_) => {
                anyhow::bail!("a mutation phase claims failure with exit code zero")
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct MutationPhaseReceipt {
    phase: MutationStage,
    process_status: MutationStatus,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq)]
enum MutationSummary {
    Success,
    Failure,
    CaughtMutant,
    MissedMutant,
    Timeout,
    Unviable,
}

#[derive(Debug, serde::Deserialize)]
struct MutationReceipt {
    scenario: MutationScenario,
    summary: MutationSummary,
    phase_results: Vec<MutationPhaseReceipt>,
}

#[derive(Debug, serde::Deserialize)]
struct MutationReport {
    outcomes: Vec<MutationReceipt>,
    total_mutants: u64,
    missed: u64,
    caught: u64,
    timeout: u64,
    unviable: u64,
    success: u64,
    end_time: Option<String>,
    cargo_mutants_version: String,
}

fn mutation_phases(
    phases: &[MutationPhaseReceipt],
) -> Result<(
    proof_budget_rules::MutationPhase,
    Option<proof_budget_rules::MutationPhase>,
)> {
    match phases {
        [
            MutationPhaseReceipt {
                phase: MutationStage::Build,
                process_status,
            },
        ] => Ok((process_status.phase()?, None)),
        [
            MutationPhaseReceipt {
                phase: MutationStage::Build,
                process_status: build,
            },
            MutationPhaseReceipt {
                phase: MutationStage::Test,
                process_status: test,
            },
        ] => Ok((build.phase()?, Some(test.phase()?))),
        _ => anyhow::bail!("mutation phases are missing, duplicated, or out of order"),
    }
}

fn validate_mutation_report(
    inventory: &[MutationIdentity],
    report: MutationReport,
    status: Option<i32>,
    package: &str,
) -> Result<MutationReport> {
    use proof_budget_rules::{MutationOutcome, MutationPhase, mutation_exit, mutation_outcome};
    ensure!(
        report.cargo_mutants_version == MUTANTS_VERSION
            && report
                .end_time
                .as_deref()
                .is_some_and(|end| !end.is_empty()),
        "mutation execution is incomplete or its version differs"
    );
    let expected: BTreeSet<_> = inventory
        .iter()
        .map(|mutant| mutant.name.as_str())
        .collect();
    ensure!(
        !expected.is_empty()
            && expected.len() == inventory.len()
            && inventory
                .iter()
                .all(|mutant| !mutant.name.is_empty() && mutant.package == package),
        "mutation inventory is empty, duplicated, or names another package"
    );
    let mut actual = BTreeSet::new();
    let mut counts = [0u64; 4];
    let mut baseline = false;
    for outcome in &report.outcomes {
        let (build, test) = mutation_phases(&outcome.phase_results)?;
        let MutationScenario::Mutant(mutant) = &outcome.scenario else {
            ensure!(
                !baseline
                    && build == MutationPhase::Passed
                    && test == Some(MutationPhase::Passed)
                    && outcome.summary == MutationSummary::Success,
                "mutation baseline is missing, duplicated, or failed"
            );
            baseline = true;
            continue;
        };
        ensure!(
            baseline,
            "mutation scenarios precede their successful baseline"
        );
        ensure!(
            mutant.package == package
                && expected.contains(mutant.name.as_str())
                && actual.insert(mutant.name.as_str()),
            "mutation outcome has an unexpected or duplicate identity"
        );
        let result = mutation_outcome(build, test)
            .context("mutation outcome has no completed test or failed build")?;
        let summary = match result {
            MutationOutcome::Caught => MutationSummary::CaughtMutant,
            MutationOutcome::Missed => MutationSummary::MissedMutant,
            MutationOutcome::TimedOut => MutationSummary::Timeout,
            MutationOutcome::Unviable => MutationSummary::Unviable,
        };
        ensure!(
            outcome.summary == summary,
            "mutation summary contradicts its phase results"
        );
        let count = counts
            .get_mut(result as usize)
            .context("unknown mutation result")?;
        *count = count.checked_add(1).context("mutation count overflow")?;
    }
    ensure!(
        baseline && actual == expected,
        "mutation execution omitted its baseline or selected mutants"
    );
    ensure!(
        report.success == 0
            && [
                report.caught,
                report.missed,
                report.timeout,
                report.unviable
            ] == counts,
        "mutation counters contradict the completed scenarios"
    );
    let exit = mutation_exit(report.total_mutants, counts)
        .context("mutation total disagrees with completed scenarios")?;
    ensure!(
        status == Some(exit),
        "mutation process exit contradicts its complete results"
    );
    Ok(report)
}

fn mutation_names(report: &MutationReport, summary: MutationSummary) -> Vec<String> {
    report
        .outcomes
        .iter()
        .filter_map(|outcome| {
            if outcome.summary == summary
                && let MutationScenario::Mutant(mutant) = &outcome.scenario
            {
                Some(mutant.name.clone())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
fn completed_mutation_report(
    output: &Path,
    status: std::process::ExitStatus,
    package: &str,
) -> Result<MutationReport> {
    decode_mutation_origin(&mutation_origin(output, status.code(), package)?, package)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MutationOrigin {
    package: String,
    status: Option<i32>,
    inventory: Vec<u8>,
    outcomes: Vec<u8>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MutationReports {
    version: crate::phase_evidence::RawResult,
    production: MutationOrigin,
    fixture: MutationOrigin,
}

fn mutation_origin(output: &Path, status: Option<i32>, package: &str) -> Result<MutationOrigin> {
    let reports = output.join("mutants.out");
    let inventory = fs::read(reports.join("mutants.json"))?;
    let report = fs::read(reports.join("outcomes.json"))?;
    crate::phase_evidence::retain(package, "mutation-inventory", &inventory)?;
    crate::phase_evidence::retain(package, "mutation-outcomes", &report)?;
    Ok(MutationOrigin {
        package: package.into(),
        status,
        inventory,
        outcomes: report,
    })
}

fn decode_mutation_origin(origin: &MutationOrigin, package: &str) -> Result<MutationReport> {
    ensure!(
        origin.package == package,
        "mutation origin names another package"
    );
    let inventory: Vec<MutationIdentity> = serde_json::from_slice(&origin.inventory)?;
    let report: MutationReport = serde_json::from_slice(&origin.outcomes)?;
    validate_mutation_report(&inventory, report, origin.status, package)
}

/// Mutates the decision core, holds its survivors and unexplained timeouts to their baselines, and refuses a fixture whose specification lost its assertion.
/// Test times stay advisory: only the outcome of each mutant counts.
fn verify_mutants(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    ensure!(
        !dotfiles::bounded::requires_container(),
        "native mutation testing refuses on owner machines; use `just proofs` or `dotfiles bounded run`"
    );
    crate::phase_evidence::run(
        root,
        "mutation production and rejecting fixture",
        MUTATION_CAMPAIGN_CASES,
        || verify_mutants_measured(root, manifest),
    )
}

fn verify_mutants_measured(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    validate_mutant_reports(root, manifest, &collect_mutants(root, manifest)?)?;
    println!(
        "Every mutant of {MUTATED_PACKAGE} died or holds its baseline, and the fixture without an assertion let its mutants survive"
    );
    Ok(())
}

pub(crate) fn validate_mutant_reports(
    root: &Path,
    manifest: &obligations::Manifest,
    reports: &MutationReports,
) -> Result<()> {
    ensure!(
        reports.version.success()
            && reports
                .version
                .stdout()?
                .contains(&format!("cargo-mutants {MUTANTS_VERSION}")),
        "mutation tool version is absent or differs"
    );
    validate_mutation_production(
        root,
        manifest,
        &decode_mutation_origin(&reports.production, MUTATED_PACKAGE)?,
    )?;
    let fixture = decode_mutation_origin(&reports.fixture, MUTANTS_FIXTURE_PACKAGE)?;
    ensure!(
        fixture.missed != 0,
        "mutation did not reject its assertionless fixture"
    );
    Ok(())
}

pub(crate) fn collect_mutants(
    root: &Path,
    manifest: &obligations::Manifest,
) -> Result<MutationReports> {
    let version = crate::phase_evidence::output(
        Tool::Cargo.command().args(["mutants", "--version"]),
        "cargo-mutants version",
    )?;
    ensure!(
        version.status.success()
            && std::str::from_utf8(&version.stdout)?
                .contains(&format!("cargo-mutants {MUTANTS_VERSION}")),
        "the pinned cargo-mutants {MUTANTS_VERSION} is unavailable"
    );
    with_mutant_source(root, |source| {
        collect_staged_mutants(root, source, manifest, version)
    })
}

fn collect_staged_mutants(
    root: &Path,
    source: &MutationSource,
    manifest: &obligations::Manifest,
    version: std::process::Output,
) -> Result<MutationReports> {
    let mut checks = Checks::default();
    let mut production = None;
    checks.run("production mutants", || {
        mutation_campaign(root, |output| {
            let status = crate::phase_evidence::output(
                mutants(&source.directory, output, &source.temporary)
                    .args(["--package", MUTATED_PACKAGE]),
                "production mutation campaign",
            )?
            .status;
            let raw = mutation_origin(output, status.code(), MUTATED_PACKAGE)?;
            validate_mutation_production(
                root,
                manifest,
                &decode_mutation_origin(&raw, MUTATED_PACKAGE)?,
            )?;
            production = Some(raw);
            Ok(())
        })
    });
    let mut fixture = None;
    checks.run("mutation fixture", || {
        mutation_campaign(root, |output| {
            let result = crate::phase_evidence::output(
                &mut mutants(&source.directory.join(MUTANTS_FIXTURE), output, &source.temporary), "mutation assertion-defect fixture",
            )?;
            let raw = mutation_origin(output, result.status.code(), MUTANTS_FIXTURE_PACKAGE)?;
            let report = decode_mutation_origin(&raw, MUTANTS_FIXTURE_PACKAGE)?;
            ensure!(
                report.missed != 0,
                "the mutation gate admitted {MUTANTS_FIXTURE}, whose specification asserts nothing, so the gate proves nothing"
            );
            fixture = Some(raw);
            Ok(())
        })
    });
    checks.finish()?;
    Ok(MutationReports {
        version: version.into(),
        production: production.context("production mutation did not complete")?,
        fixture: fixture.context("mutation fixture did not complete")?,
    })
}

fn validate_mutation_production(
    root: &Path,
    manifest: &obligations::Manifest,
    report: &MutationReport,
) -> Result<()> {
    ensure!(
        report.caught != 0,
        "cargo-mutants caught no mutant of {MUTATED_PACKAGE}, so the campaign didn't run"
    );
    let missed = mutation_names(report, MutationSummary::MissedMutant).join("\n");
    let timeouts = mutation_names(report, MutationSummary::Timeout).join("\n");
    let unexplained = obligations::unexplained_timeouts(manifest, &timeouts);
    let baselines = obligations::parse_baselines(
        &fs::read_to_string(root.join(obligations::BASELINES)).context(obligations::BASELINES)?,
    )?;
    let refusals = obligations::mutation_metrics(
        &baselines,
        obligations::Campaign {
            missed: lines(&missed),
            timed_out: unexplained.len() as u64,
        },
    );
    ensure!(
        refusals.is_empty(),
        "{}\nKill each surviving mutant with a test or a stronger specification:\n{missed}\nEnd each timed-out mutant or name it as non-terminating in {}:\n{}",
        refusals.join("\n"),
        obligations::MANIFEST,
        unexplained.join("\n")
    );
    Ok(())
}

/// The unstable options the obligations of `source` need: stubbing always, and what each obligation names.
fn flags(manifest: &obligations::Manifest, source: &str) -> BTreeSet<&'static str> {
    let mut flags = BTreeSet::from([KaniFlag::Stubbing.argument()]);
    for obligation in &manifest.obligations {
        let named = match obligation {
            Obligation::Kani {
                source: declared,
                flags,
                ..
            }
            | Obligation::KaniContract {
                source: declared,
                flags,
                ..
            } if declared == source => flags,
            _ => continue,
        };
        flags.extend(named.iter().map(|flag| flag.argument()));
    }
    flags
}

/// The nightly that Kani pins, which also carries Miri.
pub const MIRI_TOOLCHAIN: &str = "nightly-2026-08-21";

/// The crates whose tests need no foreign interface, process, or network, so Miri can interpret every one.
pub const MIRI_CRATES: [&str; 1] = ["rules"];

/// The host and Windows, with Linux retained when macOS runs the proof suite.
pub const MIRI_TARGETS: &[Option<&str>] = if cfg!(target_os = "macos") {
    &[
        None,
        Some("x86_64-unknown-linux-gnu"),
        Some("x86_64-pc-windows-msvc"),
    ]
} else {
    &[None, Some("x86_64-pc-windows-msvc")]
};

/// The selected test limit for one Miri invocation.
const MIRI_BATCH_TESTS: usize = 32;
const MIRI_EXHAUSTIVE_FAMILIES: [&str; 3] = [
    "artifact_rules::tests::sha256_rejects_every_noncanonical_byte_at_every_position_",
    "artifact_rules::tests::device_prefix_membership_matches_the_reference_for_every_byte_at_every_position_",
    "artifact_rules::tests::portable_components_match_the_split_reference_for_all_device_spellings_",
];

/// The crate with undefined behavior that Miri must refuse.
const MIRI_FIXTURE: &str = "xtask/proofs/miri-fixture/Cargo.toml";
const MIRI_CHECKOUT_TARGET: &str = "target/miri";
const MIRI_SYSROOT_CACHE: &str = "miri-sysroots";
const MIRI_SYSROOT_LOCK: &str = "setup.lock";
const MIRI_SYSROOT_DIRECTORY: &str = "sysroot";
const MIRI_CUSTOM_ENVIRONMENT: &[&str] = &[
    "MIRI",
    "MIRI_SYSROOT",
    "MIRI_LIB_SRC",
    "MIRI_NO_STD",
    "MIRIFLAGS",
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTC_STAGE",
    "RUSTC_BOOTSTRAP",
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_TARGET",
    "CARGO_BUILD_RUSTC",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
];

/// A discovered test and its existing ignore state.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MiriTest {
    pub name: String,
    pub ignored: bool,
}

fn miri_events(output: &str) -> Result<Vec<Value>> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("invalid Miri test event"))
        .collect()
}

/// Partitions the complete discovered inventory without changing any ignore state.
///
/// # Errors
///
/// Returns an error for missing, duplicate, empty, or incomplete test discovery.
pub fn miri_batches(output: &str) -> Result<Vec<Vec<MiriTest>>> {
    let mut tests = BTreeMap::new();
    let mut total = 0u64;
    for event in miri_events(output)? {
        match (event["type"].as_str(), event["event"].as_str()) {
            (Some("suite"), Some("discovery")) => {}
            (Some("suite"), Some("completed")) => {
                let count = event["tests"].as_u64().context("missing Miri test count")?;
                ensure!(event["benchmarks"] == 0, "unexpected Miri benchmark");
                total = total
                    .checked_add(count)
                    .context("Miri test count overflow")?;
            }
            (Some("test"), Some("discovered")) => {
                let name = event["name"].as_str().context("missing Miri test name")?;
                let ignored = event["ignore"]
                    .as_bool()
                    .context("missing Miri ignore state")?;
                ensure!(!name.is_empty(), "empty Miri test name");
                ensure!(
                    tests.insert(name.to_owned(), ignored).is_none(),
                    "duplicate Miri test {name}"
                );
            }
            _ => anyhow::bail!("unexpected Miri discovery event: {event}"),
        }
    }
    ensure!(
        !tests.is_empty() && tests.len() as u64 == total,
        "Miri test discovery is empty or incomplete"
    );
    let (isolated, tests): (Vec<_>, Vec<_>) = tests
        .into_iter()
        .map(|(name, ignored)| MiriTest { name, ignored })
        .partition(|test| {
            MIRI_EXHAUSTIVE_FAMILIES
                .iter()
                .any(|family| test.name.starts_with(family))
        });
    Ok(tests
        .chunks(MIRI_BATCH_TESTS)
        .map(<[MiriTest]>::to_vec)
        .chain(isolated.into_iter().map(|test| vec![test]))
        .collect())
}

/// Checks every selected test's completed outcome and ignore state, reporting elapsed time as advisory.
///
/// # Errors
///
/// Returns an error for a missing, duplicate, changed, or failed result.
pub fn validate_miri_batch(output: &str, expected: &[MiriTest], elapsed_ms: u64) -> Result<()> {
    println!("Miri host invocation: {elapsed_ms} ms (advisory)");
    let mut completed = BTreeMap::new();
    let mut suites = 0;
    for event in miri_events(output)? {
        match (event["type"].as_str(), event["event"].as_str()) {
            (Some("suite" | "test"), Some("started")) | (Some("test"), Some("timeout")) => {}
            (Some("test"), Some(outcome @ ("ok" | "ignored"))) => {
                let name = event["name"].as_str().context("missing Miri result name")?;
                ensure!(
                    completed
                        .insert(name.to_owned(), outcome == "ignored")
                        .is_none(),
                    "duplicate Miri result {name}"
                );
            }
            (Some("suite"), Some("ok")) => {
                ensure!(event["failed"] == 0, "Miri test suite failed");
                let seconds = event["exec_time"]
                    .as_number()
                    .context("missing Miri duration")?;
                ensure!(
                    seconds
                        .as_f64()
                        .is_some_and(|seconds| seconds.is_finite() && !seconds.is_sign_negative()),
                    "invalid Miri interpreted duration: {seconds}"
                );
                suites += 1;
            }
            _ => anyhow::bail!("unexpected Miri result event: {event}"),
        }
    }
    let expected_map: BTreeMap<_, _> = expected
        .iter()
        .map(|test| (test.name.clone(), test.ignored))
        .collect();
    ensure!(
        !expected.is_empty()
            && expected_map.len() == expected.len()
            && suites > 0
            && completed == expected_map,
        "Miri test results are missing, unexpected, or changed their ignore state"
    );
    Ok(())
}

fn miri_elapsed_millis(elapsed: std::time::Duration) -> Result<u64> {
    proof_budget_rules::elapsed_millis(
        elapsed.as_millis(),
        elapsed.subsec_nanos() % proof_budget_rules::NANOS_PER_MILLI,
    )
    .context("Miri host elapsed milliseconds overflow")
}

/// The wall time of each test binary that `cargo test` reports, in milliseconds.
#[must_use]
pub fn test_durations(output: &str) -> Vec<Option<u64>> {
    output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("test result: "))
        .map(|result| {
            result
                .rsplit_once("finished in ")
                .and_then(|(_, time)| time.strip_suffix('s'))
                .and_then(millis_of)
        })
        .collect()
}

/// Milliseconds of a decimal count of seconds such as `14.74`.
fn millis_of(seconds: &str) -> Option<u64> {
    const MILLIS_PER_SECOND: u64 = 1000;
    const MILLI_DIGITS: usize = 3;
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let fraction = format!("{fraction:0<MILLI_DIGITS$}");
    let millis: u64 = fraction.get(..MILLI_DIGITS)?.parse().ok()?;
    whole
        .parse::<u64>()
        .ok()?
        .checked_mul(MILLIS_PER_SECOND)?
        .checked_add(millis)
}

fn nightly_cargo(root: &Path) -> std::process::Command {
    let mut command = Tool::Rustup.command();
    command
        .current_dir(root)
        .args(["run", MIRI_TOOLCHAIN, Tool::Cargo.program()]);
    command
}

fn miri(root: &Path, sysroot: Option<&Path>) -> std::process::Command {
    let mut command = nightly_cargo(root);
    let target = std::env::var_os(platform::code_environment::MIRI_TARGET_VARIABLE)
        .map_or_else(|| root.join(MIRI_CHECKOUT_TARGET), std::path::PathBuf::from);
    command
        .args(["miri", "test", "--locked", "--target-dir"])
        .arg(target);
    if let Some(sysroot) = sysroot {
        command.env("MIRI_SYSROOT", sysroot);
    }
    command
}

fn miri_sysroot_namespace(
    root: &Path,
    cache: &Path,
    target: Option<&str>,
) -> Result<std::path::PathBuf> {
    let mut versions = Vec::new();
    for (program, arguments) in [
        (Tool::Rustc.program(), &["-vV"][..]),
        ("miri", &["--version"][..]),
        (Tool::Cargo.program(), &["miri", "--version"][..]),
    ] {
        let output = crate::phase_evidence::output(
            Tool::Rustup
                .command()
                .current_dir(root)
                .args(["run", MIRI_TOOLCHAIN, program])
                .args(arguments),
            "Miri sysroot compiler identity",
        )?;
        ensure!(
            output.status.success() && !output.stdout.is_empty(),
            "cannot identify the pinned Miri compiler: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        versions.push(String::from_utf8(output.stdout)?);
    }
    let host = versions[0]
        .lines()
        .filter_map(|line| line.strip_prefix("host: "))
        .collect::<Vec<_>>();
    ensure!(
        host.len() == 1 && !host[0].is_empty(),
        "the pinned compiler must name one host"
    );
    let identity = serde_json::to_vec(&(
        MIRI_TOOLCHAIN,
        target.unwrap_or(host[0]),
        &versions,
        fs::read(root.join(".cargo/config.toml"))?,
    ))?;
    Ok(cache
        .join(MIRI_SYSROOT_CACHE)
        .join(MIRI_TOOLCHAIN)
        .join(crate::build_inputs::hex(&Sha256::digest(identity))))
}

fn use_miri_sysroot<T>(
    storage: &Path,
    setup: impl FnOnce(&Path) -> Result<()>,
    run: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    fs::create_dir_all(storage)?;
    let lease = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(storage.join(MIRI_SYSROOT_LOCK))?;
    lease.lock()?;
    let sysroot = storage.join(MIRI_SYSROOT_DIRECTORY);
    setup(&sysroot)?;
    run(&sysroot)
}

fn with_miri_sysroot<T>(
    root: &Path,
    target: Option<&str>,
    run: impl FnOnce(Option<&Path>) -> Result<T>,
) -> Result<T> {
    let cache = std::env::var_os("DOTFILES_HOST_CACHE");
    if MIRI_CUSTOM_ENVIRONMENT
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return run(None);
    }
    let Some(cache) = cache else {
        return run(None);
    };
    let cache = std::path::PathBuf::from(cache);
    ensure!(
        cache.is_absolute(),
        "the Miri build cache must be host-local and absolute"
    );
    let storage = miri_sysroot_namespace(root, &cache, target)?;
    use_miri_sysroot(
        &storage,
        |sysroot| {
            let mut command = nightly_cargo(root);
            command.args(["miri", "setup"]).env("MIRI_SYSROOT", sysroot);
            if let Some(target) = target {
                command.args(["--target", target]);
            }
            let output = crate::phase_evidence::output(&mut command, "Miri sysroot setup")?;
            ensure!(
                output.status.success(),
                "Miri sysroot setup failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        },
        |sysroot| run(Some(sysroot)),
    )
}

/// Interprets the pure crates' selected tests under Miri for every target and refuses a fixture with undefined behavior.
pub(crate) fn verify_miri(root: &Path) -> Result<()> {
    ensure!(
        !dotfiles::bounded::requires_container(),
        "native Miri refuses on owner machines; use `just proofs` or `dotfiles bounded run`"
    );
    crate::phase_evidence::run(
        root,
        "Miri targets and UB fixture",
        MIRI_TARGETS.len() * MIRI_CRATES.len() + 1,
        || verify_miri_measured(root),
    )
}

fn verify_miri_measured(root: &Path) -> Result<()> {
    validate_miri_reports(&collect_miri(root)?)?;
    println!(
        "Miri interpreted the tests of {} for {MIRI_TARGETS:?} without undefined behavior, and refused its fixture",
        MIRI_CRATES.join(", ")
    );
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MiriRun {
    selected: Vec<MiriTest>,
    elapsed_ms: u64,
    result: crate::phase_evidence::RawResult,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MiriCase {
    krate: String,
    target: Option<String>,
    discovery: crate::phase_evidence::RawResult,
    runs: Vec<MiriRun>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MiriReports {
    cases: Vec<MiriCase>,
    fixture: crate::phase_evidence::RawResult,
}

pub(crate) fn validate_miri_reports(reports: &MiriReports) -> Result<()> {
    let expected = MIRI_TARGETS
        .iter()
        .flat_map(|target| MIRI_CRATES.iter().map(move |krate| (*krate, *target)))
        .collect::<Vec<_>>();
    ensure!(
        reports.cases.len() == expected.len(),
        "Miri crate and target inventory is incomplete or repeated"
    );
    for (case, (krate, target)) in reports.cases.iter().zip(expected) {
        ensure!(
            case.krate == krate && case.target.as_deref() == target,
            "Miri case identity differs from the current inventory"
        );
        validate_miri_case(case)?;
    }
    validate_miri_fixture(&reports.fixture)
}

fn validate_miri_fixture(result: &crate::phase_evidence::RawResult) -> Result<()> {
    ensure!(
        result.failure() && result.stderr()?.contains("Undefined Behavior"),
        "Miri did not reject the undefined-behavior fixture"
    );
    Ok(())
}

fn validate_miri_case(case: &MiriCase) -> Result<()> {
    ensure!(case.discovery.success(), "Miri inventory discovery failed");
    let expected = miri_batches(case.discovery.stdout()?)?;
    ensure!(
        case.runs.len() == expected.len(),
        "Miri result batch inventory is missing, repeated, or unexpected"
    );
    for (run, selected) in case.runs.iter().zip(expected) {
        ensure!(
            run.selected == selected && run.result.success(),
            "Miri selection or execution differs from its complete inventory"
        );
        validate_miri_batch(run.result.stdout()?, &selected, run.elapsed_ms)?;
    }
    Ok(())
}

pub(crate) fn collect_miri(root: &Path) -> Result<MiriReports> {
    let mut checks = Checks::default();
    let mut cases = Vec::new();
    for &target in MIRI_TARGETS {
        for krate in MIRI_CRATES {
            checks.run(
                &format!("Miri {krate} for {}", target.unwrap_or("the host")),
                || {
                    let case = collect_miri_case(root, krate, target)?;
                    validate_miri_case(&case)?;
                    cases.push(case);
                    Ok(())
                },
            );
        }
    }
    let mut fixture = None;
    checks.run("Miri fixture", || {
        with_miri_sysroot(root, None, |sysroot| {
            let output = crate::phase_evidence::output(
                miri(root, sysroot)
                    .arg("--manifest-path")
                    .arg(root.join(MIRI_FIXTURE)),
                "Miri undefined-behavior fixture",
            )?;
            let raw = crate::phase_evidence::RawResult::from(output);
            validate_miri_fixture(&raw)?;
            fixture = Some(raw);
            Ok(())
        })
    });
    checks.finish()?;
    Ok(MiriReports {
        cases,
        fixture: fixture.context("Miri fixture did not complete")?,
    })
}

fn collect_miri_case(root: &Path, krate: &str, target: Option<&str>) -> Result<MiriCase> {
    with_miri_sysroot(root, target, |sysroot| {
        let mut command = miri(root, sysroot);
        command.args(["--package", krate]);
        if let Some(target) = target {
            command.args(["--target", target]);
        }
        let label = format!("Miri {krate} for {}", target.unwrap_or("host"));
        let output = crate::phase_evidence::output(
            command.args(["--", "--list", "--format", "json", "-Z", "unstable-options"]),
            &format!("{label} inventory"),
        )?;
        ensure!(
            output.status.success(),
            "cannot list Miri tests of {krate} for {}:\n{}",
            target.unwrap_or("the host"),
            String::from_utf8_lossy(&output.stderr)
        );
        let discovery = crate::phase_evidence::RawResult::from(output);
        let batches = miri_batches(discovery.stdout()?)?;
        let mut checks = Checks::default();
        let mut runs = Vec::new();
        for batch in batches {
            let names: Vec<_> = batch.iter().map(|test| test.name.as_str()).collect();
            checks.run(&format!("tests [{}]", names.join(", ")), || {
            let mut command = miri(root, sysroot);
            command.args(["--package", krate]);
            if let Some(target) = target {
                command.args(["--target", target]);
            }
            let started = std::time::Instant::now();
            let output = crate::phase_evidence::output(command
                .args([
                    "--",
                    "--format",
                    "json",
                    "-Z",
                    "unstable-options",
                    "--exact",
                ])
                .args(&names), &format!("{label} [{}]", names.join(", ")))?;
            let elapsed_ms = miri_elapsed_millis(started.elapsed())?;
            let report = String::from_utf8_lossy(&output.stdout);
            ensure!(
                output.status.success(),
                "Miri refused the tests of {krate} for {}:\n{report}\n{}",
                target.unwrap_or("the host"),
                String::from_utf8_lossy(&output.stderr)
            );
            validate_miri_batch(&report, &batch, elapsed_ms)?;
            let ignored = batch.iter().filter(|test| test.ignored).count();
            println!(
                "Miri verified {} tests of {krate} for {} and retained {ignored} existing ignores",
                batch.len() - ignored,
                target.unwrap_or("the host")
            );
            runs.push(MiriRun { selected: batch.clone(), elapsed_ms, result: output.into() });
            Ok(())
        });
        }
        checks.finish()?;
        Ok(MiriCase {
            krate: krate.into(),
            target: target.map(str::to_owned),
            discovery,
            runs,
        })
    })
}

/// Verifies one rule module and its probes, and returns how many harnesses it proved and how many probes it rejected.
/// The gate refuses compiler metadata that differs from the harnesses the source declares, so the closure check and the verifier agree.
#[cfg(all(test, not(windows)))]
fn verify_rules(
    root: &Path,
    source: &str,
    probes: &str,
    flags: &BTreeSet<&'static str>,
    verifier_hash: &str,
    engine_hash: &str,
) -> Result<(usize, usize)> {
    execute_rules(prepare_rules(
        root,
        source,
        probes,
        flags,
        verifier_hash,
        engine_hash,
    )?)
}

#[derive(Debug)]
enum PreparedRules {
    Reused { proved: usize, rejected: usize },
    Compiled(Box<CompiledRules>),
}

#[derive(Debug)]
struct CompiledRules {
    root: PathBuf,
    source: String,
    path: PathBuf,
    original: Vec<u8>,
    directory: tempfile::TempDir,
    probe: tempfile::TempDir,
    probe_file: PathBuf,
    declared: Vec<String>,
    rejected: Vec<String>,
    flags: BTreeSet<&'static str>,
    input: crate::phase_evidence::KaniInput,
}

fn prepare_rules(
    root: &Path,
    source: &str,
    probes: &str,
    flags: &BTreeSet<&'static str>,
    verifier_hash: &str,
    engine_hash: &str,
) -> Result<PreparedRules> {
    let path = crate::canonical(&root.join(source))?;
    let original = fs::read(&path)?;
    let declared = obligations::module(&String::from_utf8(original.clone())?)?.harnesses;
    let rejected_names: Vec<String> = obligations::probe_harnesses(probes)?.into_iter().collect();
    let declared_names: Vec<String> = declared.iter().cloned().collect();
    let input =
        crate::phase_evidence::kani_input(root, source, probes, flags, verifier_hash, engine_hash)?;
    if let Some(origin) =
        crate::phase_evidence::cached_kani(root, &input, &declared_names, &rejected_names)?
    {
        println!(
            "Reused verified Kani subject {source}: {} production harnesses and {} rejected probes",
            declared.len(),
            rejected_names.len()
        );
        let duration = origin
            .harness_duration_ms()
            .map_or_else(|| "unmeasured".to_owned(), |millis| format!("{millis} ms"));
        println!(
            "Kani origin sha256:{}, original harness duration={duration} (advisory); current readback reran all result decoders",
            origin.identity(),
        );
        return Ok(PreparedRules::Reused {
            proved: declared.len(),
            rejected: rejected_names.len(),
        });
    }
    crate::phase_evidence::begin_kani(root, &input)?;
    let directory = tempfile::tempdir()?;
    let harnesses = verify_inventory(&path, directory.path(), flags)?;
    ensure!(
        harnesses == declared,
        "Kani lists other harnesses in {source} than the source declares: {harnesses:?} against {declared:?}"
    );
    let probe = tempfile::tempdir()?;
    let probe_file = probe.path().join("counterexample.rs");
    fs::write(
        &probe_file,
        format!(
            "#[path = {:?}]\nmod production;\n{probes}",
            path.to_string_lossy()
        ),
    )?;
    let compiled_probes = verify_inventory(&probe_file, probe.path(), flags)?;
    let expected_probes: BTreeSet<_> = declared
        .iter()
        .map(|name| format!("production::{name}"))
        .chain(rejected_names.iter().cloned())
        .collect();
    ensure!(
        compiled_probes == expected_probes,
        "Kani lists other counterexample harnesses for {source} than its complete production and rejecting inventory"
    );
    Ok(PreparedRules::Compiled(Box::new(CompiledRules {
        root: root.to_owned(),
        source: source.to_owned(),
        path,
        original,
        directory,
        probe,
        probe_file,
        declared: declared_names,
        rejected: rejected_names,
        flags: flags.clone(),
        input,
    })))
}

fn execute_rules(prepared: PreparedRules) -> Result<(usize, usize)> {
    let compiled = match prepared {
        PreparedRules::Compiled(compiled) => compiled,
        PreparedRules::Reused { proved, rejected } => return Ok((proved, rejected)),
    };
    let CompiledRules {
        root,
        source,
        path,
        original,
        directory,
        probe,
        probe_file,
        declared,
        rejected,
        flags,
        input,
    } = *compiled;
    let names: Vec<&str> = declared.iter().map(String::as_str).collect();
    let mut checks = Checks::default();
    let mut reports = Vec::new();
    checks.run("production harnesses", || {
        reports.extend(verify_file_recorded(
            &path,
            directory.path(),
            &names,
            false,
            &flags,
        )?);
        Ok(())
    });
    checks.run("counterexamples", || {
        // Each bounded receipt must reject its exact probe on an assertion.
        let probe_names: Vec<&str> = rejected.iter().map(String::as_str).collect();
        if !probe_names.is_empty() {
            reports.extend(verify_file_recorded(
                &probe_file,
                probe.path(),
                &probe_names,
                true,
                &flags,
            )?);
        }
        Ok(())
    });
    checks.run("source binding", || {
        ensure!(
            fs::read(&path)? == original,
            "production proof source {source} changed during verification"
        );
        Ok(())
    });
    checks.finish()?;
    crate::phase_evidence::publish_kani(&root, input, reports, &declared, &rejected)?;
    Ok((names.len(), rejected.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_incomplete_harness_preserves_every_individual_outcome() {
        use proof_budget_rules::Verdict;

        let mut value = serde_json::json!({"verification_results": {
            "summary": {"status": "completed", "executed": 2, "successful": 1, "failed": 0},
            "results": [
                {"harness_id": "valid", "status": "Success", "checks": [
                    {"category": "assertion", "status": "Success"},
                    {"category": "cover", "status": "Satisfied"},
                    {"category": "cover", "status": "Satisfied"}
                ]},
                {"harness_id": "timed_out", "status": "Timeout", "checks": []}
            ]
        }});
        for reversed in [false, true] {
            if reversed {
                value["verification_results"]["results"]
                    .as_array_mut()
                    .unwrap()
                    .reverse();
            }
            let mut reported = Vec::new();
            let error = validate_results_reported(
                &value,
                &["valid", "timed_out"],
                false,
                |row, verdict| {
                    reported.push((row["harness_id"].as_str().unwrap().to_owned(), verdict));
                },
            )
            .unwrap_err();
            assert!(format!("{error:#}").contains("inconclusive"));
            if reversed {
                reported.reverse();
            }
            assert_eq!(
                reported,
                [
                    ("valid".to_owned(), Verdict::Proven),
                    ("timed_out".to_owned(), Verdict::Inconclusive)
                ]
            );
        }
    }

    #[test]
    fn a_missing_report_names_every_unvalidated_harness() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let expected = ["not_started_first", "not_started_last"];
        let mut command = Tool::Git.command();
        command.arg("--version");
        let error = verify_batch_command(
            command,
            Path::new("unwritten-report.rs"),
            directory.path(),
            &expected,
            false,
            &BTreeSet::new(),
        )
        .unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("missing Kani report"), "{diagnostic}");
        for name in expected {
            assert!(diagnostic.contains(name), "{diagnostic}");
        }
        assert!(diagnostic.contains("inconclusive"), "{diagnostic}");
        Ok(())
    }

    #[test]
    fn selected_sources_keep_their_probes_and_refuse_unknown_or_duplicate_modules() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let source = "crates/rules/src/proof_budget_rules.rs".to_owned();
        let manifest = selected_manifest(root, std::slice::from_ref(&source))?;
        assert!(matches!(
            manifest.obligations.as_slice(),
            [Obligation::Kani { source: actual, probes, .. }]
                if actual == &source && !probes.is_empty()
        ));
        assert!(selected_manifest(root, &[source.clone(), source]).is_err());
        for invalid in ["missing.rs", "xtask/src/skill_proofs.rs", "../outside.rs"] {
            assert!(selected_manifest(root, &[invalid.to_owned()]).is_err());
        }
        let (selected, _) = campaign_workload(root, &manifest)?;
        let (full, _) = campaign_workload(root, &selected_manifest(root, &[])?)?;
        assert!(selected > 0 && selected < full);
        let expected = campaign_harnesses(root, &manifest)?;
        assert_eq!(expected.len(), selected as usize);
        assert_eq!(
            expected.iter().collect::<BTreeSet<_>>().len(),
            expected.len()
        );
        assert!(
            expected
                .iter()
                .any(|name| name.contains("::counterexample::"))
        );
        assert!(expected.iter().any(|name| name.contains("::proofs::")));
        assert!(
            expected
                .iter()
                .all(|name| name.starts_with("crates/rules/src/proof_budget_rules.rs::"))
        );
        let complete = campaign_harnesses(root, &selected_manifest(root, &[])?)?;
        assert_eq!(complete.len(), full as usize);
        assert!(
            campaign_harnesses(
                root,
                &obligations::Manifest {
                    obligations: Vec::new(),
                    pilots: Vec::new(),
                    non_terminating: Vec::new(),
                }
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn selected_container_sources_remain_literal_shell_arguments() {
        let source = "a'$(touch unexpected).rs".to_owned();
        let arguments = container_campaign_arguments(
            Path::new("/fixture"),
            "fixture-image",
            "fixture-cache",
            ProofCampaign::Ci,
            &[source],
        );
        assert!(
            arguments
                .last()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("proofs --source 'a'\\''$(touch unexpected).rs'")
        );
    }

    #[test]
    fn the_fuzz_compile_barrier_uses_real_compiler_status_before_heavy_work() -> Result<()> {
        use std::cell::Cell;

        const SOURCE_FILE: &str = "fuzz_compile_fixture.rs";
        const OUTPUT_FILE: &str = "fuzz_compile_fixture.rmeta";
        const MISSING_IMPORT: &str = "use anyhow::Result;\nfn main() {}\n";
        const VALID_SOURCE: &str = "fn main() {}\n";
        let scope = tempfile::tempdir()?;
        let result = (|| {
            for (valid, source) in [(false, MISSING_IMPORT), (true, VALID_SOURCE)] {
                let input = scope.path().join(SOURCE_FILE);
                fs::write(&input, source)?;
                let output = Tool::Rustc
                    .command()
                    .current_dir(scope.path())
                    .args(["--edition", "2024", "--emit", "metadata"])
                    .arg(&input)
                    .arg("-o")
                    .arg(scope.path().join(OUTPUT_FILE))
                    .output()
                    .context("compile the actual owned import fixture")?;
                assert_eq!(output.status.success(), valid);
                let calls = Cell::new(0_usize);
                let guarded = with_fuzz_compilation(
                    || require_fuzz_compilation(&output),
                    || {
                        calls.set(calls.get() + 1);
                        Ok(())
                    },
                );
                assert_eq!(calls.get(), usize::from(valid));
                if valid {
                    guarded?;
                } else {
                    let error = guarded.expect_err("a real compiler failure must block heavy work");
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    assert!(stderr.contains("E0432"), "{stderr}");
                    assert!(format!("{error:#}").contains(stderr.as_ref()));
                }
            }
            Ok(())
        })();
        platform::cleanup::report(scope.close(), "remove the owned fuzz compilation fixture");
        result
    }

    fn rule_subjects_fixture() -> obligations::Manifest {
        obligations::Manifest {
            obligations: ["first", "broken", "last"]
                .map(|source| Obligation::Kani {
                    source: source.to_owned(),
                    probes: String::new(),
                    flags: Vec::new(),
                })
                .into(),
            pilots: Vec::new(),
            non_terminating: Vec::new(),
        }
    }

    #[test]
    fn a_rule_compile_failure_collects_all_inputs_before_any_proof_body() {
        let mut compiled = Vec::new();
        let mut executed = Vec::new();
        let failure = run_rule_subjects(
            &rule_subjects_fixture(),
            |source, _| {
                compiled.push(source.to_owned());
                ensure!(source != "broken", "compiler rejected the required harness");
                Ok(source.to_owned())
            },
            |source| {
                executed.push(source);
                Ok((1, 2))
            },
        )
        .unwrap_err();
        assert_eq!(compiled, ["first", "broken", "last"]);
        assert_eq!(executed, Vec::<String>::new());
        assert!(format!("{failure:#}").contains("compiler rejected the required harness"));
    }

    #[test]
    fn every_rule_is_prepared_before_execution_and_body_failures_are_collected() {
        let operations = std::cell::RefCell::new(Vec::new());
        let failure = run_rule_subjects(
            &rule_subjects_fixture(),
            |source, _| {
                operations.borrow_mut().push(format!("prepare:{source}"));
                Ok(source.to_owned())
            },
            |source| {
                operations.borrow_mut().push(format!("execute:{source}"));
                ensure!(source == "first", "proof body refused {source}");
                Ok((1, 2))
            },
        )
        .unwrap_err();
        assert_eq!(
            *operations.borrow(),
            [
                "prepare:first",
                "prepare:broken",
                "prepare:last",
                "execute:first",
                "execute:broken",
                "execute:last"
            ]
        );
        let diagnostic = format!("{failure:#}");
        assert!(diagnostic.contains("proof body refused broken"));
        assert!(diagnostic.contains("proof body refused last"));
    }

    #[test]
    fn an_empty_rule_campaign_cannot_supply_success() {
        let mut manifest = rule_subjects_fixture();
        manifest.obligations.clear();
        let mut executed = 0;
        let failure = run_rule_subjects(
            &manifest,
            |_, _| Ok(()),
            |()| {
                executed += 1;
                Ok((0, 0))
            },
        )
        .unwrap_err();
        assert_eq!(executed, 0);
        assert!(format!("{failure:#}").contains("no required rule subjects"));
    }

    fn raw_result(
        status: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> crate::phase_evidence::RawResult {
        crate::phase_evidence::RawResult {
            status,
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn miri_reports_fixture() -> MiriReports {
        let discovery = "{\"type\":\"suite\",\"event\":\"discovery\"}\n{\"type\":\"test\",\"event\":\"discovered\",\"name\":\"tested\",\"ignore\":false}\n{\"type\":\"suite\",\"event\":\"completed\",\"tests\":1,\"benchmarks\":0}\n";
        let result = "{\"type\":\"suite\",\"event\":\"started\"}\n{\"type\":\"test\",\"event\":\"ok\",\"name\":\"tested\"}\n{\"type\":\"suite\",\"event\":\"ok\",\"failed\":0,\"exec_time\":0.001}\n";
        let cases = MIRI_TARGETS
            .iter()
            .flat_map(|target| {
                MIRI_CRATES.iter().map(move |krate| MiriCase {
                    krate: (*krate).into(),
                    target: target.map(str::to_owned),
                    discovery: raw_result(Some(0), discovery, ""),
                    runs: vec![MiriRun {
                        selected: vec![MiriTest {
                            name: "tested".into(),
                            ignored: false,
                        }],
                        elapsed_ms: 1,
                        result: raw_result(Some(0), result, ""),
                    }],
                })
            })
            .collect();
        MiriReports {
            cases,
            fixture: raw_result(Some(1), "", "Undefined Behavior"),
        }
    }

    #[test]
    fn miri_reports_revalidate_every_target_inventory_selection_and_fixture() -> Result<()> {
        let valid = miri_reports_fixture();
        validate_miri_reports(&valid)?;
        let mut invalid = valid.clone();
        invalid.cases.pop();
        assert!(validate_miri_reports(&invalid).is_err());
        let mut invalid = valid.clone();
        invalid.cases[0].target = Some("unsupported-target".into());
        assert!(validate_miri_reports(&invalid).is_err());
        let mut invalid = valid.clone();
        let repeated = invalid.cases[0].runs[0].clone();
        invalid.cases[0].runs.push(repeated);
        assert!(validate_miri_reports(&invalid).is_err());
        let mut invalid = valid.clone();
        invalid.cases[0].runs[0].selected[0].ignored = true;
        assert!(validate_miri_reports(&invalid).is_err());
        let mut invalid = valid.clone();
        invalid.cases[0].runs[0].result.status = None;
        assert!(validate_miri_reports(&invalid).is_err());
        let mut invalid = valid.clone();
        invalid.cases[0].runs[0].elapsed_ms = u64::MAX;
        validate_miri_reports(&invalid)?;
        let mut missing = serde_json::to_value(&valid)?;
        missing["cases"][0]["runs"][0]
            .as_object_mut()
            .unwrap()
            .remove("elapsed_ms");
        assert!(serde_json::from_value::<MiriReports>(missing).is_err());
        for elapsed in [
            serde_json::Value::Null,
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::from_str("18446744073709551616")?,
        ] {
            let mut invalid = serde_json::to_value(&valid)?;
            invalid["cases"][0]["runs"][0]["elapsed_ms"] = elapsed;
            assert!(serde_json::from_value::<MiriReports>(invalid).is_err());
        }
        let mut invalid = serde_json::to_value(&valid)?;
        invalid["cases"][0]["runs"][0]["unexpected"] = serde_json::json!(1);
        assert!(serde_json::from_value::<MiriReports>(invalid).is_err());
        let mut invalid = valid.clone();
        invalid.fixture.status = None;
        assert!(validate_miri_reports(&invalid).is_err());
        let mut invalid = valid;
        invalid.fixture.stderr = b"compiler failed".to_vec();
        assert!(validate_miri_reports(&invalid).is_err());
        Ok(())
    }

    #[test]
    fn miri_host_elapsed_rounds_up_and_refuses_overflow() -> Result<()> {
        use std::time::Duration;
        for (elapsed, expected) in [
            (Duration::ZERO, 0),
            (Duration::from_nanos(1), 1),
            (Duration::from_nanos(999_999), 1),
            (Duration::from_millis(1), 1),
            (Duration::from_nanos(999_999_999), 1_000),
            (Duration::from_secs(1), 1_000),
            (Duration::from_secs(30), 30_000),
            (Duration::from_secs(30) + Duration::from_nanos(1), 30_001),
            (Duration::from_millis(u64::MAX), u64::MAX),
        ] {
            assert_eq!(miri_elapsed_millis(elapsed)?, expected);
        }
        assert!(
            miri_elapsed_millis(Duration::from_millis(u64::MAX) + Duration::from_nanos(1)).is_err()
        );
        assert!(miri_elapsed_millis(Duration::MAX).is_err());
        assert!(miri_elapsed_millis(Duration::from_secs(u64::MAX / 1_000 + 1)).is_err());
        Ok(())
    }

    #[test]
    fn mutation_origins_redecode_raw_identity_inventory_and_terminal_outcomes() -> Result<()> {
        let (inventory, outcomes) = mutation_report_fixture();
        let valid = MutationOrigin {
            package: "rules".into(),
            status: Some(proof_budget_rules::MUTATION_TIMEOUT_EXIT),
            inventory: serde_json::to_vec(&inventory)?,
            outcomes: serde_json::to_vec(&outcomes)?,
        };
        decode_mutation_origin(&valid, "rules")?;
        for invalid in [
            MutationOrigin {
                package: "foreign".into(),
                ..valid.clone()
            },
            MutationOrigin {
                status: None,
                ..valid.clone()
            },
            MutationOrigin {
                inventory: b"[]".to_vec(),
                ..valid.clone()
            },
            MutationOrigin {
                outcomes: b"{}".to_vec(),
                ..valid
            },
        ] {
            assert!(decode_mutation_origin(&invalid, "rules").is_err());
        }
        Ok(())
    }

    #[test]
    fn fuzz_reports_bind_the_complete_current_corpus_target_and_rejecting_inventory() -> Result<()>
    {
        let expected = FuzzInventory {
            host: "fixture-host".into(),
            targets: vec![("parser".into(), "corpus-hash".into())],
            fixture_corpus_hash: "rejecting-corpus".into(),
        };
        let valid = FuzzReports {
            version: raw_result(Some(0), &format!("cargo-fuzz {FUZZ_VERSION}"), ""),
            host: "fixture-host".into(),
            runs: vec![FuzzRun {
                target: "parser".into(),
                corpus_hash: "corpus-hash".into(),
                result: raw_result(Some(0), "", ""),
            }],
            fixture_corpus_hash: "rejecting-corpus".into(),
            fixture: raw_result(Some(1), "", "panicked"),
        };
        validate_fuzz_reports(&expected, &valid)?;
        let mut invalid = valid.clone();
        invalid.runs.clear();
        assert!(validate_fuzz_reports(&expected, &invalid).is_err());
        let mut invalid = valid.clone();
        invalid.runs.push(invalid.runs[0].clone());
        assert!(validate_fuzz_reports(&expected, &invalid).is_err());
        let mut invalid = valid.clone();
        invalid.runs[0].corpus_hash = "stale".into();
        assert!(validate_fuzz_reports(&expected, &invalid).is_err());
        let mut invalid = valid.clone();
        invalid.host = "foreign-target".into();
        assert!(validate_fuzz_reports(&expected, &invalid).is_err());
        let mut invalid = valid;
        invalid.fixture.status = None;
        assert!(validate_fuzz_reports(&expected, &invalid).is_err());
        Ok(())
    }

    #[test]
    fn compiler_inventory_keeps_standard_and_contract_harnesses_and_refuses_false_rows()
    -> Result<()> {
        let proof =
            serde_json::json!({"pretty_name":"proofs::valid","attributes":{"kind":"Proof"}});
        let contract = serde_json::json!({"pretty_name":"proofs::contract","attributes":{"kind":{"ProofForContract":{"target_fn":"production"}}}});
        let test = serde_json::json!({"pretty_name":"tests::unit","attributes":{"kind":"Test"}});
        let decode = |rows: Value| {
            compiled_harnesses(&serde_json::to_vec(
                &serde_json::json!({"proof_harnesses":rows}),
            )?)
        };
        assert_eq!(
            decode(serde_json::json!([proof, contract, test]))?,
            BTreeSet::from(["proofs::contract".into(), "proofs::valid".into()])
        );
        for invalid in [
            serde_json::json!([]),
            serde_json::json!([test]),
            serde_json::json!([proof, proof]),
            serde_json::json!([{"pretty_name":"","attributes":{"kind":"Proof"}}]),
            serde_json::json!([{"pretty_name":"unexpected","attributes":{"kind":"Unknown"}}]),
            serde_json::json!([{"pretty_name":"contract","attributes":{"kind":{"ProofForContract":{"target_fn":""}}}}]),
        ] {
            assert!(decode(invalid).is_err());
        }
        assert!(compiled_harnesses(b"{}").is_err());
        assert!(compiled_harnesses(b"not-json").is_err());
        Ok(())
    }

    #[test]
    fn kani_version_accepts_only_the_complete_pinned_standalone_response() {
        assert!(kani_environment_version_matches(
            "Kani Rust Verifier 0.68.0 (standalone)\nCBMC 6.11.0\n"
        ));
        for invalid in [
            "Kani Rust Verifier 0.68.0",
            "Kani Rust Verifier 0.68.0 (standalone)",
            "Kani Rust Verifier 0.68.1 (standalone)\nCBMC 6.11.0",
            "Kani Rust Verifier 0.68.0 (cargo plugin)\nCBMC 6.11.0",
            "Kani Rust Verifier 0.68.0 (standalone)\nCBMC 6.10.0",
            "Kani Rust Verifier 0.68.0 (standalone)\nCBMC 6.11.0\nunexpected",
        ] {
            assert!(!kani_environment_version_matches(invalid), "{invalid}");
        }
    }

    #[test]
    #[ignore = "the bounded proof campaign or native CI runs this Kani regression"]
    #[cfg(not(target_os = "windows"))]
    fn notify_chain_decisions_and_their_counterexamples_verify_against_production() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let manifest = obligations::manifest(root)?;
        let source = "crates/rules/src/notify_chain_rules.rs";
        let probes = manifest
            .obligations
            .iter()
            .find_map(|obligation| match obligation {
                Obligation::Kani {
                    source: candidate,
                    probes,
                    ..
                } if candidate == source => Some(probes),
                _ => None,
            })
            .context("notify chains have a registered proof obligation")?;
        kani_environment_version(root)?;
        let verifier_hash = crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
        let engine_hash = crate::phase_evidence::engine_digest(root)?;
        assert_eq!(
            verify_rules(
                root,
                source,
                probes,
                &flags(&manifest, source),
                &verifier_hash,
                &engine_hash
            )?,
            (4, 3)
        );
        Ok(())
    }

    #[test]
    #[ignore = "the bounded proof campaign or native CI runs this Kani regression"]
    #[cfg(not(target_os = "windows"))]
    fn freshness_report_decisions_and_their_counterexamples_verify_against_production() -> Result<()>
    {
        const EXPECTED: (usize, usize) = (5, 11);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        crate::phase_evidence::run(
            root,
            "freshness subject regression",
            EXPECTED.0 + EXPECTED.1,
            || {
                let manifest = obligations::manifest(root)?;
                let source = "crates/rules/src/freshness_rules.rs";
                let probes = manifest
                    .obligations
                    .iter()
                    .find_map(|obligation| match obligation {
                        Obligation::Kani {
                            source: candidate,
                            probes,
                            ..
                        } if candidate == source => Some(probes),
                        _ => None,
                    })
                    .context("freshness decisions have a registered proof obligation")?;
                kani_environment_version(root)?;
                verify_kani_driver(root)?;
                let verifier_hash =
                    crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
                let engine_hash = crate::phase_evidence::engine_digest(root)?;
                let result = verify_rules(
                    root,
                    source,
                    probes,
                    &flags(&manifest, source),
                    &verifier_hash,
                    &engine_hash,
                )?;
                ensure!(
                    result == EXPECTED,
                    "freshness proof inventory differs: {result:?}"
                );
                Ok(())
            },
        )
    }
    #[cfg(not(target_os = "windows"))]
    fn declared_inventory(root: &Path, source: &str, probes: &str) -> Result<(usize, usize)> {
        let production = obligations::module(&fs::read_to_string(root.join(source))?)?;
        let rejecting = obligations::probe_harnesses(probes)?;
        ensure!(
            !production.harnesses.is_empty() && !rejecting.is_empty(),
            "a subject regression requires production and rejecting harnesses"
        );
        Ok((production.harnesses.len(), rejecting.len()))
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn shape_decisions_and_their_counterexamples_verify_against_production() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let manifest = obligations::manifest(root)?;
        let source = "crates/rules/src/shape_rules.rs";
        let probes = manifest
            .obligations
            .iter()
            .find_map(|obligation| match obligation {
                Obligation::Kani {
                    source: candidate,
                    probes,
                    ..
                } if candidate == source => Some(probes),
                _ => None,
            })
            .context("shape decisions have a registered proof obligation")?;
        let expected = declared_inventory(root, source, probes)?;
        crate::phase_evidence::run(
            root,
            "shape subject regression",
            expected.0 + expected.1,
            || {
                kani_environment_version(root)?;
                verify_kani_driver(root)?;
                let verifier_hash =
                    crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
                let engine_hash = crate::phase_evidence::engine_digest(root)?;
                let result = verify_rules(
                    root,
                    source,
                    probes,
                    &flags(&manifest, source),
                    &verifier_hash,
                    &engine_hash,
                )?;
                ensure!(
                    result == expected,
                    "shape proof inventory differs: {result:?}"
                );
                Ok(())
            },
        )
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn prose_layout_decisions_and_their_counterexamples_verify_against_production() -> Result<()> {
        const EXPECTED: (usize, usize) = (11, 15);
        const TEST: &str = "skill_proofs::tests::prose_layout_decisions_and_their_counterexamples_verify_against_production";
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        if dotfiles::bounded::requires_container() {
            ensure_linux_engine()?;
            let image = proof_image(root)?;
            require_image(&image)?;
            let image = verified_image_id(&image)?;
            crate::container::prepare_cargo(root, &image)?;
            let mut arguments =
                container_run_arguments(root, &image, &container_target_volume(root));
            *arguments.last_mut().context("proof container command")? = format!(
                "{CONTAINER_SOURCE} exec cargo test --locked --package xtask --lib -- --ignored --exact {TEST} --nocapture"
            ).into();
            let output = crate::phase_evidence::output(
                Tool::Docker.command().args(arguments),
                "bounded prose layout proofs",
            )?;
            ensure!(
                output.status.success(),
                "bounded prose layout proofs failed"
            );
            return Ok(());
        }
        crate::phase_evidence::run(
            root,
            "prose layout subject regression",
            EXPECTED.0 + EXPECTED.1,
            || {
                let manifest = obligations::manifest(root)?;
                let source = "crates/rules/src/prose_rules.rs";
                let probes = manifest
                    .obligations
                    .iter()
                    .find_map(|obligation| match obligation {
                        Obligation::Kani {
                            source: candidate,
                            probes,
                            ..
                        } if candidate == source => Some(probes),
                        _ => None,
                    })
                    .context("prose layout decisions have a registered proof obligation")?;
                kani_environment_version(root)?;
                verify_kani_driver(root)?;
                let verifier_hash =
                    crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
                let engine_hash = crate::phase_evidence::engine_digest(root)?;
                let result = verify_rules(
                    root,
                    source,
                    probes,
                    &flags(&manifest, source),
                    &verifier_hash,
                    &engine_hash,
                )?;
                ensure!(
                    result == EXPECTED,
                    "prose layout proof inventory differs: {result:?}"
                );
                Ok(())
            },
        )
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn reply_language_decisions_and_their_counterexamples_verify_against_production() -> Result<()>
    {
        const EXPECTED: (usize, usize) = (9, 4);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        crate::phase_evidence::run(
            root,
            "reply language subject regression",
            EXPECTED.0 + EXPECTED.1,
            || {
                let manifest = obligations::manifest(root)?;
                let source = "crates/rules/src/reply_language_rules.rs";
                let probes = manifest
                    .obligations
                    .iter()
                    .find_map(|obligation| match obligation {
                        Obligation::Kani {
                            source: candidate,
                            probes,
                            ..
                        } if candidate == source => Some(probes),
                        _ => None,
                    })
                    .context("reply language decisions have a registered proof obligation")?;
                kani_environment_version(root)?;
                verify_kani_driver(root)?;
                let verifier_hash =
                    crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
                let engine_hash = crate::phase_evidence::engine_digest(root)?;
                let result = verify_rules(
                    root,
                    source,
                    probes,
                    &flags(&manifest, source),
                    &verifier_hash,
                    &engine_hash,
                )?;
                ensure!(
                    result == EXPECTED,
                    "reply language proof inventory differs: {result:?}"
                );
                Ok(())
            },
        )
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn review_decisions_and_their_counterexamples_verify_against_production() -> Result<()> {
        const SUBJECTS: [(&str, (usize, usize)); 3] = [
            ("crates/rules/src/review_routing_rules.rs", (4, 10)),
            ("crates/rules/src/review_completion_rules.rs", (2, 1)),
            ("crates/rules/src/pr_rules.rs", (8, 22)),
        ];
        const EXPECTED: (usize, usize) = (14, 33);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("the proof test has a workspace root")?;
        crate::phase_evidence::run(
            root,
            "review subject regression",
            EXPECTED.0 + EXPECTED.1,
            || {
                let source_hash = platform::build_inputs::content_hash(root)?;
                ensure!(
                    source_hash == platform::BUILD_HASH,
                    "review proof producer is stale for the current source"
                );
                let manifest = obligations::manifest(root)?;
                let closure = obligations::closure(root, &manifest)?;
                ensure!(
                    closure.refusals.is_empty(),
                    "review proof inventory is not closed:\n{}",
                    closure.refusals.join("\n")
                );
                kani_environment_version(root)?;
                verify_kani_driver(root)?;
                let verifier_hash =
                    crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
                let engine_hash = crate::phase_evidence::engine_digest(root)?;
                let mut actual = (0, 0);
                let mut checks = Checks::default();
                for (source, expected) in SUBJECTS {
                    checks.run(source, || {
                        let probes = manifest
                            .obligations
                            .iter()
                            .find_map(|obligation| match obligation {
                                Obligation::Kani {
                                    source: candidate,
                                    probes,
                                    ..
                                } if candidate == source => Some(probes),
                                _ => None,
                            })
                            .context(
                                "the selected review source has a registered Kani obligation",
                            )?;
                        let result = verify_rules(
                            root,
                            source,
                            probes,
                            &flags(&manifest, source),
                            &verifier_hash,
                            &engine_hash,
                        )?;
                        ensure!(
                            result == expected,
                            "review proof inventory differs for {source}: {result:?}"
                        );
                        actual.0 += result.0;
                        actual.1 += result.1;
                        Ok(())
                    });
                }
                checks.run("source freshness", || {
                    ensure!(
                        platform::build_inputs::content_hash(root)? == source_hash
                            && crate::phase_evidence::engine_digest(root)? == engine_hash,
                        "review proof source or engine inputs changed during verification"
                    );
                    Ok(())
                });
                checks.finish()?;
                ensure!(
                    actual == EXPECTED,
                    "review subject proof execution is incomplete: {actual:?}"
                );
                Ok(())
            },
        )
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn application_and_replacement_rules_verify_against_production() -> Result<()> {
        const SOURCES: [&str; 2] = [
            "crates/rules/src/profile_rules.rs",
            "crates/rules/src/aside_rules.rs",
        ];
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("the proof test has a workspace root")?;
        let source_hash = platform::build_inputs::content_hash(root)?;
        ensure!(
            source_hash == platform::BUILD_HASH,
            "application proof producer is stale for the current source"
        );
        let mut manifest = obligations::manifest(root)?;
        let closure = obligations::closure(root, &manifest)?;
        ensure!(
            closure.refusals.is_empty(),
            "application proof inventory is not closed:\n{}",
            closure.refusals.join("\n")
        );
        manifest.obligations.retain(|obligation| {
            matches!(obligation, Obligation::Kani { source, .. } if SOURCES.contains(&source.as_str()))
        });
        ensure!(manifest.obligations.len() == SOURCES.len());
        verify_kani(root, &manifest)?;
        ensure!(
            platform::build_inputs::content_hash(root)? == source_hash,
            "application proof source changed during verification"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn acquired_material_and_mac_profile_rules_verify_against_production() -> Result<()> {
        const SOURCES: [&str; 2] = [
            "crates/rules/src/artifact_rules.rs",
            "crates/rules/src/benchmark_rules.rs",
        ];
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("the proof test has a workspace root")?;
        let source_hash = platform::build_inputs::content_hash(root)?;
        ensure!(
            source_hash == platform::BUILD_HASH,
            "material and profile proof producer is stale for the current source"
        );
        let mut manifest = obligations::manifest(root)?;
        let closure = obligations::closure(root, &manifest)?;
        ensure!(
            closure.refusals.is_empty(),
            "material and profile proof inventory is not closed:\n{}",
            closure.refusals.join("\n")
        );
        manifest.obligations.retain(|obligation| {
            matches!(obligation, Obligation::Kani { source, .. } if SOURCES.contains(&source.as_str()))
        });
        ensure!(
            manifest.obligations.len() == SOURCES.len(),
            "every selected material and profile source needs one proof obligation"
        );
        verify_kani(root, &manifest)?;
        ensure!(
            platform::build_inputs::content_hash(root)? == source_hash,
            "material or profile proof source changed during verification"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn tool_locks_and_their_counterexamples_verify_against_production() -> Result<()> {
        const SOURCE: &str = "crates/rules/src/tool_lock_rules.rs";
        const EXPECTED: (usize, usize) = (2, 10);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("the proof test has a workspace root")?;
        let source_hash = platform::build_inputs::content_hash(root)?;
        ensure!(
            source_hash == platform::BUILD_HASH,
            "tool lock proof producer is stale"
        );
        let manifest = obligations::manifest(root)?;
        let probes = manifest
            .obligations
            .iter()
            .find_map(|obligation| match obligation {
                Obligation::Kani { source, probes, .. } if source == SOURCE => Some(probes),
                _ => None,
            })
            .context("tool locks have a registered proof obligation")?;
        kani_environment_version(root)?;
        let verifier_hash = crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
        let engine_hash = crate::phase_evidence::engine_digest(root)?;
        let result = verify_rules(
            root,
            SOURCE,
            probes,
            &flags(&manifest, SOURCE),
            &verifier_hash,
            &engine_hash,
        )?;
        ensure!(
            result == EXPECTED,
            "tool lock proof inventory differs: {result:?}"
        );
        ensure!(
            platform::build_inputs::content_hash(root)? == source_hash,
            "tool lock source changed during verification"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn source_sync_and_its_counterexamples_verify_against_production() -> Result<()> {
        const SOURCE: &str = "crates/rules/src/source_sync_rules.rs";
        const EXPECTED: (usize, usize) = (1, 5);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("the proof test has a workspace root")?;
        let source_hash = platform::build_inputs::content_hash(root)?;
        ensure!(
            source_hash == platform::BUILD_HASH,
            "source synchronization proof producer is stale"
        );
        let manifest = obligations::manifest(root)?;
        let probes = manifest
            .obligations
            .iter()
            .find_map(|obligation| match obligation {
                Obligation::Kani { source, probes, .. } if source == SOURCE => Some(probes),
                _ => None,
            })
            .context("source synchronization has a registered proof obligation")?;
        kani_environment_version(root)?;
        verify_kani_driver(root)?;
        let verifier_hash = crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
        let engine_hash = crate::phase_evidence::engine_digest(root)?;
        let result = verify_rules(
            root,
            SOURCE,
            probes,
            &flags(&manifest, SOURCE),
            &verifier_hash,
            &engine_hash,
        )?;
        ensure!(
            result == EXPECTED,
            "source synchronization proof inventory differs: {result:?}"
        );
        ensure!(
            platform::build_inputs::content_hash(root)? == source_hash,
            "source synchronization changed during verification"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the installed pinned Kani"]
    #[cfg(not(target_os = "windows"))]
    fn assure_inputs_and_their_counterexamples_verify_against_production() -> Result<()> {
        const SOURCE: &str = "crates/rules/src/shape_rules.rs";
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("the proof test has a workspace root")?;
        let source_hash = platform::build_inputs::content_hash(root)?;
        ensure!(
            source_hash == platform::BUILD_HASH,
            "assure proof producer is stale"
        );
        let manifest = obligations::manifest(root)?;
        let probes = manifest
            .obligations
            .iter()
            .find_map(|obligation| match obligation {
                Obligation::Kani { source, probes, .. } if source == SOURCE => Some(probes),
                _ => None,
            })
            .context("assure decisions have a registered proof obligation")?;
        let expected = declared_inventory(root, SOURCE, probes)?;
        kani_environment_version(root)?;
        verify_kani_driver(root)?;
        let verifier_hash = crate::phase_evidence::verifier_digest(&installed_kani_directory()?)?;
        let engine_hash = crate::phase_evidence::engine_digest(root)?;
        let result = verify_rules(
            root,
            SOURCE,
            probes,
            &flags(&manifest, SOURCE),
            &verifier_hash,
            &engine_hash,
        )?;
        ensure!(
            result == expected,
            "assure proof inventory differs: {result:?}"
        );
        ensure!(
            platform::build_inputs::content_hash(root)? == source_hash,
            "assure proof source changed during verification"
        );
        Ok(())
    }

    const NATIVE_FIXTURE: &str = "DOTFILES_NATIVE_PROOF_FIXTURE";
    const NATIVE_TEST: &str =
        "skill_proofs::tests::missing_manifest_cannot_skip_independent_native_checks";
    const NIGHTLY_TEST: &str =
        "skill_proofs::tests::the_code_environment_dispatches_the_pinned_nightly_verifiers";
    const MUTATION_TEST: &str =
        "skill_proofs::tests::mutation_workers_do_not_multiply_the_test_pool";

    #[test]
    fn a_retained_miri_sysroot_requires_fresh_setup_and_execution() -> Result<()> {
        let cache = tempfile::tempdir()?;
        let setups = std::cell::Cell::new(0);
        let executions = std::cell::Cell::new(0);
        for _ in 0..2 {
            use_miri_sysroot(
                cache.path(),
                |sysroot| {
                    setups.set(setups.get() + 1);
                    fs::create_dir_all(sysroot)?;
                    fs::write(sysroot.join("compiled-stdlib"), b"compiler artifacts")?;
                    Ok(())
                },
                |sysroot| {
                    let contender = fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(cache.path().join(MIRI_SYSROOT_LOCK))?;
                    assert!(
                        contender.try_lock().is_err(),
                        "setup lease ended before interpretation"
                    );
                    assert_eq!(
                        fs::read(sysroot.join("compiled-stdlib"))?,
                        b"compiler artifacts"
                    );
                    executions.set(executions.get() + 1);
                    Ok(())
                },
            )?;
        }
        assert_eq!(setups.get(), 2);
        assert_eq!(executions.get(), 2);
        let failure = use_miri_sysroot(
            cache.path(),
            |_| anyhow::bail!("current setup failed"),
            |_| {
                executions.set(executions.get() + 1);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(failure.to_string().contains("current setup failed"));
        assert_eq!(
            executions.get(),
            2,
            "old artifacts credited a failed preparation"
        );
        Ok(())
    }

    #[test]
    fn a_miri_test_uses_the_prepared_sysroot_without_retaining_launch_metadata() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let sysroot = scope.path().join(MIRI_SYSROOT_DIRECTORY);
        let command = miri(scope.path(), Some(&sysroot));
        let selected = command
            .get_envs()
            .find_map(|(name, value)| (name == "MIRI_SYSROOT").then_some(value).flatten());
        assert_eq!(selected, Some(sysroot.as_os_str()));
        let arguments = command.get_args().collect::<Vec<_>>();
        assert!(
            arguments
                .windows(3)
                .any(|args| args == ["miri", "test", "--locked"])
        );
        assert!(!arguments.contains(&sysroot.as_os_str()));
        Ok(())
    }

    fn mutation_report_fixture() -> (Value, Value) {
        let phase = |name, status| serde_json::json!({"phase":name,"process_status":status});
        let baseline = serde_json::json!({"scenario":"Baseline","summary":"Success","phase_results":[phase("Build",serde_json::json!("Success")),phase("Test",serde_json::json!("Success"))]});
        let cases = [
            (
                "caught",
                "CaughtMutant",
                vec![
                    phase("Build", serde_json::json!("Success")),
                    phase("Test", serde_json::json!({"Failure":101})),
                ],
            ),
            (
                "missed",
                "MissedMutant",
                vec![
                    phase("Build", serde_json::json!("Success")),
                    phase("Test", serde_json::json!("Success")),
                ],
            ),
            (
                "timeout",
                "Timeout",
                vec![
                    phase("Build", serde_json::json!("Success")),
                    phase("Test", serde_json::json!("Timeout")),
                ],
            ),
            (
                "unviable",
                "Unviable",
                vec![phase("Build", serde_json::json!({"Failure":101}))],
            ),
        ];
        let inventory: Vec<_> = cases
            .iter()
            .map(|(name, _, _)| serde_json::json!({"name":name,"package":"rules"}))
            .collect();
        let mut outcomes = vec![baseline];
        outcomes.extend(cases.into_iter().map(|(name, summary, phase_results)| serde_json::json!({"scenario":{"Mutant":{"name":name,"package":"rules"}},"summary":summary,"phase_results":phase_results})));
        (
            serde_json::json!(inventory),
            serde_json::json!({"outcomes":outcomes,"total_mutants":4,"missed":1,"caught":1,"timeout":1,"unviable":1,"success":0,"end_time":"2026-10-12T10:00:00Z","cargo_mutants_version":MUTANTS_VERSION}),
        )
    }

    fn validate_mutation_fixture(
        inventory: Value,
        report: Value,
        exit: Option<i32>,
    ) -> Result<MutationReport> {
        let inventory: Vec<MutationIdentity> = serde_json::from_value(inventory)?;
        validate_mutation_report(
            &inventory,
            serde_json::from_value(report)?,
            exit,
            MUTATED_PACKAGE,
        )
    }

    #[test]
    fn mutation_reports_bind_terminal_phases_and_counters() -> Result<()> {
        let (inventory, report) = mutation_report_fixture();
        let validated = validate_mutation_fixture(
            inventory.clone(),
            report.clone(),
            Some(proof_budget_rules::MUTATION_TIMEOUT_EXIT),
        )?;
        assert_eq!(
            mutation_names(&validated, MutationSummary::CaughtMutant),
            ["caught"]
        );
        assert_eq!(
            mutation_names(&validated, MutationSummary::Timeout),
            ["timeout"]
        );
        for (pointer, value) in [
            ("/end_time", serde_json::json!(null)),
            ("/end_time", serde_json::json!("")),
            ("/cargo_mutants_version", serde_json::json!("wrong")),
            ("/total_mutants", serde_json::json!(3)),
            ("/caught", serde_json::json!(u64::MAX)),
            ("/success", serde_json::json!(1)),
            ("/outcomes/0/summary", serde_json::json!("Failure")),
            (
                "/outcomes/0/phase_results/1/process_status",
                serde_json::json!({"Failure":101}),
            ),
            (
                "/outcomes/1/scenario/Mutant/name",
                serde_json::json!("unexpected"),
            ),
            (
                "/outcomes/1/scenario/Mutant/package",
                serde_json::json!("other"),
            ),
            ("/outcomes/1/summary", serde_json::json!("MissedMutant")),
            (
                "/outcomes/1/phase_results/1/process_status",
                serde_json::json!({"Failure":0}),
            ),
            (
                "/outcomes/1/phase_results/1/process_status",
                serde_json::json!("Other"),
            ),
            (
                "/outcomes/1/phase_results/0/phase",
                serde_json::json!("Test"),
            ),
            ("/outcomes/1/phase_results", serde_json::json!([])),
            (
                "/outcomes/3/phase_results",
                serde_json::json!([{"phase":"Build","process_status":"Timeout"}]),
            ),
        ] {
            let mut broken = report.clone();
            *broken.pointer_mut(pointer).unwrap() = value;
            assert!(
                validate_mutation_fixture(
                    inventory.clone(),
                    broken,
                    Some(proof_budget_rules::MUTATION_TIMEOUT_EXIT)
                )
                .is_err(),
                "{pointer}"
            );
        }
        Ok(())
    }

    #[test]
    fn mutation_reports_require_every_unique_inventory_identity_and_baseline() {
        let (inventory, report) = mutation_report_fixture();
        for index in [0, 1] {
            let mut omitted = report.clone();
            omitted["outcomes"].as_array_mut().unwrap().remove(index);
            if index != 0 {
                omitted["caught"] = serde_json::json!(0);
                omitted["total_mutants"] = serde_json::json!(3);
            }
            assert!(
                validate_mutation_fixture(
                    inventory.clone(),
                    omitted,
                    Some(proof_budget_rules::MUTATION_TIMEOUT_EXIT)
                )
                .is_err()
            );
            let mut duplicate = report.clone();
            let copied = duplicate["outcomes"][index].clone();
            duplicate["outcomes"].as_array_mut().unwrap().push(copied);
            if index != 0 {
                duplicate["caught"] = serde_json::json!(2);
                duplicate["total_mutants"] = serde_json::json!(5);
            }
            assert!(
                validate_mutation_fixture(
                    inventory.clone(),
                    duplicate,
                    Some(proof_budget_rules::MUTATION_TIMEOUT_EXIT)
                )
                .is_err()
            );
        }
        let mut duplicate = inventory.clone();
        duplicate.as_array_mut().unwrap().push(inventory[0].clone());
        let mut other = inventory.clone();
        other[0]["package"] = serde_json::json!("other");
        for broken in [serde_json::json!([]), duplicate, other] {
            assert!(
                validate_mutation_fixture(
                    broken,
                    report.clone(),
                    Some(proof_budget_rules::MUTATION_TIMEOUT_EXIT)
                )
                .is_err()
            );
        }
        for exit in [
            None,
            Some(proof_budget_rules::MUTATION_CLEAN_EXIT),
            Some(proof_budget_rules::MUTATION_MISSED_EXIT),
        ] {
            assert!(validate_mutation_fixture(inventory.clone(), report.clone(), exit).is_err());
        }
    }

    #[test]
    fn ci_campaign_runs_all_common_checks_and_excludes_only_both_mutation_checks() {
        use std::cell::RefCell;
        for (campaign, expected) in [
            (
                ProofCampaign::Full,
                vec![
                    "Kani",
                    "Miri",
                    "fuzz",
                    "production mutants",
                    "mutation fixture",
                ],
            ),
            (ProofCampaign::Ci, vec!["Kani", "Miri", "fuzz"]),
        ] {
            for fail in [false, true] {
                let calls = RefCell::new(Vec::new());
                let check = |name| {
                    calls.borrow_mut().push(name);
                    ensure!(!fail, "{name} fixture failed");
                    Ok(())
                };
                let result = run_native_checks(
                    campaign,
                    || check("Kani"),
                    || check("Miri"),
                    || check("fuzz"),
                    || {
                        let mut checks = Checks::default();
                        checks.run("production mutants", || check("production mutants"));
                        checks.run("mutation fixture", || check("mutation fixture"));
                        checks.finish()
                    },
                );
                assert_eq!(*calls.borrow(), expected, "{campaign:?}, fail={fail}");
                assert_eq!(result.is_err(), fail);
                if fail {
                    let failure = format!("{:#}", result.unwrap_err());
                    for check in &expected {
                        assert!(failure.contains(check), "{failure}");
                    }
                    if campaign == ProofCampaign::Ci {
                        assert!(!failure.contains("mutation"), "{failure}");
                    }
                }
            }
        }
    }

    #[test]
    fn containers_preserve_the_selected_campaign_and_all_common_boundaries() {
        for (campaign, entry) in [
            (ProofCampaign::Full, "proofs"),
            (ProofCampaign::Ci, "proofs-ci"),
        ] {
            let full = container_run_arguments(Path::new("source"), "image", "target");
            let arguments =
                container_campaign_arguments(Path::new("source"), "image", "target", campaign, &[]);
            assert_eq!(arguments[..arguments.len() - 1], full[..full.len() - 1]);
            assert_eq!(
                arguments.last().unwrap(),
                &OsString::from(format!(
                    "{CONTAINER_SOURCE} exec cargo run --locked --manifest-path xtask/Cargo.toml -- {entry}"
                ))
            );
        }
    }

    #[test]
    fn mutation_source_keeps_readonly_input_bytes_and_inventory_paths() -> Result<()> {
        let original = tempfile::tempdir()?;
        let inputs = [
            ("Cargo.toml", b"[workspace]\n".as_slice()),
            (
                "crates/rules/src/lib.rs",
                b"pub fn rule() -> bool { true }\n",
            ),
            (
                "xtask/proofs/mutants-fixture/src/lib.rs",
                b"pub fn fixture() {}\n",
            ),
            ("scripts/helper", b"#!/bin/sh\nexit 0\n"),
        ];
        let mut permissions = Vec::new();
        for (relative, bytes) in inputs {
            let path = original.path().join(relative);
            fs::create_dir_all(path.parent().context("fixture parent")?)?;
            fs::write(&path, bytes)?;
            #[cfg(unix)]
            if relative == "scripts/helper" {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(
                    &path,
                    fs::Permissions::from_mode(
                        MUTANT_SOURCE_FILE_MODE | MUTANT_SOURCE_EXECUTE_BITS,
                    ),
                )?;
            }
            let writable = fs::metadata(&path)?.permissions();
            let mut sealed = writable.clone();
            sealed.set_readonly(true);
            fs::set_permissions(&path, sealed)?;
            permissions.push((path, writable));
        }
        fs::create_dir(original.path().join("target"))?;
        fs::write(original.path().join("target/ignored"), b"build output")?;
        let mut staged = None;
        let result = with_mutant_source(original.path(), |workspace| {
            let source = &workspace.directory;
            staged = Some(source.clone());
            let mut files = Vec::new();
            crate::profiles::source_files(source, &mut files)?;
            assert_eq!(files.len(), inputs.len());
            for (relative, bytes) in inputs {
                assert_eq!(fs::read(source.join(relative))?, bytes);
                assert!(
                    fs::metadata(original.path().join(relative))?
                        .permissions()
                        .readonly()
                );
                fs::write(source.join(relative), b"a mutant changed its owned input")?;
                assert_eq!(fs::read(original.path().join(relative))?, bytes);
            }
            assert!(!source.join("target").exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                assert_eq!(
                    fs::metadata(source.join("scripts/helper"))?
                        .permissions()
                        .mode()
                        & MUTANT_SOURCE_EXECUTE_BITS,
                    MUTANT_SOURCE_EXECUTE_BITS
                );
            }
            for directory in [source.clone(), source.join(MUTANTS_FIXTURE)] {
                let command =
                    mutants_with_cores(&directory, Path::new("output"), &workspace.temporary, 1);
                assert!(
                    command
                        .get_args()
                        .collect::<Vec<_>>()
                        .windows(2)
                        .any(|pair| { pair[0] == "--dir" && pair[1] == directory.as_os_str() })
                );
            }
            Ok(())
        });
        for (path, writable) in permissions {
            fs::set_permissions(path, writable)?;
        }
        result?;
        assert!(!staged.context("mutation source was staged")?.exists());
        Ok(())
    }

    #[test]
    fn mutation_source_removes_its_copy_when_the_consumer_fails() -> Result<()> {
        let original = tempfile::tempdir()?;
        fs::write(original.path().join("Cargo.toml"), b"[workspace]\n")?;
        let mut staged = None;
        let result: Result<()> = with_mutant_source(original.path(), |source| {
            staged = Some(source.directory.clone());
            anyhow::bail!("original mutation failure")
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("original mutation failure")
        );
        assert!(!staged.context("mutation source was staged")?.exists());
        assert_eq!(
            fs::read(original.path().join("Cargo.toml"))?,
            b"[workspace]\n"
        );
        Ok(())
    }

    #[test]
    fn mutation_source_holds_its_ancestor_owner_lock_until_the_consumer_finishes() -> Result<()> {
        let original = tempfile::tempdir()?;
        fs::write(original.path().join("Cargo.toml"), b"[workspace]\n")?;
        let mut campaign = None;
        with_mutant_source(original.path(), |source| {
            let source = &source.directory;
            let temporary = std::env::temp_dir();
            let owner = source
                .ancestors()
                .take_while(|directory| *directory != temporary)
                .find(|directory| directory.join("owner.json").is_file())
                .context("mutation source has no campaign owner")?;
            assert_ne!(source, owner);
            let marker: Value = serde_json::from_slice(&fs::read(owner.join("owner.json"))?)?;
            assert!(
                marker["schema"]
                    .as_str()
                    .unwrap()
                    .ends_with("-temp-owner-v1")
            );
            assert_eq!(marker["role"], "scratch");
            assert_eq!(marker["kept"], false);
            let contender = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(owner.join("owner.lock"))?;
            assert!(matches!(
                contender.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            fs::write(
                source.join("consumer-output"),
                b"consumer still owns the campaign",
            )?;
            assert!(matches!(
                contender.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            campaign = Some(owner.to_path_buf());
            Ok(())
        })?;
        assert!(!campaign.context("campaign was observed")?.exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn mutation_source_refuses_links_before_the_consumer_runs() -> Result<()> {
        let original = tempfile::tempdir()?;
        let external = tempfile::tempdir()?;
        fs::write(external.path().join("input"), b"external input")?;
        std::os::unix::fs::symlink(external.path().join("input"), original.path().join("link"))?;
        let mut ran = false;
        let result = with_mutant_source(original.path(), |_| {
            ran = true;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!ran);
        assert_eq!(fs::read(external.path().join("input"))?, b"external input");
        Ok(())
    }

    #[test]
    fn mutation_commands_bind_worker_build_and_test_pools() {
        const OBSERVED_MAC_CORES: usize = 18;
        for (cores, workers) in [
            (0, 1),
            (
                proof_budget_rules::MUTANT_JOB_LIMIT,
                proof_budget_rules::MUTANT_JOB_LIMIT,
            ),
            (
                proof_budget_rules::MUTANT_JOB_LIMIT + 1,
                proof_budget_rules::MUTANT_JOB_LIMIT,
            ),
            (OBSERVED_MAC_CORES, proof_budget_rules::MUTANT_JOB_LIMIT),
            (usize::MAX, proof_budget_rules::MUTANT_JOB_LIMIT),
        ] {
            let command = mutants_with_cores(
                Path::new("source"),
                Path::new("output"),
                Path::new("temporary root with ' space"),
                cores,
            );
            for variable in MUTANT_TEMP_VARIABLES {
                assert!(command.get_envs().any(|(name, value)| {
                    name == variable
                        && value == Some(std::ffi::OsStr::new("temporary root with ' space"))
                }));
            }
            let arguments: Vec<_> = command
                .get_args()
                .map(|argument| argument.to_string_lossy())
                .collect();
            let expected = workers.to_string();
            for option in ["--jobs", "--jobserver-tasks"] {
                assert!(
                    arguments
                        .windows(2)
                        .any(|pair| pair[0] == option && pair[1] == expected),
                    "{cores}: {arguments:?}"
                );
            }
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "--jobserver=true")
            );
            assert!(
                command
                    .get_envs()
                    .any(|(name, value)| name == "RUST_TEST_THREADS"
                        && value == Some(std::ffi::OsStr::new("1")))
            );
            assert!(
                command
                    .get_envs()
                    .any(|(name, value)| name == "CARGO_TARGET_DIR" && value.is_none())
            );
        }
    }

    #[test]
    fn mutation_failures_retain_exact_diagnostics_outside_the_scratch_scope() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let scratch = tempfile::tempdir_in(scope.path())?;
        let cache = scope.path().join("cache");
        let parent = mutant_output_root(scratch.path(), Some(&cache));
        let mut retained = None;
        let result = with_mutant_output(&parent, |output| {
            retained = Some(output.to_path_buf());
            fs::create_dir_all(output.join("mutants.out/log"))?;
            fs::write(output.join("mutants.out/outcomes.json"), b"exact outcomes")?;
            fs::write(
                output.join("mutants.out/log/finite.log"),
                b"exact test failure",
            )?;
            anyhow::bail!("unexplained mutation timeout")
        });
        let path = retained.context("the campaign started")?;
        drop(scratch);
        assert!(path.is_dir(), "the failed campaign deleted its diagnostics");
        assert!(path.starts_with(&cache));
        assert_eq!(
            fs::read(path.join("mutants.out/outcomes.json"))?,
            b"exact outcomes"
        );
        assert_eq!(
            fs::read(path.join("mutants.out/log/finite.log"))?,
            b"exact test failure"
        );
        let error = result.unwrap_err();
        let report = format!("{error:#}");
        assert!(report.contains("unexplained mutation timeout"));
        assert!(report.contains(&path.display().to_string()));
        Ok(())
    }

    #[test]
    fn mutation_success_removes_its_owned_output() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let parent = mutant_output_root(scope.path(), None);
        assert_eq!(
            parent,
            scope.path().join("target").join(MUTANT_OUTPUT_DIRECTORY)
        );
        let mut completed = None;
        with_mutant_output(&parent, |output| {
            completed = Some(output.to_path_buf());
            fs::write(output.join("outcomes.json"), b"all caught")?;
            Ok(())
        })?;
        assert!(!completed.context("the campaign started")?.exists());
        Ok(())
    }

    #[test]
    #[ignore = "requires the bounded container route with the pinned cargo-mutants"]
    fn bounded_mutation_workers_stay_with_the_campaign_owner() -> Result<()> {
        ensure!(
            !dotfiles::bounded::requires_container(),
            "use `dotfiles bounded run` for the mutation worker fixture"
        );
        let root = crate::canonical(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").as_path())?;
        mutation_worker_fixture(&root)
    }

    fn mutation_worker_fixture(root: &Path) -> Result<()> {
        let version = Tool::Cargo
            .command()
            .args(["mutants", "--version"])
            .output()?;
        ensure!(
            version.status.success()
                && String::from_utf8(version.stdout)?.trim()
                    == format!("cargo-mutants {MUTANTS_VERSION}"),
            "mutation worker fixture requires its pinned cargo-mutants"
        );
        with_mutant_source(root, |source| {
            mutation_campaign(root, |output| {
                let status = mutants(
                    &source.directory.join(MUTANTS_FIXTURE),
                    output,
                    &source.temporary,
                )
                .arg("--leak-dirs")
                .status()
                .context("start the mutation worker fixture")?;
                let report = completed_mutation_report(output, status, MUTANTS_FIXTURE_PACKAGE)?;
                ensure!(
                    report.missed != 0,
                    "the mutation worker fixture failed its baseline or caught its assertionless rule"
                );
                let workers = fs::read_dir(&source.temporary)?
                    .filter_map(|entry| match entry {
                        Ok(entry)
                            if entry
                                .file_name()
                                .to_string_lossy()
                                .starts_with(MUTANT_WORKER_PREFIX) =>
                        {
                            Some(Ok(entry))
                        }
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    })
                    .collect::<std::io::Result<Vec<_>>>()?;
                ensure!(
                    !workers.is_empty(),
                    "mutation fixture created no owned worker copies"
                );
                for worker in workers {
                    ensure!(
                        worker.file_type()?.is_dir(),
                        "mutation worker root contains a non-directory"
                    );
                    ensure!(
                        worker.path().join("Cargo.toml").is_file(),
                        "mutation worker has no source manifest"
                    );
                    ensure!(
                        !worker.path().starts_with(&source.directory),
                        "mutation worker lies inside copied source"
                    );
                    let contender = fs::OpenOptions::new().read(true).write(true).open(
                        worker
                            .path()
                            .parent()
                            .context("worker parent")?
                            .parent()
                            .context("campaign parent")?
                            .join(MUTANT_OWNER_LOCK),
                    )?;
                    ensure!(
                        matches!(contender.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
                        "mutation worker lost its ancestor owner lease"
                    );
                }
                Ok(())
            })
        })
    }

    #[test]
    #[ignore = "requires the installed pinned cargo-mutants"]
    fn mutation_workers_do_not_multiply_the_test_pool() -> Result<()> {
        let root = crate::canonical(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").as_path())?;
        if std::env::var(NATIVE_FIXTURE).is_ok_and(|value| value == MUTATION_TEST) {
            return mutation_worker_fixture(&root);
        }
        let home = platform::home::home().context("native tool home")?;
        let environment = dotfiles::hosts::code_environment(&root, &home)?;
        let output = environment
            .command(std::env::current_exe()?)?
            .args(["--ignored", "--exact", MUTATION_TEST, "--nocapture"])
            .env(NATIVE_FIXTURE, MUTATION_TEST)
            .env("RUST_TEST_THREADS", "2")
            .output()?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        ensure!(
            output.status.success(),
            "owned mutation worker fixture failed"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the installed pinned nightly, Miri, and cargo-fuzz"]
    fn the_code_environment_dispatches_the_pinned_nightly_verifiers() -> Result<()> {
        let root = crate::canonical(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").as_path())?;
        if std::env::var(NATIVE_FIXTURE).is_ok_and(|value| value == NIGHTLY_TEST) {
            let mut checks = Checks::default();
            checks.run("Miri nightly fixture", || {
                let mut targets = Checks::default();
                for &target in MIRI_TARGETS {
                    targets.run(target.unwrap_or("the host"), || {
                        let mut command = miri(&root, None);
                        command.arg("--manifest-path").arg(root.join(MIRI_FIXTURE));
                        if let Some(target) = target {
                            command.args(["--target", target]);
                        }
                        let output = command.output()?;
                        ensure!(
                            !output.status.success()
                                && String::from_utf8_lossy(&output.stderr).contains("Undefined Behavior"),
                            "Miri did not interpret its undefined-behavior fixture for {target:?}:\n{}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        Ok(())
                    });
                }
                targets.finish()
            });
            checks.run("fuzz nightly fixture", || {
                let host = nightly_host(&root)?;
                let output = replay(&root, FUZZ_FIXTURE, "crash", &host)?;
                ensure!(
                    !output.status.success()
                        && String::from_utf8_lossy(&output.stderr).contains("panicked"),
                    "fuzz did not replay its crashing corpus:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            });
            return checks.finish();
        }
        let home = platform::home::home().context("native tool home")?;
        let environment = dotfiles::hosts::code_environment(&root, &home)?;
        let output = environment
            .command(std::env::current_exe()?)?
            .args(["--ignored", "--exact", NIGHTLY_TEST, "--nocapture"])
            .env(NATIVE_FIXTURE, NIGHTLY_TEST)
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        ensure!(
            output.status.success(),
            "owned nightly verifier fixture failed"
        );
        Ok(())
    }

    #[test]
    fn missing_manifest_cannot_skip_independent_native_checks() -> Result<()> {
        if std::env::var(NATIVE_FIXTURE).is_ok_and(|value| value == NATIVE_TEST) {
            let root = tempfile::tempdir()?;
            let failure = verify_native_prepared(root.path(), ProofCampaign::Full).unwrap_err();
            let diagnostic = format!("{failure:#}");
            for stage in ["Kani:", "Miri:", "fuzz:", "mutation:"] {
                assert!(diagnostic.contains(stage), "{diagnostic}");
            }
            assert!(diagnostic.contains(obligations::MANIFEST), "{diagnostic}");
            return Ok(());
        }
        let sysroot = tempfile::tempdir()?;
        let output = platform::process::command(std::env::current_exe()?)
            .args(["--exact", NATIVE_TEST, "--nocapture"])
            .env(NATIVE_FIXTURE, NATIVE_TEST)
            .env("RUSTUP_AUTO_INSTALL", "0")
            .env("MIRI_SYSROOT", sysroot.path())
            .env("CARGO_NET_OFFLINE", "true")
            .env("MISE_AUTO_INSTALL", "false")
            .output()?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        ensure!(output.status.success(), "owned native proof fixture failed");
        Ok(())
    }
}
