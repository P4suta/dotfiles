//! The budget gate reads the gungraun summaries and holds every benchmark and the binary to the recorded ceilings.

use std::fs;
use std::path::Path;
use xtask::benchmarks::{FUNCTIONS, Measured, budgets, coverage, measured};
use xtask::obligations::{parse_baselines, parse_manifest};

#[path = "support/incident_fixture.rs"]
mod incident_fixture;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn native_checks_never_omit_their_required_campaign_or_fixed_budget() {
    use rules::hosts_rules::Family;
    for (family, extra) in [
        (Family::Linux, Some("benchmarks")),
        (Family::Mac, Some("proofs")),
        (Family::Windows, None),
    ] {
        for (native_failed, extra_failed) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let observed = std::cell::RefCell::new(Vec::new());
            let run = |stage| {
                observed.borrow_mut().push(stage);
                let failed = if stage == "check" {
                    native_failed
                } else {
                    extra_failed
                };
                anyhow::ensure!(!failed, "{stage} fixture failure");
                Ok(())
            };
            let result = xtask::skill_proofs::verify_push(
                family,
                || run("check"),
                || run("proofs"),
                || run("benchmarks"),
            );
            let mut required = vec!["check"];
            required.extend(extra);
            assert_eq!(*observed.borrow(), required);
            assert_eq!(
                result.is_err(),
                native_failed || (extra_failed && extra.is_some())
            );
            if let Err(failure) = result {
                let diagnostic = format!("{failure:#}");
                if native_failed {
                    assert!(diagnostic.contains("check fixture failure"));
                }
                if extra_failed && let Some(extra) = extra {
                    assert!(diagnostic.contains(&format!("{extra} fixture failure")));
                }
            }
        }
    }
}

/// One gungraun summary line with instruction and heap profiles.
fn summary(function: &str, instructions: u64, heap_peak: u64) -> String {
    serde_json::json!({
        "function_name": function,
        "profiles": [
            {"tool": "Callgrind", "data": {"total": {"metrics": {"Ir": {"values": {"new": instructions}}}}}},
            {"tool": "DHAT", "data": {"total": {"metrics": {"AtTGmaxBytes": {"values": {"new": heap_peak}}}}}}
        ]
    })
    .to_string()
}

#[test]
fn every_summary_line_yields_its_instructions_and_heap_peak() {
    let stdout = format!(
        "   Compiling dotfiles-benchmarks\n{}\n{}\n",
        summary("scan", 100, 5),
        summary("strip", 200, 6),
    );
    assert_eq!(
        measured(&stdout),
        [
            xtask::benchmarks::Measured {
                function: "scan".to_owned(),
                instructions: Some(100),
                heap_peak: Some(5),
            },
            xtask::benchmarks::Measured {
                function: "strip".to_owned(),
                instructions: Some(200),
                heap_peak: Some(6),
            }
        ]
    );
    assert_eq!(measured("not json\n{\"function_name\":\"\"}"), Vec::new());
}

#[test]
fn a_raised_ceiling_a_missing_measurement_and_an_unrecorded_improvement_all_fail()
-> anyhow::Result<()> {
    if incident_fixture::run(
        "a_raised_ceiling_a_missing_measurement_and_an_unrecorded_improvement_all_fail",
    )? {
        return Ok(());
    }
    let baselines = parse_baselines(
        "[[metric]]\nmetric = \"ir\"\nbench = \"scan\"\nexact = 100\n[[metric]]\nmetric = \"heap-peak\"\nbench = \"scan\"\nexact = 50\n[[metric]]\nmetric = \"binary-size\"\nexact = 1000\n",
    )
    .unwrap();
    let holds = vec![xtask::benchmarks::Measured {
        function: "scan".to_owned(),
        instructions: Some(100),
        heap_peak: Some(50),
    }];
    assert_eq!(budgets(&baselines, &holds, 1000), Vec::<String>::new());
    let raised = vec![xtask::benchmarks::Measured {
        function: "scan".to_owned(),
        instructions: Some(101),
        heap_peak: Some(50),
    }];
    assert_eq!(budgets(&baselines, &raised, 1000).len(), 1);
    let improved = vec![xtask::benchmarks::Measured {
        function: "scan".to_owned(),
        instructions: Some(100),
        heap_peak: Some(40),
    }];
    let refusals = budgets(&baselines, &improved, 999);
    assert_eq!(refusals.len(), 2, "{refusals:?}");
    assert!(
        refusals.iter().all(|refusal| refusal.contains("record")),
        "{refusals:?}"
    );
    platform::report(&anyhow::anyhow!(refusals.join("\n")));
    assert_eq!(budgets(&baselines, &holds, 1001).len(), 1);
    assert_eq!(
        budgets(&baselines, &holds, 999).len(),
        1,
        "a smaller binary needs its baseline recorded too"
    );
    let missing = budgets(
        &baselines,
        &[xtask::benchmarks::Measured {
            function: "strip".to_owned(),
            instructions: Some(1),
            heap_peak: None,
        }],
        1000,
    );
    assert_eq!(missing.len(), 3, "{missing:?}");
    Ok(())
}

#[test]
fn the_repository_records_a_ceiling_for_every_benchmark_and_its_binary() {
    let baselines =
        parse_baselines(&fs::read_to_string(root().join("policy/baselines.toml")).unwrap())
            .unwrap();
    let governed = |metric: &str| {
        baselines
            .metrics
            .iter()
            .filter(|entry| match entry {
                xtask::obligations::Metric::Ir { bench, .. } => metric == "ir" && !bench.is_empty(),
                xtask::obligations::Metric::HeapPeak { bench, .. } => {
                    metric == "heap-peak" && !bench.is_empty()
                }
                _ => false,
            })
            .count()
    };
    assert_eq!(governed("ir"), 7);
    assert_eq!(governed("heap-peak"), 7);
    assert!(
        baselines
            .metrics
            .iter()
            .any(|entry| matches!(entry, xtask::obligations::Metric::BinarySize { .. }))
    );
    let manifest =
        parse_manifest(&fs::read_to_string(root().join("policy/obligations.toml")).unwrap())
            .unwrap();
    assert!(
        root()
            .join(xtask::benchmarks::BENCHES)
            .join("Cargo.toml")
            .is_file()
            && !manifest.obligations.is_empty()
    );
}

#[test]
fn missing_duplicate_and_unknown_entries_fail_complete_benchmark_coverage() {
    let text = fs::read_to_string(root().join("policy/baselines.toml")).unwrap();
    let baselines = parse_baselines(&text).unwrap();
    let measured: Vec<_> = FUNCTIONS
        .iter()
        .map(|name| Measured {
            function: (*name).to_owned(),
            instructions: Some(1),
            heap_peak: Some(0),
        })
        .collect();
    assert_eq!(coverage(&baselines, &measured), Vec::<String>::new());
    let mut missing = measured.clone();
    missing.pop();
    assert_eq!(coverage(&baselines, &missing).len(), 1);
    let mut duplicate = measured.clone();
    duplicate.push(measured[0].clone());
    assert_eq!(coverage(&baselines, &duplicate).len(), 1);
    let mut unknown = measured.clone();
    unknown[0].function = "unknown".to_owned();
    assert_eq!(coverage(&baselines, &unknown).len(), 1);
    let mut missing_ceiling = parse_baselines(&text).unwrap();
    missing_ceiling.metrics.retain(
        |metric| !matches!(metric, xtask::obligations::Metric::Ir { bench, .. } if bench == "scan"),
    );
    assert_eq!(coverage(&missing_ceiling, &measured).len(), 1);
    let mut duplicate_ceiling = parse_baselines(&text).unwrap();
    duplicate_ceiling
        .metrics
        .push(xtask::obligations::Metric::BinarySize { exact: 1 });
    assert_eq!(coverage(&duplicate_ceiling, &measured).len(), 1);
}
