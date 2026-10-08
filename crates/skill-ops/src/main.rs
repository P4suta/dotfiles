use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use skill_ops::freshness::Built;
use skill_ops::refusal::Refusal;
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
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    /// Records one incident of a rule, unless the recorder marks the failure expected.
    Incident {
        #[arg(long)]
        rule: String,
        #[arg(long)]
        session: String,
        #[arg(long, default_value = "gate-refusal")]
        source: String,
        #[arg(long)]
        evidence: Vec<String>,
        #[arg(long)]
        red_test: bool,
        #[arg(long)]
        counterexample: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Records the prevention decision of one rule's incidents.
    Discharge {
        #[arg(long)]
        rule: String,
        #[arg(long)]
        evidence: Vec<String>,
        #[arg(long)]
        exempt_reason: Option<String>,
        #[arg(long)]
        reconsideration: Option<String>,
        #[arg(long, default_value = ".")]
        root: PathBuf,
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

/// The store that holds the content of a review's evidence, named by the digest of its evidence hashes, so the review outlives the checkout it cited.
fn evidence_store(state: &Path, hashes: &BTreeMap<String, String>) -> Result<PathBuf> {
    Ok(state.join("review-evidence").join(ops::hash(hashes)?))
}

/// Copies each cited evidence file from `root` into `store`, accepting an existing copy only when it holds the same bytes.
fn record_evidence(store: &Path, root: &Path, hashes: &BTreeMap<String, String>) -> Result<()> {
    for name in hashes.keys() {
        let bytes = fs::read(root.join(name))
            .with_context(|| format!("read review evidence {name} under {}", root.display()))?;
        let copy = store.join(name);
        if copy.try_exists()? {
            ensure!(
                fs::read(&copy)? == bytes,
                "the recorded copy of review evidence {name} differs from the evidence it records"
            );
        } else {
            ops::write_file(&copy, &bytes, false)?;
        }
    }
    Ok(())
}

/// The command that records a fresh review, which every refusal of recorded evidence names.
const RETRIAGE: &str = "skill-ops analyze --out <report.json> && skill-ops triage --decisions <decisions.json> --evidence-root <checkout>";

/// The evidence store of a recorded review, once its content matches the review's hashes.
/// A review recorded before the store existed names only its checkout, so this copies its evidence from that checkout while the checkout still holds it.
fn recorded_evidence(state: &Path, reviewed: &RuntimeReview) -> Result<PathBuf> {
    let store = evidence_store(state, &reviewed.evidence_hashes)?;
    let root = &reviewed.evidence_root;
    if !store.try_exists()? {
        let current = root
            .is_dir()
            .then(|| ops::evidence_hashes(&reviewed.review, root).ok())
            .flatten();
        if current.as_ref() != Some(&reviewed.evidence_hashes) {
            let cause = if root.is_dir() {
                "the review recorded its evidence only by path, and that checkout no longer holds the evidence it reviewed"
            } else {
                "the review recorded its evidence only by path, and that checkout no longer exists"
            };
            return Err(Refusal::new("skill-ops.review-evidence", cause, RETRIAGE)
                .evidence(format!("evidence checkout: {}", root.display()))
                .evidence(format!(
                    "review record: {}",
                    state.join("review.json").display()
                ))
                .into());
        }
        record_evidence(&store, root, &reviewed.evidence_hashes)?;
    }
    let verified = ops::evidence_hashes(&reviewed.review, &store).ok();
    if verified.as_ref() != Some(&reviewed.evidence_hashes) {
        return Err(Refusal::new(
            "skill-ops.review-evidence",
            "the recorded copy of the review's evidence no longer matches the hashes the review holds",
            RETRIAGE,
        )
        .evidence(format!("evidence store: {}", store.display()))
        .into());
    }
    Ok(store)
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
    let recorded = reviewed
        .as_ref()
        .map(|reviewed| recorded_evidence(state, reviewed))
        .transpose()?;
    let baseline = ops::read_approved(approved)?;
    let runtime = Runtime {
        catalog,
        policy,
        events: &events,
        notes: &notes,
        report: report.as_ref().or(draft.as_ref()),
        review: reviewed.as_ref().map(|item| &item.review),
        approved: &baseline,
        evidence_root: recorded.as_deref().unwrap_or(root),
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
        Commands::Load { names, bundle } => session.load(names, bundle)?,
        Commands::Observe { client } => {
            let input = stdin_json()?;
            ops::record_native_failure(&session.state, &client, &input)?;
            for observation in ops::observations(&client, &input, &session.catalog, &session.tree)?
            {
                ops::record(&session.state, &observation)?;
            }
        }
        Commands::Analyze { out } => session.analyze(&out)?,
        Commands::Triage {
            decisions,
            evidence_root,
        } => session.triage(&decisions, evidence_root)?,
        Commands::Note {
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
        Commands::Incident {
            rule,
            session: incident_session,
            source,
            evidence,
            red_test,
            counterexample,
            dry_run,
        } => session.incident(
            &rule,
            &incident_session,
            &source,
            evidence,
            red_test,
            counterexample,
            dry_run,
        )?,
        Commands::Discharge {
            rule,
            evidence,
            exempt_reason,
            reconsideration,
            root,
        } => session.discharge(&rule, evidence, exempt_reason, reconsideration, &root)?,
        Commands::Check => {
            if let Some(open) = session.open_incidents()? {
                anyhow::bail!("{open}");
            }
            ensure!(
                !session.pending()?,
                "skill maintenance is pending; run skill-ops analyze --out FILE and triage the findings it lists with skill-ops triage --decisions FILE"
            );
        }
        Commands::Stop => unreachable!("Stop is translated into a checked completion request"),
        Commands::Doctor => session.doctor()?,
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

    /// Records one incident of a rule, unless the recorder marks the failure expected.
    #[expect(
        clippy::too_many_arguments,
        reason = "the command line holds one flag per expected-failure fact"
    )]
    fn incident(
        &self,
        rule: &str,
        incident_session: &str,
        source: &str,
        evidence: Vec<String>,
        red_test: bool,
        counterexample: bool,
        dry_run: bool,
    ) -> Result<()> {
        let source = skill_ops::incident_rules::Source::from_key(source)
            .with_context(|| format!("unknown incident source `{source}`"))?;
        let incident = ops::Incident::new(rule, incident_session, source, evidence)?;
        let expected = skill_ops::incident_rules::Expected {
            red_test,
            counterexample,
            dry_run,
        };
        if ops::record_incident(&self.state, &incident, expected)? {
            println!("Recorded an incident of `{rule}`; a discharge is now required");
        } else {
            println!("Recorded no new incident");
        }
        Ok(())
    }

    /// Records the prevention decision of one rule's incidents.
    fn discharge(
        &self,
        rule: &str,
        evidence: Vec<String>,
        exempt_reason: Option<String>,
        reconsideration: Option<String>,
        root: &Path,
    ) -> Result<()> {
        let exemption = exempt_reason
            .as_deref()
            .and_then(skill_ops::incident_rules::Reason::from_key);
        let outcome = if evidence.is_empty() {
            "exempt"
        } else {
            "mechanized"
        }
        .into();
        ops::record_discharge(
            &self.state,
            root,
            &ops::Discharge {
                schema: 1,
                rule: rule.into(),
                outcome,
                evidence,
                reason: exempt_reason,
                reconsideration,
            },
        )?;
        match exemption {
            Some(reason) => println!(
                "Recorded the discharge of `{rule}`: {}",
                skill_ops::incident_rules::reason_text(reason)
            ),
            None => println!("Recorded the mechanized discharge of `{rule}`"),
        }
        Ok(())
    }

    /// The refusal that names every rule whose incidents lack a prevention decision, ranked by repetition first.
    fn open_incidents(&self) -> Result<Option<String>> {
        let open = ops::open_incident_rules(&self.state)?;
        if open.is_empty() {
            return Ok(None);
        }
        let names: Vec<String> = open
            .iter()
            .map(|(rule, repetitions, rank)| {
                let weight = if *rank == skill_ops::incident_rules::Rank::Repeated {
                    "repeated "
                } else {
                    ""
                };
                format!("{rule} ({weight}{repetitions} incidents)")
            })
            .collect();
        Ok(Some(format!(
            "incidents of {} lack a prevention decision.\nNext action: {}",
            names.join(", "),
            skill_ops::incident_rules::next_action()
        )))
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
        let evidence_hashes = ops::evidence_hashes(&review, &evidence_root)?;
        record_evidence(
            &evidence_store(state, &evidence_hashes)?,
            &evidence_root,
            &evidence_hashes,
        )?;
        let reviewed = RuntimeReview {
            evidence_root,
            evidence_hashes,
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
    let result = if matches!(cli.command, Commands::Stop) {
        use skill_ops::skill_rules::{Completion, completion};
        let input = stdin_json();
        let continued = input
            .as_ref()
            .is_ok_and(|input| input["stop_hook_active"] == true);
        cli.command = Commands::Check;
        let checked = match skill_ops::freshness::require_current(Built::SkillOps) {
            Err(error) => Err(error.context(
                "run `just install-skill-ops` in the dotfiles checkout to rebuild the stale hook",
            )),
            Ok(()) => input.and_then(|_| execute(cli)),
        };
        let reason = checked.as_ref().err().map(|error| match error.downcast_ref::<Refusal>() {
            Some(refusal) => format!(
                "Skill maintenance remains incomplete: {}. Next: {}",
                refusal.cause,
                refusal.next.as_deref().unwrap_or("none")
            ),
            None => format!("Skill maintenance remains incomplete: {error:#}. Run skill-ops analyze, assess the findings it lists with skill-operations, implement accepted changes, and run skill-ops triage for them before claiming completion."),
        });
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
