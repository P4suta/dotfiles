//! The ceilings of `policy/baselines.toml` that the gungraun benches load, so the runner itself refuses the moment a benchmark crosses its budget with exit status 3.

use gungraun::{Callgrind, Dhat, DhatMetric, EventKind};
use xtask::obligations::{Metric, parse_baselines};

/// The recorded baselines, relative to this package's directory.
pub const BASELINES: &str = "../policy/baselines.toml";

/// The `exact` ceiling of one benchmark's `metric`, refusing a benchmark the baselines don't record.
pub fn ceiling(metric: &str, bench: &str) -> u64 {
    if let Some(path) = std::env::var_os(xtask::benchmark_profile::PROFILE_ENV) {
        let profile = xtask::benchmark_profile::read_recorded(std::path::Path::new(&path))
            .unwrap_or_else(|error| panic!("read benchmark profile: {error:#}"));
        return profile
            .measurements
            .iter()
            .find_map(|entry| {
                (entry.function == bench).then_some(match metric {
                    "ir" => entry.instructions,
                    "heap-peak" => entry.heap_peak,
                    _ => panic!("unknown benchmark metric {metric}"),
                })
            })
            .unwrap_or_else(|| panic!("profile records no {metric} ceiling for {bench}"));
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(BASELINES);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let baselines =
        parse_baselines(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    let recorded = baselines.metrics.iter().find_map(|entry| match entry {
        Metric::Ir {
            bench: named,
            exact,
        } if metric == "ir" && named == bench => Some(*exact),
        Metric::HeapPeak {
            bench: named,
            exact,
        } if metric == "heap-peak" && named == bench => Some(*exact),
        _ => None,
    });
    recorded.unwrap_or_else(|| {
        panic!(
            "{} records no {metric} ceiling for the {bench} benchmark; record one before it can run",
            path.display()
        )
    })
}

/// The instruction ceiling of one benchmark, which the runner enforces with exit status 3.
#[must_use]
pub fn callgrind(bench: &str) -> Callgrind {
    let mut tool = Callgrind::default();
    let ceiling = if std::env::var_os(xtask::benchmarks::INSTRUCTION_PROBE).is_some() {
        xtask::benchmarks::PROBE_CEILING
    } else {
        ceiling("ir", bench)
    };
    tool.hard_limits([(EventKind::Ir, ceiling)]);
    tool.clone()
}

/// The heap ceiling of one benchmark, whose peak the runner enforces with exit status 3.
#[must_use]
pub fn dhat(bench: &str) -> Dhat {
    let mut tool = Dhat::default();
    tool.hard_limits([(DhatMetric::AtTGmaxBytes, ceiling("heap-peak", bench))]);
    tool.clone()
}

/// The instruction and heap ceilings of one library benchmark.
#[must_use]
pub fn library(bench: &str) -> gungraun::LibraryBenchmarkConfig {
    let mut config = gungraun::LibraryBenchmarkConfig::default();
    config.tool(callgrind(bench));
    config.tool(dhat(bench));
    config.clone()
}

/// The instruction and heap ceilings of one binary benchmark, whose whole delegated call the runner measures in the pinned image.
#[must_use]
pub fn wrapper(bench: &str) -> gungraun::BinaryBenchmarkConfig {
    let mut config = gungraun::BinaryBenchmarkConfig::default();
    config.tool(callgrind(bench));
    config.tool(dhat(bench));
    config.clone()
}
