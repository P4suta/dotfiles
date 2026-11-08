//! Native mutation inventory and phase-result validation shared by the runner and project gate.

use anyhow::{Context, Result, ensure};
use rules::proof_budget_rules;
use std::collections::BTreeSet;

#[derive(Debug, serde::Deserialize)]
pub struct MutationIdentity {
    pub name: String,
    pub package: String,
}

#[derive(Debug, serde::Deserialize)]
pub enum MutationScenario {
    Baseline,
    Mutant(MutationIdentity),
}

#[derive(Debug, serde::Deserialize)]
pub enum MutationStage {
    Build,
    Test,
}

#[derive(Debug, serde::Deserialize)]
pub enum MutationStatus {
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
pub struct MutationPhaseReceipt {
    pub phase: MutationStage,
    pub process_status: MutationStatus,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq)]
pub enum MutationSummary {
    Success,
    Failure,
    CaughtMutant,
    MissedMutant,
    Timeout,
    Unviable,
}

#[derive(Debug, serde::Deserialize)]
pub struct MutationReceipt {
    pub scenario: MutationScenario,
    pub summary: MutationSummary,
    pub phase_results: Vec<MutationPhaseReceipt>,
}

#[derive(Debug, serde::Deserialize)]
pub struct MutationReport {
    pub outcomes: Vec<MutationReceipt>,
    pub total_mutants: u64,
    pub missed: u64,
    pub caught: u64,
    pub timeout: u64,
    pub unviable: u64,
    pub success: u64,
    pub end_time: Option<String>,
    pub cargo_mutants_version: String,
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

/// # Errors
/// Refuses incomplete inventories, contradictory phase results, or another verifier version.
pub fn validate_mutation_report(
    inventory: &[MutationIdentity],
    report: MutationReport,
    status: Option<i32>,
    package: &str,
    version: &str,
) -> Result<MutationReport> {
    use proof_budget_rules::{MutationOutcome, MutationPhase, mutation_exit, mutation_outcome};
    ensure!(
        report.cargo_mutants_version == version
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

pub(super) fn validate(
    path: &std::path::Path,
    status: i32,
    expected: &[String],
    version: &str,
) -> Result<()> {
    let bytes = std::fs::read(path.with_file_name("mutants.json"))?;
    let raw: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    let inventory: Vec<MutationIdentity> = serde_json::from_slice(&bytes)?;
    let report: MutationReport = serde_json::from_slice(&std::fs::read(path)?)?;
    let files: BTreeSet<_> = raw
        .iter()
        .map(|item| {
            item.get("file")
                .and_then(serde_json::Value::as_str)
                .context("mutation inventory omits its source file")
        })
        .collect::<Result<_>>()?;
    ensure!(
        !inventory.is_empty()
            && expected
                .iter()
                .all(|prefix| files.iter().any(|file| super::source::within(file, prefix)))
            && files.iter().all(|file| expected
                .iter()
                .any(|prefix| super::source::within(file, prefix))),
        "mutation selection differs from its declared source scope"
    );
    let package = inventory
        .first()
        .context("empty mutation inventory")?
        .package
        .clone();
    let report = validate_mutation_report(&inventory, report, Some(status), &package, version)?;
    ensure!(
        report.missed == 0 && report.timeout == 0 && report.caught > 0,
        "mutation selection has survivors, timeouts, or no executed mutants"
    );
    for prefix in expected {
        ensure!(report.outcomes.iter().any(|outcome| {
            outcome.summary == MutationSummary::CaughtMutant && matches!(&outcome.scenario, MutationScenario::Mutant(mutant) if mutant.name.split_once(':').is_some_and(|(file, _)| super::source::within(file, prefix)))
        }), "selected mutation source {prefix} has no caught mutant");
    }
    Ok(())
}
