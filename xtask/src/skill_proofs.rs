use crate::obligations::{self, KaniFlag, Obligation};
use crate::proof_budget_rules;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Stdio;

pub const KANI_VERSION: &str = "0.68.0";

/// Every proof covers a reachable acceptance and a reachable refusal.
const WITNESSES: usize = 2;
const OVERFLOW_PROBE: &str = "reject_an_overflow_under_checked_mir";
const CHECKED_ADD_OVERFLOW: &str = "attempt to add with overflow";
/// The most harnesses sharing one bounded report.
pub const KANI_BATCH_SIZE: usize = 16;
const KANI_BATCH_TEST: &str =
    "a_kani_batch_verifies_every_harness_and_rejects_every_counterexample";

/// Compares a Kani results document with the expected harnesses and the proof budget.
///
/// # Errors
///
/// Returns an error when the run stops early or holds no harness, the harness set differs from `expected`, a harness lacks a measurement or exceeds the budget, or a result contradicts `counterexample`.
/// A normal run must pass every proof with an assertion, two satisfied cover witnesses, and every property verified, and a counterexample run must fail every harness on an assertion.
pub fn validate_results(value: &Value, expected: &[&str], counterexample: bool) -> Result<()> {
    let summary = &value["verification_results"]["summary"];
    ensure!(
        summary["status"] == "completed"
            && summary["executed"].as_u64() == Some(expected.len() as u64)
            && !expected.is_empty(),
        "proof execution is incomplete or empty"
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
    let slow: Vec<String> = results
        .iter()
        .filter_map(|result| {
            let duration = result["duration_ms"].as_u64();
            (proof_budget_rules::verdict(duration) != proof_budget_rules::Verdict::Within).then(
                || {
                    format!(
                        "{} took {} ms",
                        result["harness_id"].as_str().unwrap_or_default(),
                        duration
                            .map_or_else(|| "unmeasured".to_owned(), |millis| millis.to_string())
                    )
                },
            )
        })
        .collect();
    ensure!(
        slow.is_empty(),
        "each proof harness must finish within {} ms; shrink its bounds or split it without weakening its assertions:\n{}",
        proof_budget_rules::BUDGET_MILLIS,
        slow.join("\n")
    );
    if counterexample {
        ensure!(
            summary["failed"] == expected.len(),
            "counterexample was not rejected"
        );
        ensure!(
            results.iter().all(
                |result| result["checks"].as_array().is_some_and(|checks| checks
                    .iter()
                    .any(|check| check["category"] == "assertion" && check["status"] == "Failure"))
            ),
            "expected assertion counterexample is missing"
        );
        for result in results.iter().filter(|result| {
            result["harness_id"]
                .as_str()
                .is_some_and(|name| name.rsplit("::").next() == Some(OVERFLOW_PROBE))
        }) {
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
    } else {
        ensure!(
            summary["failed"] == 0 && summary["successful"] == expected.len(),
            "required proof failed"
        );
        for result in results {
            validate_proof(result).with_context(|| {
                format!(
                    "unverified harness {}",
                    result["harness_id"].as_str().unwrap_or_default()
                )
            })?;
        }
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

/// The unstable features a Kani run enables: stubbing always, and the ones a contract obligation adds.
fn features(flags: &BTreeSet<&'static str>) -> Vec<&'static str> {
    flags.iter().flat_map(|flag| ["-Z", flag]).collect()
}

fn standalone_kani() -> std::process::Command {
    // Inlining and jump threading duplicate cover intrinsics along unsatisfiable paths.
    const CHECKED_MIR: &str = "--edition=2024 -C overflow-checks=yes -C debug-assertions=yes -Z mir-opt-level=3 -Z mir-enable-passes=-Inline,-JumpThreading";
    let mut flags = std::env::var_os("RUSTFLAGS").unwrap_or_default();
    if !flags.is_empty() {
        flags.push(" ");
    }
    flags.push(CHECKED_MIR);
    let mut command = Tool::Kani.command();
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
    ensure!(
        !expected.is_empty(),
        "proof execution is incomplete or empty"
    );
    ensure!(
        expected.iter().copied().collect::<BTreeSet<_>>().len() == expected.len(),
        "duplicate proof harness"
    );
    for batch in expected.chunks(KANI_BATCH_SIZE) {
        verify_batch(source, directory, batch, counterexample, flags)?;
    }
    Ok(())
}

fn verify_batch(
    source: &Path,
    directory: &Path,
    expected: &[&str],
    counterexample: bool,
    flags: &BTreeSet<&'static str>,
) -> Result<()> {
    let output = directory.join("results.json");
    fs::write(&output, "")?;
    let mut command = standalone_kani();
    command
        .current_dir(directory)
        .arg(source)
        .args(["--output-format", "terse", "-Z", "unstable-options"])
        .args(features(flags))
        .arg("--exact")
        .arg("--harness-timeout")
        .arg(format!("{}s", proof_budget_rules::TIMEOUT_SECONDS))
        .arg("--export-json")
        .arg(&output);
    for name in expected {
        command.args(["--harness", name]);
    }
    let status = command.status().context("start pinned Kani verifier")?;
    ensure!(
        status.success() != counterexample,
        "Kani returned an unexpected outcome: {status}"
    );
    let result: Value = skill_ops::read_json(&output)?;
    validate_results(&result, expected, counterexample)
}

/// The harnesses that `kani list` finds in `source`, which every run must then pass.
fn verify_inventory(
    source: &Path,
    directory: &Path,
    flags: &BTreeSet<&'static str>,
) -> Result<BTreeSet<String>> {
    let status = standalone_kani()
        .current_dir(directory)
        .args(["list", "--format", "json"])
        .args(features(flags))
        .arg(source)
        .status()?;
    ensure!(status.success(), "cannot list required proof harnesses");
    let listed: Value = skill_ops::read_json(&directory.join("kani-list.json"))?;
    ensure!(
        listed["kani-version"] == KANI_VERSION,
        "unexpected proof inventory version"
    );
    let declarations = listed["standard-harnesses"]
        .as_object()
        .context("missing standard harness inventory")?;
    let contracts = listed["contract-harnesses"].as_object();
    let names: BTreeSet<String> = declarations
        .values()
        .chain(contracts.into_iter().flat_map(serde_json::Map::values))
        .flat_map(|value| value.as_array().into_iter().flatten())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("invalid proof inventory")
        })
        .collect::<Result<_>>()?;
    ensure!(
        !names.is_empty(),
        "{} declares no proof harness",
        source.display()
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

/// Builds an absent image with the native gate's release pins.
///
/// # Errors
///
/// Returns an error when a pin or Docker command fails.
pub fn ensure_image(root: &Path, definition: &Path, image: &str) -> Result<()> {
    let present = Tool::Docker
        .command()
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if present.success() {
        return Ok(());
    }
    let mut command = Tool::Docker.command();
    command
        .args(["build", "--tag", image, "--file"])
        .arg(definition);
    for pin in ImagePins::load(root)?.arguments() {
        command.arg("--build-arg").arg(pin);
    }
    let status = command.arg(root.join("xtask/proofs")).status()?;
    ensure!(
        status.success(),
        "building the verification image failed: {status}"
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
const CONTAINER_GATE: &str = "set -o pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; exec cargo run --locked --manifest-path xtask/Cargo.toml -- proofs";

pub fn container_run_arguments(root: &Path, image: &str, target_volume: &str) -> Vec<OsString> {
    // Docker reads `--mount` as one CSV record, so quoting keeps a comma in the path inside the field, and Windows forbids the quote character in paths.
    let mut mount = OsString::from("type=bind,\"source=");
    mount.push(root);
    mount.push("\",target=/source,readonly");
    // The same image also runs the gungraun benchmarks, whose setarch disables address-space randomization through a personality change the default seccomp profile refuses.
    let mut arguments: Vec<OsString> = [
        "run",
        "--rm",
        "--security-opt=seccomp=unconfined",
        "--mount",
    ]
    .map(OsString::from)
    .into();
    arguments.push(mount);
    arguments.extend(
        [
            "--volume",
            &format!("{target_volume}:/cache"),
            "--volume",
            "dotfiles-proofs-registry:/usr/local/cargo/registry",
            "--env",
            "CARGO_TARGET_DIR=/cache/target",
            "--env",
            &format!("{INSIDE_CONTAINER}=1"),
            "--workdir",
            "/work",
            image,
            "bash",
            "-ec",
            CONTAINER_GATE,
        ]
        .map(OsString::from),
    );
    arguments
}

/// Kani has no native build here, so the unchanged proof gate runs on a pinned Linux verification host that mounts this checkout read-only.
fn verify_in_container(root: &Path) -> Result<()> {
    let root = crate::canonical(root)?;
    ensure_linux_engine().context("Kani proofs on this host run in a Linux container")?;
    let definition = root.join(PROOF_IMAGE);
    let image = proof_image(&root)?;
    ensure_image(&root, &definition, &image)?;
    let status = Tool::Docker
        .command()
        .args(container_run_arguments(
            &root,
            &image,
            &container_target_volume(&root),
        ))
        .status()
        .context("start the Kani verification container")?;
    ensure!(
        status.success(),
        "the containerized proof gate failed: {status}"
    );
    Ok(())
}

/// Runs every Kani obligation of the manifest natively on macOS and Linux, and in a Linux container elsewhere.
///
/// # Errors
///
/// Returns an error when the pinned Kani version or Docker engine can't run, Docker can't build the verification image, or any proof or counterexample check fails.
pub fn verify(root: &Path) -> Result<()> {
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        verify_native(root)?;
    } else {
        verify_in_container(root)?;
    }
    if std::env::var_os(INSIDE_CONTAINER).is_some() {
        // The image pins Valgrind and the gungraun runner, so the budgets run here with them natively.
        crate::benchmarks::verify(root)?;
        return Ok(());
    }
    // The models and native-host benchmarks run in their pinned images.
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        crate::benchmarks::verify_in_container(root)?;
    }
    let manifest = obligations::manifest(root)?;
    crate::quint::verify(root, &manifest)?;
    crate::alloy::verify(root, &manifest)?;
    crate::lean::verify(root, &manifest)?;
    Ok(())
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

fn verify_native(root: &Path) -> Result<()> {
    let version = Tool::Kani.command().arg("--version").output()?;
    ensure!(
        version.status.success()
            && String::from_utf8(version.stdout)?
                .contains(&format!("Kani Rust Verifier {KANI_VERSION}")),
        "the pinned Kani version is unavailable"
    );
    verify_kani_driver(root)?;
    let manifest = obligations::manifest(root)?;
    let mut verified = 0;
    let mut rejected = 0;
    for obligation in &manifest.obligations {
        if let Obligation::Kani { source, probes } = obligation {
            let flags = flags(&manifest, source);
            let (proved, refused) = verify_rules(root, source, probes, &flags)?;
            verified += proved;
            rejected += refused;
        }
    }
    println!(
        "Verified all {verified} production policy harnesses with their reachable outcomes, and rejected all {rejected} counterexamples"
    );
    verify_miri(root)?;
    verify_fuzz(root)?;
    verify_mutants(root, &manifest)
}

fn verify_kani_driver(root: &Path) -> Result<()> {
    let output = Tool::Cargo
        .command()
        .current_dir(root)
        .args([
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
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    ensure!(
        output.status.success()
            && stdout
                .lines()
                .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;")),
        "the Kani driver regression did not pass"
    );
    Ok(())
}

/// The cargo-fuzz release that `mise.toml` and the proof image pin.
pub const FUZZ_VERSION: &str = "0.13.2";

/// The cargo-fuzz directory of this repository's parser targets, which `assure.toml` names as their owners.
pub const FUZZ_DIRECTORY: &str = "fuzz";

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

/// Replays the committed corpus of `target` once, with no new input, and returns the run's output.
fn replay(root: &Path, directory: &str, target: &str) -> Result<std::process::Output> {
    Tool::Cargo
        .command()
        .current_dir(root)
        .arg(format!("+{MIRI_TOOLCHAIN}"))
        .args(["fuzz", "run", "--fuzz-dir", directory, target])
        .arg(format!("{directory}/corpus/{target}"))
        .args(["--", "-runs=0"])
        .output()
        .context("start cargo-fuzz")
}

/// Replays every parser target over its committed corpus and refuses a fixture whose corpus makes its target panic.
fn verify_fuzz(root: &Path) -> Result<()> {
    let version = Tool::Cargo.command().args(["fuzz", "--version"]).output()?;
    ensure!(
        version.status.success()
            && String::from_utf8(version.stdout)?.contains(&format!("cargo-fuzz {FUZZ_VERSION}")),
        "the pinned cargo-fuzz {FUZZ_VERSION} is unavailable"
    );
    let targets = fuzz_targets(&root.join(FUZZ_DIRECTORY))?;
    ensure!(!targets.is_empty(), "{FUZZ_DIRECTORY} holds no fuzz target");
    for target in &targets {
        let corpus = root.join(FUZZ_DIRECTORY).join("corpus").join(target);
        ensure!(
            fs::read_dir(&corpus).is_ok_and(|mut entries| entries.next().is_some()),
            "the fuzz target {target} has no corpus under {}",
            corpus.display()
        );
        let output = replay(root, FUZZ_DIRECTORY, target)?;
        ensure!(
            output.status.success(),
            "replaying the corpus of {target} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = replay(root, FUZZ_FIXTURE, "crash")?;
    ensure!(
        !output.status.success() && String::from_utf8_lossy(&output.stderr).contains("panicked"),
        "the corpus replay admitted {FUZZ_FIXTURE}, whose corpus makes its target panic, so the gate proves nothing"
    );
    println!(
        "Replayed the corpus of every fuzz target, {}, and refused its fixture",
        targets.join(", ")
    );
    Ok(())
}

/// The cargo-mutants release that `mise.toml` and the proof image pin.
pub const MUTANTS_VERSION: &str = "27.1.0";

/// The mutants built and tested at once: one per core the host offers, since each one only builds and tests the small core.
fn mutant_jobs() -> String {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .to_string()
}

/// The crate whose specification lost its assertion, which the mutation gate must refuse.
const MUTANTS_FIXTURE: &str = "xtask/proofs/mutants-fixture";

/// The package whose tests must catch every mutant of its code.
const MUTATED_PACKAGE: &str = "rules";

fn mutants(directory: &Path, output: &Path) -> std::process::Command {
    let mut command = Tool::Cargo.command();
    // Each mutant builds in its own copy of the tree, which a shared target directory would serialize.
    command
        .env_remove("CARGO_TARGET_DIR")
        .args(["mutants", "--no-shuffle", "--jobs"])
        .arg(mutant_jobs())
        .arg("--dir")
        .arg(directory)
        .arg("--output")
        .arg(output);
    command
}

/// A campaign's list of mutants with one outcome, empty when the campaign found none.
fn outcome(output: &Path, list: &str) -> String {
    fs::read_to_string(output.join("mutants.out").join(list)).unwrap_or_default()
}

fn lines(text: &str) -> u64 {
    text.lines().filter(|line| !line.trim().is_empty()).count() as u64
}

/// Mutates the decision core, holds its survivors and unexplained timeouts to their baselines, and refuses a fixture whose specification lost its assertion.
/// Test times stay advisory: only the outcome of each mutant counts.
fn verify_mutants(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    let version = Tool::Cargo
        .command()
        .args(["mutants", "--version"])
        .output()?;
    ensure!(
        version.status.success()
            && String::from_utf8(version.stdout)?
                .contains(&format!("cargo-mutants {MUTANTS_VERSION}")),
        "the pinned cargo-mutants {MUTANTS_VERSION} is unavailable"
    );
    let output = tempfile::tempdir()?;
    let status = mutants(root, output.path())
        .args(["--package", MUTATED_PACKAGE])
        .status()
        .context("start cargo-mutants")?;
    ensure!(
        !outcome(output.path(), "caught.txt").is_empty(),
        "cargo-mutants caught no mutant of {MUTATED_PACKAGE}, so the campaign didn't run: {status}"
    );
    let missed = outcome(output.path(), "missed.txt");
    let unexplained =
        obligations::unexplained_timeouts(manifest, &outcome(output.path(), "timeout.txt"));
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
    let fixture = tempfile::tempdir()?;
    mutants(&root.join(MUTANTS_FIXTURE), fixture.path())
        .status()
        .context("start cargo-mutants on its fixture")?;
    ensure!(
        lines(&outcome(fixture.path(), "missed.txt")) > 0,
        "the mutation gate admitted {MUTANTS_FIXTURE}, whose specification asserts nothing, so the gate proves nothing"
    );
    println!(
        "Every mutant of {MUTATED_PACKAGE} died or holds its baseline, and the fixture without an assertion let its mutants survive"
    );
    Ok(())
}

/// The unstable options the obligations of `source` need: stubbing always, and what each contract obligation names.
fn flags(manifest: &obligations::Manifest, source: &str) -> BTreeSet<&'static str> {
    let mut flags = BTreeSet::from([KaniFlag::Stubbing.argument()]);
    for obligation in &manifest.obligations {
        if let Obligation::KaniContract {
            source: contract,
            flags: named,
            ..
        } = obligation
            && contract == source
        {
            flags.extend(named.iter().map(|flag| flag.argument()));
        }
    }
    flags
}

/// The nightly that Kani pins, which also carries Miri.
pub const MIRI_TOOLCHAIN: &str = "nightly-2026-08-21";

/// The crates whose tests need no foreign interface, process, or network, so Miri can interpret every one.
pub const MIRI_CRATES: [&str; 1] = ["rules"];

/// The targets Miri interprets the tests for: the host, and Windows, which the tools also ship to.
pub const MIRI_TARGETS: [Option<&str>; 2] = [None, Some("x86_64-pc-windows-msvc")];

/// The tests in one Miri invocation, whose whole binary must meet the proof budget.
const MIRI_BATCH_TESTS: usize = 32;

/// The crate with undefined behavior that Miri must refuse.
const MIRI_FIXTURE: &str = "xtask/proofs/miri-fixture/Cargo.toml";

/// A discovered test and its existing ignore state.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    let tests: Vec<_> = tests
        .into_iter()
        .map(|(name, ignored)| MiriTest { name, ignored })
        .collect();
    Ok(tests
        .chunks(MIRI_BATCH_TESTS)
        .map(<[MiriTest]>::to_vec)
        .collect())
}

/// Checks every selected test's completed outcome, ignore state, and binary duration.
///
/// # Errors
///
/// Returns an error for a missing, duplicate, changed, failed, or slow result.
pub fn validate_miri_batch(output: &str, expected: &[MiriTest]) -> Result<()> {
    let mut completed = BTreeMap::new();
    let mut suites = 0;
    for event in miri_events(output)? {
        match (event["type"].as_str(), event["event"].as_str()) {
            (Some("suite" | "test"), Some("started")) => {}
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
                    .context("missing Miri duration")?
                    .to_string();
                ensure!(
                    proof_budget_rules::verdict(millis_of(&seconds))
                        == proof_budget_rules::Verdict::Within,
                    "Miri test binary exceeds {} ms: {seconds}s",
                    proof_budget_rules::BUDGET_MILLIS
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

fn miri(root: &Path) -> std::process::Command {
    let mut command = Tool::Cargo.command();
    command
        .current_dir(root)
        .arg(format!("+{MIRI_TOOLCHAIN}"))
        .args(["miri", "test", "--locked"]);
    command
}

/// Interprets the pure crates' tests under Miri for every target, each test binary within the proof budget, and refuses a fixture with undefined behavior.
fn verify_miri(root: &Path) -> Result<()> {
    for target in MIRI_TARGETS {
        for krate in MIRI_CRATES {
            let mut command = miri(root);
            command.args(["--package", krate]);
            if let Some(target) = target {
                command.args(["--target", target]);
            }
            let output = command
                .args(["--", "--list", "--format", "json", "-Z", "unstable-options"])
                .output()
                .context("list Miri tests")?;
            ensure!(
                output.status.success(),
                "cannot list Miri tests of {krate} for {}:\n{}",
                target.unwrap_or("the host"),
                String::from_utf8_lossy(&output.stderr)
            );
            for batch in miri_batches(&String::from_utf8(output.stdout)?)? {
                let mut command = miri(root);
                command.args(["--package", krate]);
                if let Some(target) = target {
                    command.args(["--target", target]);
                }
                let output = command
                    .args([
                        "--",
                        "--format",
                        "json",
                        "-Z",
                        "unstable-options",
                        "--exact",
                    ])
                    .args(batch.iter().map(|test| &test.name))
                    .output()
                    .context("start Miri")?;
                let report = String::from_utf8_lossy(&output.stdout);
                ensure!(
                    output.status.success(),
                    "Miri refused the tests of {krate} for {}:\n{report}\n{}",
                    target.unwrap_or("the host"),
                    String::from_utf8_lossy(&output.stderr)
                );
                validate_miri_batch(&report, &batch).with_context(|| {
                    format!("Miri tests of {krate} for {}", target.unwrap_or("the host"))
                })?;
                let ignored = batch.iter().filter(|test| test.ignored).count();
                println!(
                    "Miri verified {} tests of {krate} for {} within the proof budget and retained {ignored} existing ignores",
                    batch.len() - ignored,
                    target.unwrap_or("the host")
                );
            }
        }
    }
    let output = miri(root)
        .arg("--manifest-path")
        .arg(root.join(MIRI_FIXTURE))
        .output()
        .context("start Miri on its fixture")?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains("Undefined Behavior"),
        "Miri admitted the out-of-bounds read in {MIRI_FIXTURE}, so the gate proves nothing"
    );
    println!(
        "Miri interpreted the tests of {} for the host and Windows without undefined behavior, and refused its fixture",
        MIRI_CRATES.join(", ")
    );
    Ok(())
}

/// Verifies one rule module and its probes, and returns how many harnesses it proved and how many probes it rejected.
/// The gate refuses a `kani list` inventory that differs from the harnesses the source declares, so the closure check and the verifier agree.
fn verify_rules(
    root: &Path,
    source: &str,
    probes: &str,
    flags: &BTreeSet<&'static str>,
) -> Result<(usize, usize)> {
    let path = crate::canonical(&root.join(source))?;
    let original = fs::read(&path)?;
    let declared = obligations::module(&String::from_utf8(original.clone())?)?.harnesses;
    let directory = tempfile::tempdir()?;
    let harnesses = verify_inventory(&path, directory.path(), flags)?;
    ensure!(
        harnesses == declared,
        "Kani lists other harnesses in {source} than the source declares: {harnesses:?} against {declared:?}"
    );
    let names: Vec<&str> = harnesses.iter().map(String::as_str).collect();
    verify_file(&path, directory.path(), &names, false, flags)?;
    let rejected = obligations::probe_harnesses(probes)?;
    let probe = tempfile::tempdir()?;
    let file = probe.path().join("counterexample.rs");
    fs::write(
        &file,
        format!(
            "#[path = {:?}]\nmod production;\n{probes}",
            path.to_string_lossy()
        ),
    )?;
    // Each bounded receipt must reject its exact probe on an assertion.
    let probe_names: Vec<&str> = rejected.iter().map(String::as_str).collect();
    if !probe_names.is_empty() {
        verify_file(&file, probe.path(), &probe_names, true, flags)?;
    }
    ensure!(
        fs::read(&path)? == original,
        "production proof source {source} changed during verification"
    );
    Ok((names.len(), rejected.len()))
}
