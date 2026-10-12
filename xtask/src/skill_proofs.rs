use crate::checks::Checks;
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
    let mut checks = Checks::default();
    checks.run("budget", || {
        ensure!(
            slow.is_empty(),
            "each proof harness must finish within {} ms; shrink its bounds or split it without weakening its assertions:\n{}",
            proof_budget_rules::BUDGET_MILLIS,
            slow.join("\n")
        );
        Ok(())
    });
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
    for result in results {
        let name = result["harness_id"].as_str().unwrap_or_default();
        checks.run(&format!("harness {name}"), || {
            if counterexample {
                validate_counterexample(result)
            } else {
                validate_proof(result)
            }
        });
    }
    checks.finish()
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
    let mut checks = Checks::default();
    for batch in expected.chunks(KANI_BATCH_SIZE) {
        checks.run(
            &format!("{} [{}]", source.display(), batch.join(", ")),
            || verify_batch(source, directory, batch, counterexample, flags),
        );
    }
    checks.finish()
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
    let mut checks = Checks::default();
    checks.run("Kani exit", || {
        ensure!(
            status.success() != counterexample,
            "Kani returned an unexpected outcome: {status}"
        );
        Ok(())
    });
    checks.run("Kani report", || {
        let result: Value = skill_ops::read_json(&output)?;
        validate_results(&result, expected, counterexample)
    });
    checks.finish()
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
    if crate::container::amd64_image_present(image)? {
        return Ok(());
    }
    let mut command = Tool::Docker.command();
    command
        .args([
            "build",
            "--platform",
            crate::container::LINUX_AMD64,
            "--tag",
            image,
            "--file",
        ])
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
        "--platform",
        crate::container::LINUX_AMD64,
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
            "DOTFILES_HOST_CACHE=/cache",
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

/// Runs the complete proof campaign, with fixed measurement budgets on Linux and in the Windows container.
///
/// # Errors
///
/// Returns an error when the pinned Kani version or Docker engine can't run, Docker can't build the verification image, or any proof or counterexample check fails.
pub fn verify(root: &Path) -> Result<()> {
    let mut checks = Checks::default();
    if cfg!(target_os = "macos") {
        println!("Mac proof campaign: Kani, Miri, fuzz, mutation, and models.");
        println!(
            "Fixed Linux x86_64 benchmark budgets require the Linux code-stage result from `dotfiles hosts check`."
        );
    }
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        checks.run("native proofs", || verify_native(root));
    } else {
        checks.run("container proofs", || verify_in_container(root));
    }
    if std::env::var_os(INSIDE_CONTAINER).is_some() {
        // The image pins Valgrind and the gungraun runner, so the budgets run here with them natively.
        checks.run("benchmarks", || crate::benchmarks::verify(root));
        return checks.finish();
    }
    // The instruction ceilings depend on the native x86_64 CPU's libc dispatch and the image.
    if cfg!(target_os = "linux") {
        checks.run("benchmarks", || {
            crate::benchmarks::verify_in_container(root)
        });
    }
    checks.run("models", || {
        let manifest = obligations::manifest(root)?;
        let mut models = Checks::default();
        models.run("Quint", || crate::quint::verify(root, &manifest));
        models.run("Alloy", || crate::alloy::verify(root, &manifest));
        models.run("Lean", || crate::lean::verify(root, &manifest));
        models.finish()
    });
    checks.finish()
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
    let manifest = obligations::manifest(root);
    let mut checks = Checks::default();
    checks.run("Kani", || {
        let manifest = manifest
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error:#}"))?;
        verify_kani(root, manifest)
    });
    checks.run("Miri", || verify_miri(root));
    checks.run("fuzz", || verify_fuzz(root));
    checks.run("mutation", || {
        let manifest = manifest
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error:#}"))?;
        verify_mutants(root, manifest)
    });
    checks.finish()
}

fn verify_kani(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    let version = Tool::Kani.command().arg("--version").output()?;
    ensure!(
        version.status.success()
            && String::from_utf8(version.stdout)?
                .contains(&format!("Kani Rust Verifier {KANI_VERSION}")),
        "the pinned Kani version is unavailable"
    );
    verify_kani_driver(root)?;
    let mut verified = 0;
    let mut rejected = 0;
    let mut checks = Checks::default();
    for obligation in &manifest.obligations {
        if let Obligation::Kani { source, probes } = obligation {
            checks.run(source, || {
                let flags = flags(manifest, source);
                let (proved, refused) = verify_rules(root, source, probes, &flags)?;
                verified += proved;
                rejected += refused;
                Ok(())
            });
        }
    }
    checks.finish()?;
    println!(
        "Verified all {verified} production policy harnesses with their reachable outcomes, and rejected all {rejected} counterexamples"
    );
    Ok(())
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

fn nightly_host(root: &Path) -> Result<String> {
    let output = Tool::Rustup
        .command()
        .current_dir(root)
        .args(["run", MIRI_TOOLCHAIN, Tool::Rustc.program(), "-vV"])
        .output()
        .context("query the pinned nightly host")?;
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
    nightly_cargo(root)
        .args([
            "fuzz",
            "run",
            "--target",
            host,
            "--fuzz-dir",
            directory,
            target,
        ])
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
    let host = nightly_host(root)?;
    let mut checks = Checks::default();
    for target in &targets {
        checks.run(&format!("fuzz target {target}"), || {
            let corpus = root.join(FUZZ_DIRECTORY).join("corpus").join(target);
            ensure!(
                fs::read_dir(&corpus).is_ok_and(|mut entries| entries.next().is_some()),
                "the fuzz target {target} has no corpus under {}",
                corpus.display()
            );
            let output = replay(root, FUZZ_DIRECTORY, target, &host)?;
            ensure!(
                output.status.success(),
                "replaying the corpus of {target} failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        });
    }
    checks.run("fuzz fixture", || {
        let output = replay(root, FUZZ_FIXTURE, "crash", &host)?;
        ensure!(
            !output.status.success() && String::from_utf8_lossy(&output.stderr).contains("panicked"),
            "the corpus replay admitted {FUZZ_FIXTURE}, whose corpus makes its target panic, so the gate proves nothing"
        );
        Ok(())
    });
    checks.finish()?;
    println!(
        "Replayed the corpus of every fuzz target, {}, and refused its fixture",
        targets.join(", ")
    );
    Ok(())
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
const HOST_CACHE_VARIABLE: &str = "DOTFILES_HOST_CACHE";

fn mutants(directory: &Path, output: &Path) -> std::process::Command {
    mutants_with_cores(
        directory,
        output,
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
    )
}

fn mutants_with_cores(directory: &Path, output: &Path, available: usize) -> std::process::Command {
    let mut command = Tool::Cargo.command();
    let jobs = proof_budget_rules::mutant_jobs(available).to_string();
    // Each mutant builds in its own copy of the tree, which a shared target directory would serialize.
    command
        .env_remove("CARGO_TARGET_DIR")
        .env("RUST_TEST_THREADS", MUTANT_TEST_THREADS)
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

fn completed_mutation_report(
    output: &Path,
    status: std::process::ExitStatus,
    package: &str,
) -> Result<MutationReport> {
    let reports = output.join("mutants.out");
    let inventory: Vec<MutationIdentity> =
        serde_json::from_slice(&fs::read(reports.join("mutants.json"))?)?;
    let report: MutationReport = serde_json::from_slice(&fs::read(reports.join("outcomes.json"))?)?;
    validate_mutation_report(&inventory, report, status.code(), package)
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
    let mut checks = Checks::default();
    checks.run("production mutants", || {
        verify_mutant_results(root, manifest)
    });
    checks.run("mutation fixture", || {
        mutation_campaign(root, |output| {
            let status = mutants(&root.join(MUTANTS_FIXTURE), output)
                .status()
                .context("start cargo-mutants on its fixture")?;
            let report = completed_mutation_report(output, status, MUTANTS_FIXTURE_PACKAGE)?;
            ensure!(
                report.missed != 0,
                "the mutation gate admitted {MUTANTS_FIXTURE}, whose specification asserts nothing, so the gate proves nothing"
            );
            Ok(())
        })
    });
    checks.finish()?;
    println!(
        "Every mutant of {MUTATED_PACKAGE} died or holds its baseline, and the fixture without an assertion let its mutants survive"
    );
    Ok(())
}

fn verify_mutant_results(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    mutation_campaign(root, |output| check_mutant_results(root, manifest, output))
}

fn check_mutant_results(
    root: &Path,
    manifest: &obligations::Manifest,
    output: &Path,
) -> Result<()> {
    let status = mutants(root, output)
        .args(["--package", MUTATED_PACKAGE])
        .status()
        .context("start cargo-mutants")?;
    let report = completed_mutation_report(output, status, MUTATED_PACKAGE)?;
    ensure!(
        report.caught != 0,
        "cargo-mutants caught no mutant of {MUTATED_PACKAGE}, so the campaign didn't run: {status}"
    );
    let missed = mutation_names(&report, MutationSummary::MissedMutant).join("\n");
    let timeouts = mutation_names(&report, MutationSummary::Timeout).join("\n");
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

/// The tests in one Miri invocation, whose whole binary must meet the proof budget.
const MIRI_BATCH_TESTS: usize = 32;

/// The crate with undefined behavior that Miri must refuse.
const MIRI_FIXTURE: &str = "xtask/proofs/miri-fixture/Cargo.toml";
const MIRI_CHECKOUT_TARGET: &str = "target/miri";

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
    const NANOS_PER_MILLI: u32 = 1_000_000;
    const ROUNDING_TICK: std::time::Duration = std::time::Duration::from_nanos(1);
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
                    .context("missing Miri duration")?;
                let millis = seconds
                    .as_f64()
                    .filter(|seconds| !seconds.is_sign_negative())
                    .and_then(|seconds| {
                        let duration = std::time::Duration::try_from_secs_f64(seconds).ok()?;
                        let duration = if duration.as_secs_f64() < seconds {
                            duration.checked_add(ROUNDING_TICK)?
                        } else {
                            duration
                        };
                        u64::try_from(duration.as_millis())
                            .ok()?
                            .checked_add(u64::from(duration.subsec_nanos() % NANOS_PER_MILLI != 0))
                    });
                ensure!(
                    proof_budget_rules::verdict(millis) == proof_budget_rules::Verdict::Within,
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

fn nightly_cargo(root: &Path) -> std::process::Command {
    let mut command = Tool::Rustup.command();
    command
        .current_dir(root)
        .args(["run", MIRI_TOOLCHAIN, Tool::Cargo.program()]);
    command
}

fn miri(root: &Path) -> std::process::Command {
    let mut command = nightly_cargo(root);
    let target = std::env::var_os(platform::code_environment::MIRI_TARGET_VARIABLE)
        .map_or_else(|| root.join(MIRI_CHECKOUT_TARGET), std::path::PathBuf::from);
    command
        .args(["miri", "test", "--locked", "--target-dir"])
        .arg(target);
    command
}

/// Interprets the pure crates' tests under Miri for every target, each test binary within the proof budget, and refuses a fixture with undefined behavior.
fn verify_miri(root: &Path) -> Result<()> {
    let mut checks = Checks::default();
    for &target in MIRI_TARGETS {
        for krate in MIRI_CRATES {
            checks.run(
                &format!("Miri {krate} for {}", target.unwrap_or("the host")),
                || verify_miri_crate(root, krate, target),
            );
        }
    }
    checks.run("Miri fixture", || {
        let output = miri(root)
            .arg("--manifest-path")
            .arg(root.join(MIRI_FIXTURE))
            .output()
            .context("start Miri on its fixture")?;
        ensure!(
            !output.status.success()
                && String::from_utf8_lossy(&output.stderr).contains("Undefined Behavior"),
            "Miri did not report undefined behavior in {MIRI_FIXTURE}: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    });
    checks.finish()?;
    println!(
        "Miri interpreted the tests of {} for {MIRI_TARGETS:?} without undefined behavior, and refused its fixture",
        MIRI_CRATES.join(", ")
    );
    Ok(())
}

fn verify_miri_crate(root: &Path, krate: &str, target: Option<&str>) -> Result<()> {
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
    let batches = miri_batches(&String::from_utf8(output.stdout)?)?;
    let mut checks = Checks::default();
    for batch in batches {
        let names: Vec<_> = batch.iter().map(|test| test.name.as_str()).collect();
        checks.run(&format!("tests [{}]", names.join(", ")), || {
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
                .args(&names)
                .output()
                .context("start Miri")?;
            let report = String::from_utf8_lossy(&output.stdout);
            ensure!(
                output.status.success(),
                "Miri refused the tests of {krate} for {}:\n{report}\n{}",
                target.unwrap_or("the host"),
                String::from_utf8_lossy(&output.stderr)
            );
            validate_miri_batch(&report, &batch)?;
            let ignored = batch.iter().filter(|test| test.ignored).count();
            println!(
                "Miri verified {} tests of {krate} for {} within the proof budget and retained {ignored} existing ignores",
                batch.len() - ignored,
                target.unwrap_or("the host")
            );
            Ok(())
        });
    }
    checks.finish()
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
    let mut checks = Checks::default();
    checks.run("production harnesses", || {
        verify_file(&path, directory.path(), &names, false, flags)
    });
    let mut rejected = 0;
    checks.run("counterexamples", || {
        let harnesses = obligations::probe_harnesses(probes)?;
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
        let probe_names: Vec<&str> = harnesses.iter().map(String::as_str).collect();
        if !probe_names.is_empty() {
            verify_file(&file, probe.path(), &probe_names, true, flags)?;
        }
        rejected = harnesses.len();
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
    Ok((names.len(), rejected))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE_FIXTURE: &str = "DOTFILES_NATIVE_PROOF_FIXTURE";
    const NATIVE_TEST: &str =
        "skill_proofs::tests::missing_manifest_cannot_skip_independent_native_checks";
    const NIGHTLY_TEST: &str =
        "skill_proofs::tests::the_code_environment_dispatches_the_pinned_nightly_verifiers";
    const MUTATION_TEST: &str =
        "skill_proofs::tests::mutation_workers_do_not_multiply_the_test_pool";

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
            let command = mutants_with_cores(Path::new("source"), Path::new("output"), cores);
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
    #[ignore = "requires the installed pinned cargo-mutants"]
    fn mutation_workers_do_not_multiply_the_test_pool() -> Result<()> {
        let root = crate::canonical(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").as_path())?;
        if std::env::var(NATIVE_FIXTURE).is_ok_and(|value| value == MUTATION_TEST) {
            return mutation_campaign(&root, |output| {
                let status = mutants(&root.join(MUTANTS_FIXTURE), output)
                    .status()
                    .context("start the mutation worker fixture")?;
                let report = completed_mutation_report(output, status, MUTANTS_FIXTURE_PACKAGE)?;
                ensure!(
                    report.missed != 0,
                    "the mutation worker fixture failed its baseline or caught its assertionless rule"
                );
                Ok(())
            });
        }
        let home = platform::home::home().context("native tool home")?;
        let environment = dotfiles::hosts::code_environment(&root, &home)?;
        let output = environment
            .command(std::env::current_exe()?)
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
                        let mut command = miri(&root);
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
            .command(std::env::current_exe()?)
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
            let failure = verify_native(root.path()).unwrap_err();
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
