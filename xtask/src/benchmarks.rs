//! Valgrind measurements of the local policy gate's instruction, heap, and size ceilings.
//! The gungraun runner refuses a raised ceiling with its exit status 3, and a lowered measurement fails here until the baseline records it.

use crate::obligations::{self, Baselines, Metric, parse_baselines};
use crate::skill_proofs::{container_target_volume, proof_image};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use rules::obligation_rules::{Better, Standing, standing};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The gungraun release of the library, its runner, and the proof image.
pub const GUNGRAUN_VERSION: &str = "0.20.0";

/// The gungraun workspace of the repository, which holds the library and binary benchmarks.
pub const BENCHES: &str = "benchmarks";

/// The profile of the binary every host installs, whose size the baselines ceiling.
const PROFILE: &str = "release-guard";

/// The heap profiler's metric for the bytes live at its peak.
const DHAT_PEAK: &str = "attgmaxbytes";

/// The gungraun flag every run passes: the summaries as one JSON object per line, which the gate reads back.
const RUN_FLAGS: &str = "--output-format=json";
const VALGRIND_VERSION: &str = "valgrind-3.19.0";
const REGRESSION_STATUS: i32 = 3;
pub const INSTRUCTION_PROBE: &str = "DOTFILES_BENCHMARK_IR_PROBE";
pub const PROBE_CEILING: u64 = 1;

/// The permission to turn off address randomization for stable loader instruction counts.
const SECCOMP: &str = "--security-opt=seccomp=unconfined";

/// One benchmark's function, instruction count, and peak heap usage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Measured {
    pub function: String,
    pub instructions: Option<u64>,
    pub heap_peak: Option<u64>,
}

/// One profile's metric from a summary line, whose key the tool names and whose value `new` holds.
fn metric(profiles: &Value, tool: &str, metric: &str) -> Option<u64> {
    profiles.as_array()?.iter().find(|profile| {
        profile["tool"]
            .as_str()
            .is_some_and(|named| named.eq_ignore_ascii_case(tool))
    })?["data"]["total"]["metrics"]
        .as_object()?
        .iter()
        .find(|(key, _)| key.to_ascii_lowercase() == metric)?
        .1["values"]["new"]
        .as_u64()
}

/// The benchmarks of one `--output-format=json` stdout, which holds one summary object per line.
#[must_use]
pub fn measured(stdout: &str) -> Vec<Measured> {
    stdout
        .lines()
        .filter(|line| line.starts_with('{'))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|summary| Measured {
            function: summary["function_name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            instructions: metric(&summary["profiles"], "Callgrind", "ir"),
            heap_peak: metric(&summary["profiles"], "DHAT", DHAT_PEAK),
        })
        .filter(|entry| !entry.function.is_empty())
        .collect()
}

/// The benchmarks every run measures: three over the committed corpus and four through the gate's entry points.
pub const FUNCTIONS: [&str; rules::benchmark_rules::BENCHMARKS] = [
    "scan",
    "strip",
    "classify",
    "git_status",
    "pre_push_denied",
    "commit_message_clean",
    "commit_message_attribution",
];

/// Refuses a missing, repeated, or unknown benchmark, metric, or binary ceiling.
#[must_use]
pub fn coverage(baselines: &Baselines, measured: &[Measured]) -> Vec<String> {
    use rules::benchmark_rules::{Counts, complete};
    let count = |count: usize| u64::try_from(count).unwrap_or(u64::MAX);
    let counts = FUNCTIONS.map(|name| Counts {
        instructions: count(
            baselines
                .metrics
                .iter()
                .filter(|metric| matches!(metric, Metric::Ir { bench, .. } if bench == name))
                .count(),
        ),
        heap: count(
            baselines
                .metrics
                .iter()
                .filter(|metric| matches!(metric, Metric::HeapPeak { bench, .. } if bench == name))
                .count(),
        ),
        measurements: count(
            measured
                .iter()
                .filter(|entry| entry.function == name)
                .count(),
        ),
    });
    let binaries = count(
        baselines
            .metrics
            .iter()
            .filter(|metric| matches!(metric, Metric::BinarySize { .. }))
            .count(),
    );
    let unknown = measured
        .iter()
        .any(|entry| !FUNCTIONS.contains(&entry.function.as_str()))
        || baselines.metrics.iter().any(|metric| match metric {
            Metric::Ir { bench, .. } | Metric::HeapPeak { bench, .. } => {
                !FUNCTIONS.contains(&bench.as_str())
            }
            _ => false,
        });
    if complete(counts, binaries, unknown) {
        Vec::new()
    } else {
        vec![format!(
            "every benchmark needs exactly one measurement, one Ir ceiling, and one heap ceiling, and the binary needs one size ceiling: {counts:?}, binaries={binaries}, unknown={unknown}"
        )]
    }
}

/// Refusals for a moved or missing measurement in the named unit.
fn moved(name: &str, unit: &str, baseline: u64, value: Option<u64>) -> Vec<String> {
    let Some(value) = value else {
        return vec![format!("{name} has no measured {unit} in the summaries")];
    };
    if unit == "instructions" && rules::benchmark_rules::instructions_hold(baseline, value) {
        return Vec::new();
    }
    match standing(Better::Lower, baseline, value) {
        Standing::Holds => Vec::new(),
        Standing::Regressed => vec![format!(
            "{name} measures {value} {unit} instead of the recorded {baseline}; the gungraun runner refused it with its exit status 3"
        )],
        Standing::Improved => vec![format!(
            "{name} measures {value} {unit} instead of the recorded {baseline}; record {value} so it can't slip back"
        )],
    }
}

/// Refusals for moved measurements, missing ceilings, and missing benchmarks.
#[must_use]
pub fn budgets(baselines: &Baselines, measured: &[Measured], binary_size: u64) -> Vec<String> {
    let mut refusals = Vec::new();
    for entry in &baselines.metrics {
        match entry {
            Metric::Ir { bench, exact } => refusals.extend(moved(
                bench,
                "instructions",
                *exact,
                measured
                    .iter()
                    .find(|one| one.function == *bench)
                    .and_then(|one| one.instructions),
            )),
            Metric::HeapPeak { bench, exact } => refusals.extend(moved(
                bench,
                "heap bytes at its peak",
                *exact,
                measured
                    .iter()
                    .find(|one| one.function == *bench)
                    .and_then(|one| one.heap_peak),
            )),
            Metric::BinarySize { exact } => refusals.extend(moved(
                "the release-guard dotguard binary",
                "bytes",
                *exact,
                Some(binary_size),
            )),
            _ => {}
        }
    }
    let governed: Vec<&str> = baselines
        .metrics
        .iter()
        .filter_map(|entry| match entry {
            Metric::Ir { bench, .. } | Metric::HeapPeak { bench, .. } => Some(bench.as_str()),
            _ => None,
        })
        .collect();
    refusals.extend(
        measured
            .iter()
            .filter(|one| !governed.contains(&one.function.as_str()))
            .map(|one| {
                format!(
                    "the {} benchmark ran with no ceiling in {}; record one before it can run",
                    one.function,
                    obligations::BASELINES
                )
            }),
    );
    refusals
}

/// The `release-guard` binary the benchmarks run and the size gate measures.
fn binary(root: &Path) -> PathBuf {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    target.join(PROFILE).join("dotguard")
}

fn reject_raised_instructions(root: &Path) -> Result<()> {
    let output = Tool::Cargo
        .command()
        .current_dir(root)
        .args(["bench", "--locked", "--manifest-path"])
        .arg(root.join(BENCHES).join("Cargo.toml"))
        .args(["--bench", "library", "--no-run", "--message-format=json"])
        .output()?;
    ensure!(output.status.success(), "building the budget probe failed");
    let executable = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|artifact| {
            (artifact["reason"] == "compiler-artifact" && artifact["target"]["name"] == "library")
                .then(|| artifact["executable"].as_str().map(PathBuf::from))
                .flatten()
        })
        .context("cargo reported no library benchmark executable")?;
    let probe = crate::tool::external(&executable)
        .current_dir(root.join(BENCHES))
        .args(["--bench", "--regression-fail-fast"])
        .env(INSTRUCTION_PROBE, "1")
        .output()?;
    ensure!(
        probe.status.code() == Some(REGRESSION_STATUS),
        "the instruction probe {} returned {} instead of {REGRESSION_STATUS}: {}\n{}",
        executable.display(),
        probe.status,
        String::from_utf8_lossy(&probe.stdout),
        String::from_utf8_lossy(&probe.stderr)
    );
    Ok(())
}

/// Holds the gate's own work to the recorded ceilings: instructions and heap peak per benchmark, and the size of the installed binary.
///
/// # Errors
///
/// Returns an error for unavailable tools, failed benchmarks, or a moved measurement.
pub fn verify(root: &Path) -> Result<()> {
    let mut runner = crate::tool::external("gungraun-runner");
    let version = runner
        .arg("--version")
        .output()
        .context("start gungraun-runner; the proof image installs it")?;
    ensure!(
        version.status.success()
            && String::from_utf8_lossy(&version.stdout).contains(GUNGRAUN_VERSION),
        "the pinned gungraun-runner {GUNGRAUN_VERSION} is unavailable"
    );
    let valgrind = crate::tool::external("valgrind")
        .arg("--version")
        .output()
        .context("start valgrind; the proof image installs it")?;
    ensure!(
        valgrind.status.success()
            && String::from_utf8_lossy(&valgrind.stdout).trim() == VALGRIND_VERSION,
        "the pinned {VALGRIND_VERSION} is unavailable"
    );
    let binary = binary(root);
    let status = Tool::Cargo
        .command()
        .current_dir(root)
        .args([
            "build",
            "--locked",
            "--profile",
            PROFILE,
            "--package",
            "dotguard",
        ])
        .status()
        .context("build the release-guard dotguard binary")?;
    ensure!(status.success(), "building the release-guard binary failed");
    let size = std::fs::metadata(&binary)
        .with_context(|| format!("the built binary is missing at {}", binary.display()))?
        .len();
    let output = Tool::Cargo
        .command()
        .current_dir(root)
        .args(["bench", "--locked", "--manifest-path"])
        .arg(root.join(BENCHES).join("Cargo.toml"))
        .args(["--bench", "library", "--bench", "wrapper", "--"])
        // The flags land after `--`, so gungraun reads them and cargo passes no libtest harness.
        .args(RUN_FLAGS.split_whitespace())
        .env("DOTGUARD_BIN", &binary)
        .output()
        .context("start the gungraun benchmarks")?;
    std::io::stdout()
        .lock()
        .write_all(&output.stdout)
        .context("write the original gungraun summaries")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let measured = measured(&stdout);
    ensure!(
        output.status.success(),
        "the gungraun benchmarks failed for the {size}-byte binary: {measured:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let baselines = parse_baselines(&std::fs::read_to_string(root.join(obligations::BASELINES))?)?;
    let missing = coverage(&baselines, &measured);
    ensure!(missing.is_empty(), "{}", missing.join("\n"));
    let refusals = budgets(&baselines, &measured, size);
    ensure!(refusals.is_empty(), "{}", refusals.join("\n"));
    reject_raised_instructions(root)?;
    println!(
        "Held every gungraun benchmark and the {size}-byte release-guard binary to the recorded ceilings"
    );
    Ok(())
}

/// The gate the benchmarks container runs: copy the checkout and run only the budget gate of the proofs action.
const CONTAINER_BENCH_GATE: &str = "set -eo pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; exec cargo run --locked --manifest-path xtask/Cargo.toml -- benchmarks";
/// Runs the budget gate in the pinned Linux environment that holds its recorded ceilings.
///
/// # Errors
///
/// Returns an error when Docker can't build or run the image or the gate inside fails.
pub fn verify_in_container(root: &Path) -> Result<()> {
    let root = crate::canonical(root)?;
    let definition = root.join(crate::skill_proofs::PROOF_IMAGE);
    let image = proof_image(&root)?;
    crate::skill_proofs::ensure_image(&root, &definition, &image)?;
    let mut mount = std::ffi::OsString::from("type=bind,\"source=");
    mount.push(&root);
    mount.push("\",target=/source,readonly");
    let status = Tool::Docker
        .command()
        .args(["run", "--rm", SECCOMP, "--mount"])
        .arg(mount)
        .args([
            "--volume",
            &format!("{}:/cache", container_target_volume(&root)),
            "--volume",
            "dotfiles-proofs-registry:/usr/local/cargo/registry",
            "--env",
            "CARGO_TARGET_DIR=/cache/target",
            "--workdir",
            "/work",
            &image,
            "bash",
            "-ec",
            CONTAINER_BENCH_GATE,
        ])
        .stdin(Stdio::null())
        .status()
        .context("start the benchmarks container")?;
    ensure!(status.success(), "the containerized budget gate failed");
    Ok(())
}
