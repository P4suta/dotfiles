//! Valgrind measurements of the local policy gate's instruction, heap, and size ceilings.
//! The gungraun runner refuses a raised ceiling with its exit status 3, and a lowered measurement fails here until the baseline records it.

use crate::obligations::{self, Baselines, Metric, parse_baselines};
use crate::skill_proofs::{container_target_volume, proof_image};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use rules::obligation_rules::{Better, Standing, standing};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
const CONTAINER_BUILD_JOBS: &str = "CARGO_BUILD_JOBS=2";
const TRANSPORT_SCHEMA: u32 = 1;
const REQUEST_FILE: &str = "request.json";
const REPORT_FILE: &str = "report.json";
const DIAGNOSTICS_FILE: &str = "diagnostics.json";
const BINARY_FILE: &str = "dotguard";
const CONTAINER_SOURCE: &str = "/source";
const CONTAINER_REQUEST: &str = "/benchmark-request.json";
const CONTAINER_RESULT: &str = "/result";
const CONTAINER_EVIDENCE: &str = "/tmp/benchmark-evidence";
const EVIDENCE_DIRECTORY: &str = "proof-evidence";
const RESULT_CACHE: &str = "DOTFILES_HOST_CACHE";
const MAX_REQUEST_BYTES: u64 = 8 * 1024;
const MAX_EXECUTION_NAME_BYTES: usize = 256;
const OBSERVATION_BENCHES: [&str; 2] = ["library", "wrapper"];
const COMMAND_ENDPOINT_ARGUMENTS: usize = 2;
const HEX_DIGITS_PER_BYTE: usize = 2;
const MAX_RESULT_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RESULT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_RESULT_ENTRIES: usize = 256;
const OBSERVATION_FILE: &str = "observation.json";
const OBSERVATION_GATE: &str = "set -eo pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; exec cargo run --locked --offline --manifest-path xtask/Cargo.toml -- benchmarks-observe-report";
const SOURCE_INPUTS: [&str; 10] = [
    ".cargo",
    "Cargo.toml",
    "Cargo.lock",
    "mise.toml",
    "mise.lock",
    "benchmarks",
    "crates",
    "xtask",
    "policy",
    "home",
];

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

fn raised_instruction_reports(
    root: &Path,
) -> Result<(
    crate::phase_evidence::RawResult,
    crate::phase_evidence::RawResult,
)> {
    let output = crate::phase_evidence::output(
        Tool::Cargo
            .command()
            .current_dir(root)
            .args(["bench", "--locked", "--manifest-path"])
            .arg(root.join(BENCHES).join("Cargo.toml"))
            .args(["--bench", "library", "--no-run", "--message-format=json"]),
        "benchmark rejecting probe build",
    )?;
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
    let probe = crate::phase_evidence::output(
        crate::tool::external(&executable)
            .current_dir(root.join(BENCHES))
            .args(["--bench", "--regression-fail-fast"])
            .env_remove(crate::benchmark_profile::PROFILE_ENV)
            .env(INSTRUCTION_PROBE, "1"),
        "benchmark raised-instruction rejecting probe",
    )?;
    ensure!(
        probe.status.code() == Some(REGRESSION_STATUS),
        "the instruction probe {} returned {} instead of {REGRESSION_STATUS}: {}\n{}",
        executable.display(),
        probe.status,
        String::from_utf8_lossy(&probe.stdout),
        String::from_utf8_lossy(&probe.stderr)
    );
    Ok((output.into(), probe.into()))
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reports {
    runner: crate::phase_evidence::RawResult,
    valgrind: crate::phase_evidence::RawResult,
    build: crate::phase_evidence::RawResult,
    binary_hash: String,
    binary_size: u64,
    measurements: crate::phase_evidence::RawResult,
    probe_build: crate::phase_evidence::RawResult,
    probe: crate::phase_evidence::RawResult,
}

pub(crate) fn validate(root: &Path, reports: &Reports) -> Result<()> {
    let binary = binary(root);
    let baselines = parse_baselines(&std::fs::read_to_string(root.join(obligations::BASELINES))?)?;
    validate_measurements(
        &baselines,
        std::fs::metadata(&binary)?.len(),
        &crate::phase_evidence::verifier_digest(&binary)?,
        reports,
    )
}

fn validate_measurements(
    baselines: &Baselines,
    binary_size: u64,
    binary_hash: &str,
    reports: &Reports,
) -> Result<()> {
    ensure!(
        reports.runner.success() && reports.runner.stdout()?.contains(GUNGRAUN_VERSION),
        "benchmark runner version differs"
    );
    ensure!(
        reports.valgrind.success() && reports.valgrind.stdout()?.trim() == VALGRIND_VERSION,
        "benchmark Valgrind version differs"
    );
    ensure!(
        reports.build.success() && reports.measurements.success(),
        "benchmark build or execution did not succeed"
    );
    ensure!(
        reports.binary_size == binary_size && reports.binary_hash == binary_hash,
        "measured binary size or bytes differ"
    );
    let measured = measured(reports.measurements.stdout()?);
    let missing = coverage(baselines, &measured);
    ensure!(missing.is_empty(), "{}", missing.join("\n"));
    let refusals = budgets(baselines, &measured, reports.binary_size);
    ensure!(refusals.is_empty(), "{}", refusals.join("\n"));
    ensure!(
        reports.probe_build.success() && reports.probe.status == Some(REGRESSION_STATUS),
        "benchmark instruction defect was not rejected by the actual runner"
    );
    Ok(())
}

/// Holds the gate's own work to the recorded ceilings: instructions and heap peak per benchmark, and the size of the installed binary.
///
/// # Errors
///
/// Returns an error for unavailable tools, failed benchmarks, or a moved measurement.
pub fn verify(root: &Path) -> Result<()> {
    let reports = collect(root)?;
    validate(root, &reports)?;
    println!(
        "Held every gungraun benchmark and the {}-byte release-guard binary to the recorded ceilings",
        reports.binary_size
    );
    Ok(())
}

pub(crate) fn collect(root: &Path) -> Result<Reports> {
    let mut runner = crate::tool::external("gungraun-runner");
    let version =
        crate::phase_evidence::output(runner.arg("--version"), "gungraun-runner version")?;
    ensure!(
        version.status.success()
            && String::from_utf8_lossy(&version.stdout).contains(GUNGRAUN_VERSION),
        "the pinned gungraun-runner {GUNGRAUN_VERSION} is unavailable"
    );
    let valgrind = crate::phase_evidence::output(
        crate::tool::external("valgrind").arg("--version"),
        "Valgrind version",
    )?;
    ensure!(
        valgrind.status.success()
            && String::from_utf8_lossy(&valgrind.stdout).trim() == VALGRIND_VERSION,
        "the pinned {VALGRIND_VERSION} is unavailable"
    );
    let binary = binary(root);
    let build = crate::phase_evidence::output(
        Tool::Cargo.command().current_dir(root).args([
            "build",
            "--locked",
            "--profile",
            PROFILE,
            "--package",
            "dotguard",
        ]),
        "release-guard binary build",
    )?;
    ensure!(
        build.status.success(),
        "building the release-guard binary failed"
    );
    let size = std::fs::metadata(&binary)
        .with_context(|| format!("the built binary is missing at {}", binary.display()))?
        .len();
    let output = crate::phase_evidence::output(
        Tool::Cargo
            .command()
            .current_dir(root)
            .args(["bench", "--locked", "--manifest-path"])
            .arg(root.join(BENCHES).join("Cargo.toml"))
            .args(["--bench", "library", "--bench", "wrapper", "--"])
            // The flags land after `--`, so gungraun reads them and cargo passes no libtest harness.
            .args(RUN_FLAGS.split_whitespace())
            .env_remove(crate::benchmark_profile::PROFILE_ENV)
            .env("DOTGUARD_BIN", &binary),
        "gungraun benchmark measurements",
    )?;
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
    let binary_hash = crate::phase_evidence::verifier_digest(&binary)?;
    let (probe_build, probe) = raised_instruction_reports(root)?;
    Ok(Reports {
        runner: version.into(),
        valgrind: valgrind.into(),
        build: build.into(),
        binary_hash,
        binary_size: size,
        measurements: output.into(),
        probe_build,
        probe,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportRequest {
    schema: u32,
    execution: String,
    image_id: String,
    target: String,
    source_hash: String,
    invocation_hash: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportReport {
    request: TransportRequest,
    reports_hash: String,
    diagnostics_hash: String,
    reports: Reports,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRequest {
    execution: TransportRequest,
    metadata_only: bool,
    host_info: crate::phase_evidence::RawResult,
    host_version: crate::phase_evidence::RawResult,
    configuration: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedBench {
    name: String,
    build: crate::phase_evidence::RawResult,
    measurement: Option<crate::phase_evidence::RawResult>,
    executable: Option<PathBuf>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    request: ObservationRequest,
    metadata: crate::benchmark_profile::Metadata,
    metadata_after: Option<crate::benchmark_profile::Metadata>,
    runner: Option<crate::phase_evidence::RawResult>,
    valgrind: Option<crate::phase_evidence::RawResult>,
    binary_build: Option<crate::phase_evidence::RawResult>,
    binary_hash: Option<String>,
    binary_size: Option<u64>,
    benches: Vec<ObservedBench>,
    probe: Option<crate::phase_evidence::RawResult>,
}

fn benchmark_executable(output: &crate::phase_evidence::RawResult, name: &str) -> Result<PathBuf> {
    let executables: Vec<_> = output
        .stdout()?
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|value| value["reason"] == "compiler-artifact" && value["target"]["name"] == name)
        .filter_map(|value| value["executable"].as_str().map(PathBuf::from))
        .collect();
    ensure!(
        executables.len() == 1,
        "cargo reported no unique {name} benchmark executable"
    );
    executables
        .into_iter()
        .next()
        .context("benchmark executable is absent")
}

fn observe_bench(root: &Path, name: &str, binary: &Path) -> Result<ObservedBench> {
    let build: crate::phase_evidence::RawResult = crate::phase_evidence::output(
        Tool::Cargo
            .command()
            .current_dir(root)
            .args(["bench", "--locked", "--offline", "--manifest-path"])
            .arg(root.join(BENCHES).join("Cargo.toml"))
            .args(["--bench", name, "--no-run", "--message-format=json"]),
        &format!("observed {name} benchmark build"),
    )?
    .into();
    let executable = if build.success() {
        Some(benchmark_executable(&build, name)?)
    } else {
        None
    };
    let measurement = executable
        .as_ref()
        .map(|executable| {
            crate::phase_evidence::output(
                crate::tool::external(executable)
                    .current_dir(root.join(BENCHES))
                    .args(["--bench", RUN_FLAGS])
                    .env_remove(crate::benchmark_profile::PROFILE_ENV)
                    .env_remove(INSTRUCTION_PROBE)
                    .env("DOTGUARD_BIN", binary),
                &format!("observed {name} benchmark measurements"),
            )
            .map(Into::into)
        })
        .transpose()?;
    Ok(ObservedBench {
        name: name.into(),
        build,
        measurement,
        executable,
    })
}

fn observe_measurements(root: &Path, observation: &mut Observation) -> Result<()> {
    observation.runner = Some(
        crate::phase_evidence::output(
            crate::tool::external("gungraun-runner").arg("--version"),
            "observed gungraun runner",
        )?
        .into(),
    );
    observation.valgrind = Some(
        crate::phase_evidence::output(
            crate::tool::external("valgrind").arg("--version"),
            "observed Valgrind",
        )?
        .into(),
    );
    observation.binary_build = Some(
        crate::phase_evidence::output(
            Tool::Cargo.command().current_dir(root).args([
                "build",
                "--locked",
                "--offline",
                "--profile",
                PROFILE,
                "--package",
                "dotguard",
            ]),
            "observed release-guard binary build",
        )?
        .into(),
    );
    let binary = binary(root);
    if observation
        .binary_build
        .as_ref()
        .is_some_and(crate::phase_evidence::RawResult::success)
        && binary.is_file()
    {
        observation.binary_hash = Some(crate::phase_evidence::verifier_digest(&binary)?);
        observation.binary_size = Some(fs::metadata(&binary)?.len());
    }
    let library = observe_bench(root, "library", &binary);
    let wrapper = observe_bench(root, "wrapper", &binary);
    observation.benches = vec![library?, wrapper?];
    observation.probe = observation
        .benches
        .iter()
        .find(|bench| bench.name == "library")
        .and_then(|bench| bench.executable.as_ref())
        .map(|executable| {
            crate::phase_evidence::output(
                crate::tool::external(executable)
                    .current_dir(root.join(BENCHES))
                    .args(["--bench", "--regression-fail-fast", RUN_FLAGS])
                    .env_remove(crate::benchmark_profile::PROFILE_ENV)
                    .env(INSTRUCTION_PROBE, "1"),
                "observed actual raised-instruction rejecting probe",
            )
            .map(Into::into)
        })
        .transpose()?;
    observation.metadata_after = Some(crate::benchmark_profile::collect()?);
    Ok(())
}

fn validate_observation(observation: &Observation, expected: &ObservationRequest) -> Result<()> {
    validate_request(&expected.execution)?;
    ensure!(
        expected.configuration == observation_configuration()?,
        "observation configuration differs"
    );
    ensure!(
        serde_json::to_vec(&observation.request)? == serde_json::to_vec(expected)?,
        "observation belongs to another source or execution"
    );
    crate::benchmark_profile::processor_hash(&observation.metadata)?;
    ensure!(
        observation.metadata.libc.success()
            && observation.metadata.rustc.success()
            && observation.metadata.kernel.success()
            && !observation.metadata.rust_features.is_empty(),
        "guest toolchain or runtime observation failed"
    );
    if expected.metadata_only {
        ensure!(
            observation.benches.is_empty()
                && observation.probe.is_none()
                && observation.metadata_after.is_none()
                && observation.binary_build.is_none(),
            "metadata-only observation unexpectedly ran measurements"
        );
        return Ok(());
    }
    validate_observed_versions(observation)?;
    ensure!(
        observation
            .binary_build
            .as_ref()
            .is_some_and(crate::phase_evidence::RawResult::success)
            && observation.binary_hash.is_some()
            && observation.binary_size.is_some(),
        "observed binary is missing"
    );
    ensure!(
        observation.benches.len() == OBSERVATION_BENCHES.len()
            && OBSERVATION_BENCHES.into_iter().all(|name| observation
                .benches
                .iter()
                .filter(|bench| bench.name == name)
                .count()
                == 1),
        "observed benchmark target inventory is incomplete"
    );
    let mut measurements = Vec::new();
    for bench in &observation.benches {
        let result = bench
            .measurement
            .as_ref()
            .context("observed benchmark did not execute")?;
        ensure!(
            bench.build.success() && matches!(result.status, Some(0 | REGRESSION_STATUS)),
            "observed benchmark failed or was interrupted"
        );
        measurements.extend(measured(result.stdout()?));
    }
    ensure!(
        measurements.len() == FUNCTIONS.len()
            && FUNCTIONS.iter().all(|name| measurements
                .iter()
                .filter(|value| value.function == *name
                    && value.instructions.is_some()
                    && value.heap_peak.is_some())
                .count()
                == 1),
        "observation lacks an exact instruction and heap inventory"
    );
    validate_observed_probe(
        observation
            .probe
            .as_ref()
            .context("observed instruction probe is missing")?,
    )?;
    Ok(())
}

fn validate_observed_probe(probe: &crate::phase_evidence::RawResult) -> Result<()> {
    ensure!(
        probe.status == Some(REGRESSION_STATUS)
            && measured(probe.stdout()?).iter().any(|value| value
                .instructions
                .is_some_and(|instructions| instructions > PROBE_CEILING)),
        "observed instruction defect has no original measured instruction rejection"
    );
    Ok(())
}

fn validate_observed_versions(observation: &Observation) -> Result<()> {
    ensure!(
        observation
            .runner
            .as_ref()
            .is_some_and(|value| value.success()
                && value
                    .stdout()
                    .is_ok_and(|text| text.contains(GUNGRAUN_VERSION)))
            && observation
                .valgrind
                .as_ref()
                .is_some_and(|value| value.success()
                    && value
                        .stdout()
                        .is_ok_and(|text| text.trim() == VALGRIND_VERSION)),
        "observation used unavailable verifier versions"
    );
    let request = &observation.request;
    let before = crate::benchmark_profile::identity(
        &observation.metadata,
        &request.execution.image_id,
        &request.host_info,
        &request.host_version,
        &request.configuration,
    )?;
    let after = crate::benchmark_profile::identity(
        observation
            .metadata_after
            .as_ref()
            .context("final processor and runtime readback is missing")?,
        &request.execution.image_id,
        &request.host_info,
        &request.host_version,
        &request.configuration,
    )?;
    ensure!(
        before == after,
        "processor, runtime or tools changed during benchmark observation"
    );
    Ok(())
}

/// Exports fresh original observations without granting any budget verification credit.
///
/// # Errors
///
/// Refuses stale source, malformed requests, missing metrics, failed tools, or incomplete controls.
pub fn write_observation_report(root: &Path) -> Result<()> {
    ensure!(
        cfg!(target_os = "linux"),
        "benchmark observations require Linux"
    );
    let request: ObservationRequest = serde_json::from_slice(&read_bounded(
        Path::new(CONTAINER_REQUEST),
        MAX_REQUEST_BYTES,
    )?)?;
    crate::phase_evidence::run(
        root,
        "non-verifying guest benchmark observation",
        if request.metadata_only {
            0
        } else {
            FUNCTIONS.len()
        },
        || produce_observation(root, &request),
    )
}

fn produce_observation(root: &Path, request: &ObservationRequest) -> Result<()> {
    validate_request(&request.execution)?;
    ensure!(
        request.configuration == observation_configuration()?,
        "observation configuration differs"
    );
    validate_sources(root, Path::new(CONTAINER_SOURCE), &request.execution)?;
    let mut observation = Observation {
        request: request.clone(),
        metadata: crate::benchmark_profile::collect()?,
        metadata_after: None,
        runner: None,
        valgrind: None,
        binary_build: None,
        binary_hash: None,
        binary_size: None,
        benches: Vec::new(),
        probe: None,
    };
    let measured = if request.metadata_only {
        Ok(())
    } else {
        crate::benchmark_profile::identity(
            &observation.metadata,
            &request.execution.image_id,
            &request.host_info,
            &request.host_version,
            &request.configuration,
        )
        .and_then(|_| observe_measurements(root, &mut observation))
    };
    let result = Path::new(CONTAINER_RESULT);
    create_result(
        &result.join(OBSERVATION_FILE),
        &serde_json::to_vec(&observation)?,
        MAX_RESULT_FILE_BYTES,
    )?;
    if observation
        .binary_build
        .as_ref()
        .is_some_and(crate::phase_evidence::RawResult::success)
    {
        create_result(
            &result.join(BINARY_FILE),
            &read_bounded(&binary(root), MAX_RESULT_FILE_BYTES)?,
            MAX_RESULT_FILE_BYTES,
        )?;
    }
    measured?;
    validate_sources(root, Path::new(CONTAINER_SOURCE), &request.execution)?;
    validate_observation(&observation, request)
}

fn observed_host() -> Result<(
    crate::phase_evidence::RawResult,
    crate::phase_evidence::RawResult,
)> {
    let info: crate::phase_evidence::RawResult = crate::phase_evidence::output(
        Tool::Docker.command().args(["info", "--format", "{\"architecture\":{{json .Architecture}},\"operating_system\":{{json .OperatingSystem}},\"kernel\":{{json .KernelVersion}},\"engine\":{{json .ServerVersion}}}"]),
        "Mac benchmark daemon runtime identity",
    )?.into();
    let version: crate::phase_evidence::RawResult = crate::phase_evidence::output(
        Tool::Docker
            .command()
            .args(["version", "--format", "{{json .Client}}"]),
        "Mac benchmark client runtime identity",
    )?
    .into();
    ensure!(
        info.success() && version.success(),
        "Docker runtime observation failed"
    );
    let client: Value = serde_json::from_slice(&version.stdout)?;
    ensure!(
        client["Os"] == "darwin",
        "Mac benchmark observations require the actual Darwin client"
    );
    Ok((info, version))
}

/// Collects the current Mac guest profile and optionally every original measurement.
///
/// # Errors
///
/// Refuses missing prepared images, changed source, interrupted transports, or incomplete observations.
pub fn observe_mac(root: &Path, output: &Path, metadata_only: bool) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "Mac profile observations require macOS"
    );
    let root = crate::canonical(root)?;
    ensure!(!output.exists(), "observation output already exists");
    let image = proof_image(&root)?;
    let image_id = crate::skill_proofs::verified_image_id(&image)?;
    crate::container::prepare_cargo(&root, &image_id)?;
    let (host_info, host_version) = observed_host()?;
    let exchange = tempfile::Builder::new()
        .prefix("benchmark-observation-")
        .tempdir()?;
    let directory = crate::canonical(exchange.path())?;
    let result = directory.join("output");
    fs::create_dir(&result)?;
    let request = ObservationRequest {
        execution: TransportRequest {
            schema: TRANSPORT_SCHEMA,
            execution: directory
                .file_name()
                .and_then(|name| name.to_str())
                .context("observation execution name")?
                .into(),
            image_id: image_id.clone(),
            target: crate::container::LINUX_AMD64.into(),
            source_hash: source_hash(&root)?,
            invocation_hash: invocation_hash()?,
        },
        metadata_only,
        host_info,
        host_version,
        configuration: observation_configuration()?,
    };
    let request_file = directory.join(REQUEST_FILE);
    create_result(
        &request_file,
        &serde_json::to_vec(&request)?,
        MAX_REQUEST_BYTES,
    )?;
    let command = container_command(&root, &image_id, &request_file, &result);
    let arguments: Vec<_> = command.get_args().map(std::ffi::OsStr::to_owned).collect();
    ensure!(
        arguments
            .last()
            .is_some_and(|value| value == CONTAINER_BENCH_GATE),
        "benchmark transport argv differs"
    );
    let mut command = Tool::Docker.command();
    command
        .arg("run")
        .arg("--network=none")
        .args(
            arguments
                .iter()
                .skip(1)
                .take(arguments.len().saturating_sub(COMMAND_ENDPOINT_ARGUMENTS)),
        )
        .arg(OBSERVATION_GATE)
        .stdin(Stdio::null());
    let terminal =
        crate::phase_evidence::output(&mut command, "Mac benchmark observation container");
    let checked = export_observation(&root, &request, &result, output, terminal);
    if checked.is_ok() {
        platform::cleanup::report(exchange.close(), "remove benchmark observation scratch");
    } else {
        let preserved = exchange.keep();
        crate::phase_evidence::retain(
            "incomplete benchmark observation",
            "location",
            preserved.as_os_str().as_encoded_bytes(),
        )?;
    }
    checked
}

fn export_observation(
    root: &Path,
    request: &ObservationRequest,
    result: &Path,
    output: &Path,
    terminal: Result<std::process::Output>,
) -> Result<()> {
    retain_result(result)?;
    let original = read_bounded(&result.join(OBSERVATION_FILE), MAX_RESULT_FILE_BYTES)?;
    create_result(output, &original, MAX_RESULT_FILE_BYTES)?;
    ensure!(
        terminal?.status.code() == Some(0),
        "Mac observation failed or was interrupted; original report remains preserved"
    );
    ensure!(
        source_hash(root)? == request.execution.source_hash,
        "source changed during Mac observation"
    );
    let observation: Observation = serde_json::from_slice(&original)?;
    validate_observation(&observation, request)?;
    if !request.metadata_only {
        let identity = crate::benchmark_profile::identity(
            &observation.metadata,
            &request.execution.image_id,
            &request.host_info,
            &request.host_version,
            &request.configuration,
        )?;
        ensure!(
            crate::benchmark_profile::disposition(
                rules::benchmark_rules::ProfileMode::Observe,
                &identity,
                &identity,
                true,
                true,
                true
            ) == rules::benchmark_rules::ProfileVerdict::Observation,
            "observation cannot supply verification credit"
        );
        let bytes = read_bounded(&result.join(BINARY_FILE), MAX_RESULT_FILE_BYTES)?;
        ensure!(
            observation.binary_size == Some(u64::try_from(bytes.len())?)
                && observation.binary_hash.as_deref()
                    == Some(
                        crate::phase_evidence::verifier_digest(&result.join(BINARY_FILE))?.as_str()
                    ),
            "original observed binary bytes differ"
        );
    }
    println!(
        "Observed the Mac Linux/AMD64 guest; this output grants no benchmark verification credit"
    );
    Ok(())
}

fn observation_configuration() -> Result<String> {
    Ok(serde_json::to_string(&(
        OBSERVATION_GATE,
        invocation_hash()?,
        "--network=none",
        "--tool=none",
        "--quiet",
        "--bench",
        "--regression-fail-fast",
        crate::benchmark_profile::PROFILE_ENV,
        "fixed-glibc-2.36-cpuid-9-slots",
    ))?)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TransportFile {
    path: String,
    bytes: Vec<u8>,
}

fn digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes))
}

fn source_hash(root: &Path) -> Result<String> {
    let mut inputs = SOURCE_INPUTS
        .into_iter()
        .map(|relative| {
            Ok((
                relative,
                crate::phase_evidence::verifier_digest(&root.join(relative))?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    inputs.push((
        "runtime build stamp",
        platform::build_inputs::content_hash(root)?,
    ));
    Ok(digest(&serde_json::to_vec(&inputs)?))
}

fn invocation_hash() -> Result<String> {
    Ok(digest(&serde_json::to_vec(&(
        CONTAINER_BENCH_GATE,
        crate::container::LINUX_AMD64,
        SECCOMP,
        PROFILE,
        RUN_FLAGS,
        GUNGRAUN_VERSION,
        VALGRIND_VERSION,
        INSTRUCTION_PROBE,
        PROBE_CEILING,
        REGRESSION_STATUS,
        dotfiles::bounded::resource_arguments(),
        CONTAINER_BUILD_JOBS,
    ))?))
}

fn validate_request(request: &TransportRequest) -> Result<()> {
    let hash = |value: &str| {
        value.len() == Sha256::output_size() * HEX_DIGITS_PER_BYTE
            && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    };
    ensure!(
        request.schema == TRANSPORT_SCHEMA
            && !request.execution.is_empty()
            && request.execution.len() <= MAX_EXECUTION_NAME_BYTES
            && request
                .execution
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && request.target == crate::container::LINUX_AMD64
            && request.image_id.strip_prefix("sha256:").is_some_and(hash)
            && hash(&request.source_hash)
            && request.invocation_hash == invocation_hash()?,
        "benchmark transport request has an unsupported context"
    );
    Ok(())
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "benchmark transport requires a regular result file"
    );
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "benchmark transport result exceeds its byte bound"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= limit,
        "benchmark transport result grew beyond its byte bound"
    );
    Ok(bytes)
}

fn create_result(path: &Path, bytes: &[u8], limit: u64) -> Result<()> {
    ensure!(
        u64::try_from(bytes.len())? <= limit,
        "benchmark transport result exceeds its byte bound"
    );
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn validate_sources(root: &Path, source: &Path, request: &TransportRequest) -> Result<()> {
    ensure!(
        source_hash(root)? == request.source_hash && source_hash(source)? == request.source_hash,
        "benchmark source or copied context differs from this execution's request"
    );
    Ok(())
}

/// Runs the original Linux budget gate and exports its raw reports and measured binary.
///
/// # Errors
///
/// Refuses a changed request or source, failed gate, oversize result, or existing output.
pub fn write_container_report(root: &Path) -> Result<()> {
    ensure!(cfg!(target_os = "linux"), "benchmark reports require Linux");
    let request: TransportRequest = serde_json::from_slice(&read_bounded(
        Path::new(CONTAINER_REQUEST),
        MAX_REQUEST_BYTES,
    )?)?;
    validate_request(&request)?;
    let source = Path::new(CONTAINER_SOURCE);
    validate_sources(root, source, &request)?;
    let checked = crate::phase_evidence::run(
        root,
        "fixed Linux benchmark payload",
        FUNCTIONS.len(),
        || {
            let reports = collect(root)?;
            validate(root, &reports)?;
            validate_sources(root, source, &request)?;
            Ok(reports)
        },
    );
    let diagnostics = serde_json::to_vec(&result_files(
        &Path::new(CONTAINER_EVIDENCE).join(EVIDENCE_DIRECTORY),
    )?)?;
    let result = Path::new(CONTAINER_RESULT);
    create_result(
        &result.join(DIAGNOSTICS_FILE),
        &diagnostics,
        MAX_RESULT_FILE_BYTES,
    )?;
    let reports = checked?;
    let measured_binary = read_bounded(&binary(root), MAX_RESULT_FILE_BYTES)?;
    create_result(
        &result.join(BINARY_FILE),
        &measured_binary,
        MAX_RESULT_FILE_BYTES,
    )?;
    validate_sources(root, source, &request)?;
    let report = TransportReport {
        request,
        reports_hash: digest(&serde_json::to_vec(&reports)?),
        diagnostics_hash: digest(&diagnostics),
        reports,
    };
    create_result(
        &result.join(REPORT_FILE),
        &serde_json::to_vec(&report)?,
        MAX_RESULT_FILE_BYTES,
    )
}

fn decode_transport(
    baselines: &Baselines,
    expected: &TransportRequest,
    outer_status: Option<i32>,
    report: &[u8],
    diagnostics: &[u8],
    measured_binary: &Path,
) -> Result<Reports> {
    ensure!(
        outer_status == Some(0),
        "the containerized budget gate failed or was interrupted"
    );
    validate_request(expected)?;
    ensure!(
        u64::try_from(report.len())? <= MAX_RESULT_FILE_BYTES,
        "benchmark transport report exceeds its byte bound"
    );
    let report: TransportReport = serde_json::from_slice(report)?;
    ensure!(
        report.request == *expected,
        "benchmark transport report belongs to another execution or context"
    );
    ensure!(
        report.reports_hash == digest(&serde_json::to_vec(&report.reports)?),
        "benchmark transport raw reports changed"
    );
    ensure!(
        u64::try_from(diagnostics.len())? <= MAX_RESULT_FILE_BYTES
            && report.diagnostics_hash == digest(diagnostics),
        "benchmark transport original diagnostics are missing or changed"
    );
    let files: Vec<TransportFile> = serde_json::from_slice(diagnostics)?;
    let mut names = std::collections::BTreeSet::new();
    ensure!(
        !files.is_empty() && files.len() <= MAX_RESULT_ENTRIES,
        "benchmark transport original diagnostics have no bounded file inventory"
    );
    for file in &files {
        ensure!(
            file.path.split('/').all(|component| {
                !component.is_empty()
                    && component != "."
                    && component != ".."
                    && component.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
            }) && names.insert(&file.path),
            "benchmark transport original diagnostics have an invalid or duplicate path"
        );
    }
    let bytes = read_bounded(measured_binary, MAX_RESULT_FILE_BYTES)?;
    validate_measurements(
        baselines,
        u64::try_from(bytes.len())?,
        &crate::phase_evidence::verifier_digest(measured_binary)?,
        &report.reports,
    )?;
    Ok(report.reports)
}

fn result_files(result: &Path) -> Result<Vec<TransportFile>> {
    let mut pending = vec![result.to_owned()];
    let mut entries = 0_usize;
    let mut bytes = 0_u64;
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        entries += 1;
        ensure!(
            entries <= MAX_RESULT_ENTRIES,
            "benchmark transport result exceeds its entry bound"
        );
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "benchmark transport result cannot include a symlink"
        );
        if metadata.is_dir() {
            let mut children = Vec::new();
            for child in fs::read_dir(&path)? {
                ensure!(
                    entries + pending.len() + children.len() < MAX_RESULT_ENTRIES,
                    "benchmark transport result exceeds its entry bound"
                );
                children.push(child?.path());
            }
            children.sort();
            pending.extend(children.into_iter().rev());
        } else {
            let content = read_bounded(&path, MAX_RESULT_FILE_BYTES)?;
            bytes = bytes
                .checked_add(u64::try_from(content.len())?)
                .context("benchmark transport result byte count overflow")?;
            ensure!(
                bytes <= MAX_RESULT_BYTES,
                "benchmark transport result exceeds its total byte bound"
            );
            files.push(TransportFile {
                path: path
                    .strip_prefix(result)?
                    .to_str()
                    .context("benchmark transport diagnostic name")?
                    .replace('\\', "/"),
                bytes: content,
            });
        }
    }
    Ok(files)
}

fn retain_result(result: &Path) -> Result<()> {
    for file in result_files(result)? {
        crate::phase_evidence::retain(
            &format!("benchmark container {}", file.path),
            "transport",
            &file.bytes,
        )?;
    }
    Ok(())
}

fn bind_mount(source: &Path, target: &str, readonly: bool) -> std::ffi::OsString {
    let mut mount = std::ffi::OsString::from("type=bind,\"source=");
    mount.push(source);
    mount.push(format!("\",target={target}"));
    if readonly {
        mount.push(",readonly");
    }
    mount
}

/// Copies the checkout and executes the original budget gate with owned report output.
const CONTAINER_BENCH_GATE: &str = "set -eo pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; exec cargo run --locked --manifest-path xtask/Cargo.toml -- benchmarks-report";

fn container_command(root: &Path, image_id: &str, request: &Path, result: &Path) -> Command {
    let mut command = Tool::Docker.command();
    command
        .args([
            "run",
            "--rm",
            "--pull=never",
            "--platform",
            crate::container::LINUX_AMD64,
            SECCOMP,
            "--mount",
        ])
        .arg(bind_mount(root, CONTAINER_SOURCE, true))
        .args(dotfiles::bounded::resource_arguments())
        .arg("--mount")
        .arg(bind_mount(request, CONTAINER_REQUEST, true))
        .arg("--mount")
        .arg(bind_mount(result, CONTAINER_RESULT, false))
        .args([
            "--volume",
            &format!("{}:/cache", container_target_volume(root)),
            "--volume",
            crate::container::CARGO_REGISTRY_VOLUME,
            "--env",
            "CARGO_TARGET_DIR=/cache/target",
            "--env",
            CONTAINER_BUILD_JOBS,
            "--env",
            &format!("{RESULT_CACHE}={CONTAINER_EVIDENCE}"),
            "--workdir",
            "/work",
            image_id,
            "bash",
            "-ec",
            CONTAINER_BENCH_GATE,
        ])
        .stdin(Stdio::null());
    command
}

/// Runs the budget gate in the pinned Linux environment that holds its recorded ceilings.
///
/// # Errors
///
/// Refuses an unavailable image, failed Docker execution, or a failed gate.
pub fn verify_in_container(root: &Path) -> Result<()> {
    let root = crate::canonical(root)?;
    let image = proof_image(&root)?;
    crate::skill_proofs::require_image(&image)?;
    let image_id = crate::skill_proofs::verified_image_id(&image)?;
    crate::container::prepare_cargo(&root, &image_id)?;
    let exchange = tempfile::Builder::new().prefix("benchmarks-").tempdir()?;
    let directory = crate::canonical(exchange.path())?;
    let result = directory.join("output");
    fs::create_dir(&result)?;
    let request = TransportRequest {
        schema: TRANSPORT_SCHEMA,
        execution: directory
            .file_name()
            .and_then(|name| name.to_str())
            .context("benchmark execution name")?
            .to_owned(),
        image_id: image_id.clone(),
        target: crate::container::LINUX_AMD64.into(),
        source_hash: source_hash(&root)?,
        invocation_hash: invocation_hash()?,
    };
    validate_request(&request)?;
    let request_file = directory.join(REQUEST_FILE);
    let request_bytes = serde_json::to_vec(&request)?;
    create_result(&request_file, &request_bytes, MAX_REQUEST_BYTES)?;
    crate::phase_evidence::retain("benchmark container request", "transport", &request_bytes)?;
    let output = crate::phase_evidence::output(
        &mut container_command(&root, &image_id, &request_file, &result),
        "fixed Linux benchmark container",
    );
    let checked = (|| {
        retain_result(&result)?;
        let output = output?;
        ensure!(
            output.status.code() == Some(0),
            "the containerized budget gate failed or was interrupted"
        );
        ensure!(
            source_hash(&root)? == request.source_hash,
            "benchmark source changed during container execution"
        );
        let baselines = parse_baselines(&fs::read_to_string(root.join(obligations::BASELINES))?)?;
        let reports = decode_transport(
            &baselines,
            &request,
            output.status.code(),
            &read_bounded(&result.join(REPORT_FILE), MAX_RESULT_FILE_BYTES)?,
            &read_bounded(&result.join(DIAGNOSTICS_FILE), MAX_RESULT_FILE_BYTES)?,
            &result.join(BINARY_FILE),
        )?;
        println!(
            "Held every gungraun benchmark and the {}-byte release-guard binary to the recorded ceilings",
            reports.binary_size
        );
        Ok(())
    })();
    if checked.is_ok() {
        platform::cleanup::report(exchange.close(), "remove benchmark transport scratch");
    } else {
        let preserved = exchange.keep();
        crate::phase_evidence::retain(
            "incomplete benchmark container transport",
            "location",
            preserved.as_os_str().as_encoded_bytes(),
        )?;
    }
    checked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(status: Option<i32>, stdout: &str) -> crate::phase_evidence::RawResult {
        crate::phase_evidence::RawResult {
            status,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn observation_controls_require_original_instructions_and_the_specific_terminal_status()
    -> Result<()> {
        let original = serde_json::json!({"function_name":"scan","profiles":[{"tool":"Callgrind","data":{"total":{"metrics":{"Ir":{"values":{"new":PROBE_CEILING + 1}}}}}}]}).to_string();
        validate_observed_probe(&raw(Some(REGRESSION_STATUS), &original))?;
        for status in [None, Some(0), Some(1)] {
            assert!(validate_observed_probe(&raw(status, &original)).is_err());
        }
        for stdout in ["", "{}", "null", "failed due to a network error"] {
            assert!(validate_observed_probe(&raw(Some(REGRESSION_STATUS), stdout)).is_err());
        }
        let non_regression = original.replace(
            &format!("\"new\":{}", PROBE_CEILING + 1),
            &format!("\"new\":{PROBE_CEILING}"),
        );
        assert!(validate_observed_probe(&raw(Some(REGRESSION_STATUS), &non_regression)).is_err());
        Ok(())
    }

    fn reports() -> Result<(Baselines, Reports)> {
        let mut metrics = vec![Metric::BinarySize { exact: 1000 }];
        let mut summaries = Vec::new();
        for name in FUNCTIONS {
            metrics.extend([
                Metric::Ir {
                    bench: name.into(),
                    exact: 100,
                },
                Metric::HeapPeak {
                    bench: name.into(),
                    exact: 50,
                },
            ]);
            summaries.push(serde_json::json!({"function_name":name,"profiles":[{"tool":"Callgrind","data":{"total":{"metrics":{"Ir":{"values":{"new":100}}}}}},{"tool":"DHAT","data":{"total":{"metrics":{"AtTGmaxBytes":{"values":{"new":50}}}}}}]}).to_string());
        }
        let mut baselines = parse_baselines("")?;
        baselines.metrics = metrics;
        Ok((
            baselines,
            Reports {
                runner: raw(Some(0), GUNGRAUN_VERSION),
                valgrind: raw(Some(0), VALGRIND_VERSION),
                build: raw(Some(0), ""),
                binary_hash: "binary-digest".into(),
                binary_size: 1000,
                measurements: raw(Some(0), &summaries.join("\n")),
                probe_build: raw(Some(0), ""),
                probe: raw(Some(REGRESSION_STATUS), ""),
            },
        ))
    }

    fn transport() -> Result<(tempfile::TempDir, Baselines, TransportReport)> {
        let result = tempfile::tempdir()?;
        let binary = result.path().join(BINARY_FILE);
        create_result(&binary, &[0; 1000], MAX_RESULT_FILE_BYTES)?;
        let diagnostics = serde_json::to_vec(&[TransportFile {
            path: "execution-fresh/original.stdout".into(),
            bytes: b"original".to_vec(),
        }])?;
        create_result(
            &result.path().join(DIAGNOSTICS_FILE),
            &diagnostics,
            MAX_RESULT_FILE_BYTES,
        )?;
        let (baselines, mut reports) = reports()?;
        reports.binary_hash = crate::phase_evidence::verifier_digest(&binary)?;
        Ok((
            result,
            baselines,
            TransportReport {
                request: TransportRequest {
                    schema: TRANSPORT_SCHEMA,
                    execution: "benchmarks-fresh".into(),
                    image_id: format!("sha256:{}", digest(b"fixed-image")),
                    target: crate::container::LINUX_AMD64.into(),
                    source_hash: digest(b"current-source"),
                    invocation_hash: invocation_hash()?,
                },
                reports_hash: digest(&serde_json::to_vec(&reports)?),
                diagnostics_hash: digest(&diagnostics),
                reports,
            },
        ))
    }

    #[test]
    fn benchmark_transport_requires_original_raw_results_and_the_exported_binary() -> Result<()> {
        let (result, baselines, valid) = transport()?;
        let binary = result.path().join(BINARY_FILE);
        let diagnostics =
            read_bounded(&result.path().join(DIAGNOSTICS_FILE), MAX_RESULT_FILE_BYTES)?;
        let decode = |report: &TransportReport| {
            decode_transport(
                &baselines,
                &valid.request,
                Some(0),
                &serde_json::to_vec(report)?,
                &diagnostics,
                &binary,
            )
        };
        decode(&valid)?;
        let mut changed = valid.clone();
        changed.reports.probe.status = Some(0);
        assert!(decode(&changed).is_err());
        changed.reports_hash = digest(&serde_json::to_vec(&changed.reports)?);
        assert!(decode(&changed).is_err());
        let mut missing = valid.clone();
        missing.reports.measurements.stdout = b"{}".to_vec();
        missing.reports_hash = digest(&serde_json::to_vec(&missing.reports)?);
        assert!(decode(&missing).is_err());
        fs::write(&binary, [1; 1000])?;
        assert!(decode(&valid).is_err());
        fs::write(&binary, [0; 999])?;
        assert!(decode(&valid).is_err());
        fs::remove_file(&binary)?;
        assert!(decode(&valid).is_err());
        Ok(())
    }

    #[test]
    fn benchmark_transport_refuses_exit_only_missing_malformed_and_interrupted_results()
    -> Result<()> {
        let (result, baselines, valid) = transport()?;
        let binary = result.path().join(BINARY_FILE);
        let diagnostics =
            read_bounded(&result.path().join(DIAGNOSTICS_FILE), MAX_RESULT_FILE_BYTES)?;
        let bytes = serde_json::to_vec(&valid)?;
        for status in [Some(1), Some(REGRESSION_STATUS), None] {
            assert!(
                decode_transport(
                    &baselines,
                    &valid.request,
                    status,
                    &bytes,
                    &diagnostics,
                    &binary
                )
                .is_err()
            );
        }
        for bytes in [b"".as_slice(), b"{}", b"null", b"{\"request\":"] {
            assert!(
                decode_transport(
                    &baselines,
                    &valid.request,
                    Some(0),
                    bytes,
                    &diagnostics,
                    &binary
                )
                .is_err()
            );
        }
        let mut unknown = serde_json::to_value(&valid)?;
        unknown["unreviewed"] = serde_json::json!(true);
        assert!(
            decode_transport(
                &baselines,
                &valid.request,
                Some(0),
                &serde_json::to_vec(&unknown)?,
                &diagnostics,
                &binary,
            )
            .is_err()
        );
        let mut unknown = serde_json::to_value(&valid)?;
        unknown["reports"]["measurements"]["unreviewed"] = serde_json::json!(true);
        assert!(
            decode_transport(
                &baselines,
                &valid.request,
                Some(0),
                &serde_json::to_vec(&unknown)?,
                &diagnostics,
                &binary,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn benchmark_transport_binds_fresh_execution_image_target_source_and_invocation() -> Result<()>
    {
        let (result, baselines, valid) = transport()?;
        let binary = result.path().join(BINARY_FILE);
        let diagnostics =
            read_bounded(&result.path().join(DIAGNOSTICS_FILE), MAX_RESULT_FILE_BYTES)?;
        let mut requests = Vec::new();
        let mut request = valid.request.clone();
        request.schema += 1;
        requests.push(request);
        let mut request = valid.request.clone();
        request.execution = "benchmarks-old".into();
        requests.push(request);
        let mut request = valid.request.clone();
        request.image_id = format!("sha256:{}", digest(b"other-image"));
        requests.push(request);
        let mut request = valid.request.clone();
        request.target = "linux/arm64".into();
        requests.push(request);
        let mut request = valid.request.clone();
        request.source_hash = digest(b"old-source");
        requests.push(request);
        let mut request = valid.request.clone();
        request.invocation_hash = digest(b"different-gate");
        requests.push(request);
        for request in requests {
            let mut stale = valid.clone();
            stale.request = request;
            assert!(
                decode_transport(
                    &baselines,
                    &valid.request,
                    Some(0),
                    &serde_json::to_vec(&stale)?,
                    &diagnostics,
                    &binary,
                )
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn benchmark_transport_requires_lossless_strict_original_diagnostics() -> Result<()> {
        let (result, baselines, valid) = transport()?;
        let binary = result.path().join(BINARY_FILE);
        assert!(
            decode_transport(
                &baselines,
                &valid.request,
                Some(0),
                &serde_json::to_vec(&valid)?,
                b"changed",
                &binary,
            )
            .is_err()
        );
        for diagnostics in [
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!([{ "path": "../foreign", "bytes": [0] }]),
            serde_json::json!([{ "path": "original", "bytes": [0], "unknown": true }]),
            serde_json::json!([
                { "path": "original", "bytes": [0] },
                { "path": "original", "bytes": [1] }
            ]),
        ] {
            let diagnostics = serde_json::to_vec(&diagnostics)?;
            let mut changed = valid.clone();
            changed.diagnostics_hash = digest(&diagnostics);
            assert!(
                decode_transport(
                    &baselines,
                    &valid.request,
                    Some(0),
                    &serde_json::to_vec(&changed)?,
                    &diagnostics,
                    &binary,
                )
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn benchmark_transport_has_bounded_create_new_regular_outputs() -> Result<()> {
        let result = tempfile::tempdir()?;
        let file = result.path().join(REPORT_FILE);
        create_result(&file, b"original", MAX_RESULT_FILE_BYTES)?;
        assert!(create_result(&file, b"replacement", MAX_RESULT_FILE_BYTES).is_err());
        assert_eq!(read_bounded(&file, 8)?, b"original");
        assert!(read_bounded(&file, 7).is_err());
        assert!(read_bounded(result.path(), MAX_RESULT_FILE_BYTES).is_err());
        let absent = result.path().join("absent");
        assert!(read_bounded(&absent, MAX_RESULT_FILE_BYTES).is_err());
        let request = result.path().join(REQUEST_FILE);
        let too_long = vec![b'x'; usize::try_from(MAX_REQUEST_BYTES)? + 1];
        assert!(create_result(&request, &too_long, MAX_REQUEST_BYTES).is_err());
        assert!(!request.exists());
        for index in 0..MAX_RESULT_ENTRIES {
            fs::write(result.path().join(format!("entry-{index}")), b"")?;
        }
        assert!(result_files(result.path()).is_err());
        Ok(())
    }

    #[test]
    fn benchmark_container_enforces_the_declared_guest_budget() -> Result<()> {
        let command = container_command(
            Path::new("owned-source"),
            "actual-image-id",
            Path::new("owned-request"),
            Path::new("owned-result"),
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_str().context("benchmark argument UTF-8"))
            .collect::<Result<Vec<_>>>()?;
        for expected in [
            ["--cpus", "2"],
            ["--memory", "2147483648"],
            ["--memory-swap", "2147483648"],
            ["--pids-limit", "256"],
            ["--env", "CARGO_BUILD_JOBS=2"],
            ["--platform", "linux/amd64"],
        ] {
            assert_eq!(
                arguments
                    .windows(2)
                    .filter(|pair| *pair == expected)
                    .count(),
                1
            );
        }
        for option in ["--cpus", "--memory", "--pids-limit"] {
            assert_eq!(
                arguments
                    .iter()
                    .filter(|argument| **argument == option)
                    .count(),
                1
            );
        }
        assert_eq!(arguments.last(), Some(&CONTAINER_BENCH_GATE));
        Ok(())
    }

    #[test]
    fn benchmark_transport_checks_both_original_and_copied_source_bytes() -> Result<()> {
        let source = tempfile::tempdir()?;
        let copied = tempfile::tempdir()?;
        for root in [source.path(), copied.path()] {
            for relative in SOURCE_INPUTS {
                let path = root.join(relative);
                if path.extension().is_some() {
                    fs::write(path, b"source")?;
                } else {
                    fs::create_dir(&path)?;
                    fs::write(path.join("input"), b"source")?;
                }
            }
            for krate in platform::build_inputs::CRATES {
                let directory = root.join(krate).join("src");
                fs::create_dir_all(&directory)?;
                fs::write(directory.join("input.rs"), b"source")?;
            }
        }
        let (_, _, mut valid) = transport()?;
        valid.request.source_hash = source_hash(source.path())?;
        validate_sources(copied.path(), source.path(), &valid.request)?;
        fs::write(copied.path().join("benchmarks/input"), b"changed")?;
        assert!(validate_sources(copied.path(), source.path(), &valid.request).is_err());
        fs::write(copied.path().join("benchmarks/input"), b"source")?;
        fs::write(source.path().join("policy/input"), b"changed")?;
        assert!(validate_sources(copied.path(), source.path(), &valid.request).is_err());
        fs::write(source.path().join("policy/input"), b"source")?;
        validate_sources(copied.path(), source.path(), &valid.request)?;
        fs::write(
            copied.path().join("lefthook.yml"),
            b"build-stamp-only change",
        )?;
        assert!(validate_sources(copied.path(), source.path(), &valid.request).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn benchmark_transport_refuses_linked_payloads_and_result_directories() -> Result<()> {
        let result = tempfile::tempdir()?;
        let other = tempfile::tempdir()?;
        let file = other.path().join(BINARY_FILE);
        create_result(&file, b"foreign", MAX_RESULT_FILE_BYTES)?;
        let linked = result.path().join(BINARY_FILE);
        std::os::unix::fs::symlink(&file, &linked)?;
        assert!(read_bounded(&linked, MAX_RESULT_FILE_BYTES).is_err());
        assert!(retain_result(result.path()).is_err());
        fs::remove_file(&linked)?;
        std::os::unix::fs::symlink(other.path(), &linked)?;
        assert!(retain_result(result.path()).is_err());
        Ok(())
    }

    #[test]
    fn raw_benchmark_reports_bind_all_measurements_binary_and_actual_rejecting_probe() -> Result<()>
    {
        let (baselines, valid) = reports()?;
        validate_measurements(&baselines, 1000, "binary-digest", &valid)?;
        assert!(validate_measurements(&baselines, 1001, "binary-digest", &valid).is_err());
        assert!(validate_measurements(&baselines, 1000, "changed-binary", &valid).is_err());
        let mut incomplete = valid.clone();
        incomplete.measurements.stdout = b"{}".to_vec();
        assert!(validate_measurements(&baselines, 1000, "binary-digest", &incomplete).is_err());
        let mut wrong_probe = valid.clone();
        wrong_probe.probe.status = Some(0);
        assert!(validate_measurements(&baselines, 1000, "binary-digest", &wrong_probe).is_err());
        let mut interrupted = valid.clone();
        interrupted.probe.status = None;
        assert!(validate_measurements(&baselines, 1000, "binary-digest", &interrupted).is_err());
        let mut wrong_tool = valid;
        wrong_tool.valgrind.stdout = b"valgrind-unpinned".to_vec();
        assert!(validate_measurements(&baselines, 1000, "binary-digest", &wrong_tool).is_err());
        Ok(())
    }
}
