use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use skill_ops::freshness::Built;
use skill_ops::{self as ops, Catalog, Note, Policy, Report, Review, Runtime};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The largest native hook input the engine reads, 8 MiB.
const HOOK_INPUT_LIMIT: usize = 8 * 1024 * 1024;

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

fn reviewed(approved: &Path) -> Result<BTreeMap<String, ops::Reviewed>> {
    Ok(if approved.try_exists()? {
        ops::read_approved(approved)?.reviewed
    } else {
        BTreeMap::new()
    })
}

fn stdin_json() -> Result<serde_json::Value> {
    let mut text = String::new();
    std::io::stdin()
        .take(HOOK_INPUT_LIMIT as u64 + 1)
        .read_to_string(&mut text)?;
    ensure!(
        text.len() <= HOOK_INPUT_LIMIT,
        "hook input exceeds its size bound"
    );
    serde_json::from_str(&text).context("invalid native hook input")
}

fn archived_report(state: &Path, identity: &str) -> Result<Report> {
    ensure!(
        skill_ops::skill_rules::immutable_identity_valid(
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
    let baseline = ops::read_approved(approved)?;
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
    if draft.is_some()
        && !ops::runtime_pending(Runtime {
            report: draft.as_ref(),
            review: None,
            ..runtime
        })?
    {
        return Ok(false);
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
        let baseline = ops::read_approved(&approved)?;
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
    let session = Session {
        user_directory,
        tree,
        state,
        approved,
        catalog,
        policy,
    };
    match cli.command {
        Action::Load { names, bundle } => session.load(names, bundle)?,
        Action::Observe { client } => {
            for observation in
                ops::observations(&client, &stdin_json()?, &session.catalog, &session.tree)?
            {
                ops::record(&session.state, &observation)?;
            }
        }
        Action::Analyze { out } => session.analyze(&out)?,
        Action::Triage {
            decisions,
            evidence_root,
        } => session.triage(&decisions, evidence_root)?,
        Action::Note {
            kind,
            summary,
            skill,
            evidence,
        } => session.note(&Note {
            kind,
            skills: skill,
            summary,
            evidence,
        })?,
        Action::Check => ensure!(
            !session.pending()?,
            "skill maintenance is pending; run skill-ops analyze --out FILE and triage the findings it lists with skill-ops triage --decisions FILE"
        ),
        Action::Stop => unreachable!("Stop is translated into a checked completion request"),
        Action::Doctor => session.doctor()?,
    }
    Ok(())
}

/// The catalog, policy, and local state one skill-ops command works on.
struct Session {
    user_directory: PathBuf,
    tree: PathBuf,
    state: PathBuf,
    approved: PathBuf,
    catalog: ops::Catalog,
    policy: Policy,
}

impl Session {
    fn pending(&self) -> Result<bool> {
        pending(
            &self.catalog,
            &self.policy,
            &self.state,
            &self.approved,
            &self.tree,
        )
    }

    /// Prints the dependency plan of the selected skills and each skill's instructions in that order.
    fn load(&self, mut names: Vec<String>, bundle: Option<String>) -> Result<()> {
        if let Some(bundle) = bundle {
            names.extend(
                self.policy
                    .bundles
                    .get(&bundle)
                    .context("unknown skill bundle")?
                    .iter()
                    .cloned(),
            );
        }
        ensure!(!names.is_empty(), "select a skill or bundle");
        let available: BTreeSet<_> = self.catalog.skills.keys().cloned().collect();
        let plan = ops::dependency_plan(&available, &names, &self.policy.requirements)?;
        println!("SKILL_OPS_LOAD {}", serde_json::to_string(&plan)?);
        for name in plan {
            println!(
                "\n{}",
                fs::read_to_string(self.tree.join(name).join("SKILL.md"))?
            );
        }
        Ok(())
    }

    fn analyze(&self, out: &Path) -> Result<()> {
        let (catalog, policy, state, approved) =
            (&self.catalog, &self.policy, &self.state, &self.approved);
        let events = ops::recorded(state)?;
        let notes = ops::notes(state)?;
        let report = ops::analyze(catalog, policy, &events, &notes)?;
        let adopted = ops::adopted_findings(&report, &reviewed(approved)?);
        ops::write_json(out, &report, false)?;
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
        let open: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| !adopted.contains(&finding.id))
            .collect();
        println!(
            "Adopted {} findings from the reviewed skill revisions",
            adopted.len()
        );
        if open.is_empty() {
            println!("No finding needs triage");
        } else {
            for finding in &open {
                println!("Needs triage: {} ({})", finding.id, finding.kind);
            }
            println!(
                "Record these dispositions in a Review JSON with report_hash and decisions, then run skill-ops triage --decisions FILE."
            );
        }
        Ok(())
    }

    fn triage(&self, decisions: &Path, evidence_root: Option<PathBuf>) -> Result<()> {
        let (catalog, policy, state, approved) =
            (&self.catalog, &self.policy, &self.state, &self.approved);
        let review: Review = ops::read_json(decisions)?;
        let report = archived_report(state, &review.report_hash)?;
        let current = ops::analyze(catalog, policy, &ops::recorded(state)?, &ops::notes(state)?)?;
        ensure!(
            report == current,
            "analysis changed before triage; analyze the current evidence"
        );
        let evidence_root =
            skill_ops::canonical(&evidence_root.unwrap_or_else(|| self.tree.clone()))?;
        ops::check_review(
            &ops::hash(&report)?,
            &report.findings,
            &ops::adopted_findings(&report, &reviewed(approved)?),
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
        Ok(())
    }

    fn note(&self, note: &Note) -> Result<()> {
        ops::analyze(&self.catalog, &self.policy, &[], std::slice::from_ref(note))?;
        let destination = self
            .state
            .join("notes")
            .join(format!("{}.json", ops::hash(note)?));
        if destination.try_exists()? {
            ensure!(
                ops::read_json::<Note>(&destination)? == *note,
                "conflicting improvement observation"
            );
        } else {
            ops::write_json(&destination, note, false)?;
        }
        println!(
            "Recorded a local improvement observation; analysis and disposition are now required"
        );
        Ok(())
    }

    fn doctor(&self) -> Result<()> {
        let user_directory = &self.user_directory;
        let events = ops::recorded(&self.state)?;
        println!(
            "Shared catalog: {} validated skills; {} local observations",
            self.catalog.skills.len(),
            events.len()
        );
        println!(
            "Required maintenance: {}",
            if self.pending()? {
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
                skill_ops::skill_install::merge_hooks(data.clone(), client)? == data,
                "{client} observer or completion hook registration has drifted"
            );
            println!(
                "{client}: observer and completion definitions registered; runtime trust and activation require native client verification"
            );
        }
        ensure!(
            fs::read_to_string(user_directory.join(".config/opencode/plugins/skill-ops.ts"))?
                == include_str!("../../../home/dot_config/opencode/plugins/skill-ops.ts"),
            "OpenCode observer definition has drifted"
        );
        println!(
            "OpenCode: typed observer definition registered; runtime activation requires native client verification"
        );
        Ok(())
    }
}

fn main() {
    let mut cli = Cli::parse();
    let result = if matches!(cli.command, Action::Stop) {
        use skill_ops::skill_rules::{Completion, completion};
        let input = stdin_json();
        let continued = input
            .as_ref()
            .is_ok_and(|input| input["stop_hook_active"] == true);
        cli.command = Action::Check;
        let checked = match skill_ops::freshness::require_current(Built::SkillOps) {
            Err(error) => Err(error.context(
                "run `just install-skill-ops` in the dotfiles checkout to rebuild the stale hook",
            )),
            Ok(()) => input.and_then(|_| execute(cli)),
        };
        let reason = checked.as_ref().err().map(|error| format!("Skill maintenance remains incomplete: {error:#}. Run skill-ops analyze, assess the findings it lists with skill-operations, implement accepted changes, and run skill-ops triage for them before claiming completion."));
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
        skill_ops::freshness::require_current(Built::SkillOps).and_then(|()| execute(cli))
    };
    if let Err(error) = result {
        skill_ops::report(&error);
        std::process::exit(1);
    }
}
