use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use dotfiles_xtask::skill_ops::{self as ops, Catalog, Note, Policy, Report, Review, Runtime};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Load, observe, analyze, and verify the owner's shared skills locally")]
struct Cli {
    #[arg(long, env = "SKILL_OPS_TREE")]
    tree: Option<PathBuf>,
    #[arg(long, env = "SKILL_OPS_POLICY")]
    policy: Option<PathBuf>,
    #[arg(long, env = "SKILL_OPS_STATE")]
    state: Option<PathBuf>,
    #[arg(long, env = "SKILL_OPS_APPROVED")]
    approved: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    Load {
        names: Vec<String>,
        #[arg(long)]
        bundle: Option<String>,
    },
    Observe {
        #[arg(long)]
        client: String,
    },
    Analyze {
        #[arg(long)]
        out: PathBuf,
    },
    Triage {
        #[arg(long)]
        decisions: PathBuf,
        #[arg(long)]
        evidence_root: Option<PathBuf>,
    },
    Note {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        skill: Vec<String>,
        #[arg(long, required = true)]
        evidence: Vec<String>,
    },
    Check,
    Stop,
    Doctor,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeReview {
    evidence_root: PathBuf,
    evidence_hashes: BTreeMap<String, String>,
    review: Review,
}

fn optional<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if path.try_exists()? {
        Ok(Some(ops::read_json(path)?))
    } else {
        Ok(None)
    }
}

fn stdin_json() -> Result<serde_json::Value> {
    let mut text = String::new();
    std::io::stdin().take(8_388_609).read_to_string(&mut text)?;
    ensure!(text.len() <= 8_388_608, "hook input exceeds its size bound");
    serde_json::from_str(&text).context("invalid native hook input")
}

fn archived_report(state: &Path, identity: &str) -> Result<Report> {
    ensure!(
        dotfiles_xtask::skill_rules::immutable_identity_valid(
            identity.len(),
            identity.bytes().all(|byte| byte.is_ascii_hexdigit())
        ),
        "invalid immutable report identity"
    );
    let report = ops::read_json(&state.join("reports").join(format!("{identity}.json")))?;
    ensure!(
        ops::hash(&report)? == identity,
        "immutable report identity mismatch"
    );
    Ok(report)
}

fn pending(
    catalog: &Catalog,
    policy: &Policy,
    state: &Path,
    approved: &Path,
    root: &Path,
) -> Result<bool> {
    let draft: Option<Report> = optional(&state.join("report.json"))?;
    let reviewed: Option<RuntimeReview> = optional(&state.join("review.json"))?;
    let report = reviewed
        .as_ref()
        .map(|reviewed| archived_report(state, &reviewed.review.report_hash))
        .transpose()?;
    let events = ops::recorded(state)?;
    let notes = ops::notes(state)?;
    if let Some(reviewed) = &reviewed {
        ensure!(
            ops::evidence_hashes(&reviewed.review, &reviewed.evidence_root)?
                == reviewed.evidence_hashes,
            "review evidence changed; reassess the current evidence"
        );
    }
    let baseline: ops::Approved = ops::read_json(approved)?;
    let runtime = Runtime {
        catalog,
        policy,
        events: &events,
        notes: &notes,
        report: report.as_ref().or(draft.as_ref()),
        review: reviewed.as_ref().map(|item| &item.review),
        approved: &baseline,
        evidence_root: reviewed
            .as_ref()
            .map_or(root, |item| item.evidence_root.as_path()),
    };
    if draft.is_some() && reviewed.is_some() {
        // Preserve loss detection for observations captured by newer drafts.
        ops::runtime_pending(Runtime {
            report: draft.as_ref(),
            review: None,
            ..runtime
        })?;
    }
    ops::runtime_pending(runtime)
}

fn execute(cli: Cli) -> Result<()> {
    let user_directory = PathBuf::from(
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .context("user directory is unavailable")?,
    );
    let tree = cli
        .tree
        .clone()
        .unwrap_or_else(|| user_directory.join(".agents/skills"));
    let policy_path = cli
        .policy
        .unwrap_or_else(|| user_directory.join(".config/skill-ops/policy.json"));
    let state = cli
        .state
        .unwrap_or_else(|| user_directory.join(".local/state/skill-ops"));
    let approved = cli
        .approved
        .unwrap_or_else(|| user_directory.join(".config/skill-ops/approved.json"));
    let catalog = if cli.tree.is_none() {
        let baseline: ops::Approved = ops::read_json(&approved)?;
        ensure!(
            ops::hash(&baseline.catalog)? == baseline.catalog_hash,
            "managed catalog manifest is inconsistent"
        );
        ops::catalog_owned(&tree, &baseline.catalog)?
    } else {
        ops::catalog(&tree)?
    };
    let policy: Policy = ops::read_json(&policy_path)?;
    ops::validate_policy(&policy, &catalog)?;
    match cli.command {
        Action::Load { mut names, bundle } => {
            if let Some(bundle) = bundle {
                names.extend(
                    policy
                        .bundles
                        .get(&bundle)
                        .context("unknown skill bundle")?
                        .iter()
                        .cloned(),
                );
            }
            ensure!(!names.is_empty(), "select a skill or bundle");
            let available: BTreeSet<_> = catalog.skills.keys().cloned().collect();
            let plan = ops::dependency_plan(&available, &names, &policy.requirements)?;
            println!("SKILL_OPS_LOAD {}", serde_json::to_string(&plan)?);
            for name in plan {
                println!(
                    "\n{}",
                    fs::read_to_string(tree.join(name).join("SKILL.md"))?
                );
            }
        }
        Action::Observe { client } => {
            for observation in ops::observations(&client, &stdin_json()?, &catalog, &tree)? {
                ops::record(&state, &observation)?;
            }
        }
        Action::Analyze { out } => {
            let events = ops::recorded(&state)?;
            let notes = ops::notes(&state)?;
            let report = ops::analyze(&catalog, &policy, &events, &notes)?;
            ops::write_json(&out, &report, false)?;
            ops::preserve_json(
                &state
                    .join("reports")
                    .join(format!("{}.json", ops::hash(&report)?)),
                &report,
            )?;
            if out != state.join("report.json") {
                ops::write_json(&state.join("report.json"), &report, true)?;
            }
            println!(
                "Analyzed {} skills, {} observations, {} sessions, and {} improvement notes; report {}",
                catalog.skills.len(),
                events.len(),
                report.sessions,
                notes.len(),
                ops::hash(&report)?
            );
            println!(
                "Record every disposition in a Review JSON with report_hash and decisions, then run skill-ops triage --decisions FILE."
            );
        }
        Action::Triage {
            decisions,
            evidence_root,
        } => {
            let review: Review = ops::read_json(&decisions)?;
            let report = archived_report(&state, &review.report_hash)?;
            let current = ops::analyze(
                &catalog,
                &policy,
                &ops::recorded(&state)?,
                &ops::notes(&state)?,
            )?;
            ensure!(
                report == current,
                "analysis changed before triage; analyze the current evidence"
            );
            let evidence_root = dotfiles_xtask::canonical(&evidence_root.unwrap_or(tree))?;
            ops::check_review(
                &ops::hash(&report)?,
                &report.findings,
                &review,
                &evidence_root,
            )?;
            let reviewed = RuntimeReview {
                evidence_hashes: ops::evidence_hashes(&review, &evidence_root)?,
                evidence_root,
                review,
            };
            ops::preserve_json(
                &state
                    .join("reviews")
                    .join(format!("{}.json", ops::hash(&reviewed)?)),
                &reviewed,
            )?;
            ops::write_json(&state.join("review.json"), &reviewed, true)?;
            println!("Verified every disposition against the current analysis and local evidence");
        }
        Action::Note {
            kind,
            summary,
            skill,
            evidence,
        } => {
            let note = Note {
                kind,
                skills: skill,
                summary,
                evidence,
            };
            ops::analyze(&catalog, &policy, &[], std::slice::from_ref(&note))?;
            let destination = state
                .join("notes")
                .join(format!("{}.json", ops::hash(&note)?));
            if destination.try_exists()? {
                ensure!(
                    ops::read_json::<Note>(&destination)? == note,
                    "conflicting improvement observation"
                );
            } else {
                ops::write_json(&destination, &note, false)?;
            }
            println!(
                "Recorded a local improvement observation; analysis and disposition are now required"
            );
        }
        Action::Check => ensure!(
            !pending(&catalog, &policy, &state, &approved, &tree)?,
            "skill maintenance is pending; analyze and triage the current evidence"
        ),
        Action::Stop => unreachable!("Stop is translated into a checked completion request"),
        Action::Doctor => {
            let events = ops::recorded(&state)?;
            println!(
                "Shared catalog: {} validated skills; {} local observations",
                catalog.skills.len(),
                events.len()
            );
            println!(
                "Required maintenance: {}",
                if pending(&catalog, &policy, &state, &approved, &tree)? {
                    "pending"
                } else {
                    "current"
                }
            );
            for (client, path) in [
                ("codex", user_directory.join(".codex/hooks.json")),
                ("claude", user_directory.join(".claude/settings.json")),
            ] {
                let data: serde_json::Value = ops::read_json(&path)?;
                ensure!(
                    dotfiles_xtask::skill_install::merge_hooks(data.clone(), client)? == data,
                    "{client} observer or completion hook registration has drifted"
                );
                println!(
                    "{client}: observer and completion definitions registered; runtime trust and activation require native client verification"
                );
            }
            ensure!(
                fs::read_to_string(user_directory.join(".config/opencode/plugins/skill-ops.ts"))?
                    == include_str!("../../../dot_config/opencode/plugins/skill-ops.ts"),
                "OpenCode observer definition has drifted"
            );
            println!(
                "OpenCode: typed observer definition registered; runtime activation requires native client verification"
            );
        }
    }
    Ok(())
}

fn main() {
    let mut cli = Cli::parse();
    let result = if matches!(cli.command, Action::Stop) {
        use dotfiles_xtask::skill_rules::{Completion, completion};
        let input = stdin_json();
        let continued = input
            .as_ref()
            .is_ok_and(|input| input["stop_hook_active"] == true);
        cli.command = Action::Check;
        let checked = input.and_then(|_| execute(cli));
        let reason = checked.as_ref().err().map(|error| format!("Skill maintenance remains incomplete: {error:#}. Run skill-ops analyze, assess the findings with skill-operations, implement accepted changes, and run skill-ops triage before claiming completion."));
        let response = match completion(checked.is_ok(), continued) {
            Completion::Allow => serde_json::json!({}),
            Completion::RequestMaintenance => {
                serde_json::json!({"decision":"block","reason":reason})
            }
            Completion::EndIncomplete => {
                serde_json::json!({"continue":false,"stopReason":reason,"systemMessage":"Skill maintenance remains incomplete; automatic continuation is bounded."})
            }
        };
        println!("{response}");
        Ok(())
    } else {
        execute(cli)
    };
    if let Err(error) = result {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
