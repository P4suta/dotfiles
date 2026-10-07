use crate::obligations::{self, KaniFlag, Obligation};
use crate::proof_budget_rules;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Stdio;

pub const KANI_VERSION: &str = "0.68.0";

/// Every proof covers a reachable acceptance and a reachable refusal.
const WITNESSES: usize = 2;

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
    } else {
        ensure!(
            summary["failed"] == 0 && summary["successful"] == expected.len(),
            "required proof failed"
        );
        for result in results {
            ensure!(
                result["status"] == "Success",
                "proof was unsupported, skipped, or failed"
            );
            let checks = result["checks"]
                .as_array()
                .context("missing proof properties")?;
            ensure!(
                checks.iter().any(|check| check["category"] == "assertion"),
                "proof has no assertion"
            );
            ensure!(
                checks
                    .iter()
                    .filter(|check| check["category"] == "cover" && check["status"] == "Satisfied")
                    .count()
                    >= WITNESSES,
                "proof lacks reachable acceptance and refusal witnesses"
            );
            ensure!(
                checks.iter().all(|check| if check["category"] == "cover" {
                    check["status"] == "Satisfied"
                } else {
                    check["status"] == "Success"
                }),
                "proof property is unverified"
            );
        }
    }
    Ok(())
}

/// The unstable features a Kani run enables: stubbing always, and the ones a contract obligation adds.
fn features(flags: &BTreeSet<&'static str>) -> Vec<&'static str> {
    flags.iter().flat_map(|flag| ["-Z", flag]).collect()
}

fn verify_file(
    source: &Path,
    directory: &Path,
    expected: &[&str],
    counterexample: bool,
    flags: &BTreeSet<&'static str>,
) -> Result<()> {
    let output = directory.join("results.json");
    let mut command = Tool::Kani.command();
    command
        .current_dir(directory)
        .arg(source)
        .args(["--output-format", "terse", "-Z", "unstable-options"])
        .args(features(flags))
        .arg("--harness-timeout")
        .arg(format!("{}s", proof_budget_rules::TIMEOUT_SECONDS))
        .arg("--export-json")
        .arg(&output);
    if counterexample {
        for name in expected {
            command.args(["--harness", name]);
        }
        command.arg("--exact");
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
    let status = Tool::Kani
        .command()
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
    let mut arguments: Vec<OsString> = ["run", "--rm", "--mount"].map(OsString::from).into();
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
    let engine = Tool::Docker
        .command()
        .args(["version", "--format", "{{.Server.Os}}"])
        .stderr(Stdio::null())
        .output()
        .context("Kani proofs on this host run in a Linux container; install Docker")?;
    ensure!(
        engine.status.success() && String::from_utf8(engine.stdout)?.trim() == "linux",
        "Kani proofs on this host need a running Docker engine with Linux containers"
    );
    let definition = root.join("xtask/proofs/Dockerfile");
    let image = container_image(&fs::read(&definition)?);
    let present = Tool::Docker
        .command()
        .args(["image", "inspect"])
        .arg(&image)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !present.success() {
        let status = Tool::Docker
            .command()
            .args(["build", "--tag"])
            .arg(&image)
            .arg("--file")
            .arg(&definition)
            .arg(root.join("xtask/proofs"))
            .status()
            .context("build the pinned Kani verification image")?;
        ensure!(
            status.success(),
            "cannot build the pinned Kani verification image: {status}"
        );
    }
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
    // The Quint models run in their own pinned image without a network on every host, so the container gate leaves them out.
    if std::env::var_os(INSIDE_CONTAINER).is_none() {
        crate::quint::verify(root, &obligations::manifest(root)?)?;
    }
    Ok(())
}

/// Set inside the Kani container, whose gate leaves the Quint models to the host that started it.
const INSIDE_CONTAINER: &str = "DOTFILES_PROOFS_CONTAINER";

fn verify_native(root: &Path) -> Result<()> {
    let version = Tool::Kani.command().arg("--version").output()?;
    ensure!(
        version.status.success()
            && String::from_utf8(version.stdout)?
                .contains(&format!("Kani Rust Verifier {KANI_VERSION}")),
        "the pinned Kani version is unavailable"
    );
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
    verify_mutants(root, &manifest)
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

/// The crate with undefined behavior that Miri must refuse.
const MIRI_FIXTURE: &str = "xtask/proofs/miri-fixture/Cargo.toml";

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
            let output = command.output().context("start Miri")?;
            let report = String::from_utf8_lossy(&output.stdout);
            ensure!(
                output.status.success(),
                "Miri refused the tests of {krate} for {}:\n{report}\n{}",
                target.unwrap_or("the host"),
                String::from_utf8_lossy(&output.stderr)
            );
            let durations = test_durations(&report);
            ensure!(!durations.is_empty(), "Miri ran no test binary of {krate}");
            ensure!(
                durations.iter().all(|duration| {
                    proof_budget_rules::verdict(*duration) == proof_budget_rules::Verdict::Within
                }),
                "the Miri tests of {krate} for {} must each finish within {} ms; split a slow test without weakening it:\n{report}",
                target.unwrap_or("the host"),
                proof_budget_rules::BUDGET_MILLIS
            );
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
    // One run compiles the module once for every probe, and the result check still requires each one to fail on an assertion.
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
