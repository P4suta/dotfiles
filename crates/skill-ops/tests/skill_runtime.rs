use anyhow::Result;
use serde_json::{Value, json};
use skill_ops::{self as ops, Decision, Outcome, Policy, Review};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn fixture() -> Result<tempfile::TempDir> {
    let directory = match std::env::var_os(ops::INCIDENT_EVIDENCE_ROOT) {
        Some(root) => tempfile::tempdir_in(fixture_process::directory(Path::new(&root)))?,
        None => tempfile::tempdir()?,
    };
    let tree = directory.path().join("tree");
    for name in ["alpha", "beta"] {
        fs::create_dir_all(tree.join(name))?;
        fs::write(
            tree.join(name).join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: Check the {name} boundary.\n---\n# Contract\nValidate the actual input.\n"
            ),
        )?;
    }
    let policy = Policy {
        schema: 1,
        review_every: 3,
        minimum_pair_sessions: 2,
        pair_percent: 80,
        max_entry_words: 100,
        requirements: vec![ops::Requirement {
            skill: "alpha".into(),
            dependencies: vec!["beta".into()],
        }],
        bundles: std::collections::BTreeMap::default(),
    };
    ops::write_json(&directory.path().join("policy.json"), &policy, false)?;
    ops::write_json(
        &directory.path().join("approved.json"),
        &ops::Approved {
            catalog: ops::catalog(&tree)?,
            catalog_hash: ops::hash(&ops::catalog(&tree)?)?,
            policy_hash: ops::hash(&policy)?,
            engine_hash: ops::engine_hash()?,
            reviewed: std::collections::BTreeMap::default(),
        },
        false,
    )?;
    Ok(directory)
}

fn run(root: &Path, arguments: &[&str], input: Option<&Value>) -> Result<Output> {
    run_with_label(root, arguments, input, None)
}

fn counterexample(root: &Path, arguments: &[&str], input: Option<&Value>) -> Result<Output> {
    run_with_label(root, arguments, input, Some("counterexample"))
}

fn run_with_label(
    root: &Path,
    arguments: &[&str],
    input: Option<&Value>,
    label: Option<&str>,
) -> Result<Output> {
    let mut command = fixture_process::command(env!("CARGO_BIN_EXE_skill-ops"), root);
    command
        .current_dir(root)
        .args(["--tree"])
        .arg(root.join("tree"))
        .args(["--policy"])
        .arg(root.join("policy.json"))
        .args(["--state"])
        .arg(root.join("state"))
        .args(["--approved"])
        .arg(root.join("approved.json"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(label) = label {
        command.env("DOTFILES_EXPECTED_FAILURE", label);
    } else {
        command.env_remove("DOTFILES_EXPECTED_FAILURE");
    }
    let mut child = command.spawn()?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("piped input")
            .write_all(&serde_json::to_vec(input)?)?;
    } else {
        drop(child.stdin.take());
    }
    Ok(child.wait_with_output()?)
}

fn success(output: Output) -> Result<Output> {
    anyhow::ensure!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

#[test]
fn build_and_dependency_outputs_are_outside_chezmoi_source_state() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let checkout = directory.path().join("checkout");
    let source = checkout.join("home");
    let destination = directory.path().join("native-home");
    fs::create_dir_all(&source)?;
    fs::create_dir_all(&destination)?;
    fs::write(checkout.join(".chezmoiroot"), "home\n")?;
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/.chezmoiignore"),
        source.join(".chezmoiignore"),
    )?;
    fs::create_dir_all(source.join(".chezmoitemplates/partials"))?;
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../home/.chezmoitemplates/partials/host-state-ignore"),
        source.join(".chezmoitemplates/partials/host-state-ignore"),
    )?;
    fs::create_dir_all(source.join(".chezmoidata"))?;
    for data in ["profiles.toml", "targets.toml", "scripts.toml"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../home/.chezmoidata")
                .join(data),
            source.join(".chezmoidata").join(data),
        )?;
    }
    fs::write(source.join("dot_owned"), "managed configuration")?;
    for relative in [
        "home/target/checks/debug/generated",
        "home/node_modules/dependency/generated",
        "target/checks/debug/generated",
        "node_modules/dependency/generated",
        "xtask/target/debug/generated",
    ] {
        let path = checkout.join(relative);
        fs::create_dir_all(path.parent().expect("generated output parent"))?;
        fs::write(path, "changing build output")?;
    }
    let config = directory.path().join("chezmoi.toml");
    fs::write(
        &config,
        "[data]\nprofile = 'linux'\n[data.platforms.linux]\n",
    )?;
    let output = success(
        platform::process::command("chezmoi")
            .arg("--config")
            .arg(config)
            .arg("--source")
            .arg(checkout)
            .arg("--destination")
            .arg(destination)
            .arg("--persistent-state")
            .arg(directory.path().join("state.boltdb"))
            .args(["managed", "--include", "files", "--path-style", "relative"])
            .output()?,
    )?;
    assert_eq!(
        String::from_utf8(output.stdout)?
            .lines()
            .collect::<Vec<_>>(),
        [".owned"]
    );
    Ok(())
}

#[test]
fn cli_loads_prerequisites_and_refuses_unknown_names() -> Result<()> {
    let fixture = fixture()?;
    let loaded = success(run(fixture.path(), &["load", "alpha", "beta"], None)?)?;
    assert!(String::from_utf8(loaded.stdout)?.starts_with("SKILL_OPS_LOAD [\"beta\",\"alpha\"]\n"));
    assert!(
        !run(fixture.path(), &["load", "unknown"], None)?
            .status
            .success()
    );
    Ok(())
}

#[test]
fn fixture_failures_preserve_the_machine_incident_log() -> Result<()> {
    fn snapshot(state: &Path) -> Result<std::collections::BTreeMap<PathBuf, Vec<u8>>> {
        let mut records = std::collections::BTreeMap::new();
        let directory = state.join("incidents");
        if directory.try_exists()? {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    records.insert(PathBuf::from(entry.file_name()), fs::read(entry.path())?);
                }
            }
        }
        Ok(records)
    }
    let home = platform::home::home().expect("the machine home");
    let machine_state = home.join(fixture_process::MACHINE_STATE);
    let before = snapshot(&machine_state)?;
    let fixture = fixture()?;
    let output = run(fixture.path(), &["load", "unknown"], None)?;
    assert!(!output.status.success());
    let incidents = ops::incidents(&fixture.path().join("state"))?;
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].rule, "tool.failure");
    assert_eq!(snapshot(&machine_state)?, before);
    if std::env::var_os(ops::INCIDENT_EVIDENCE_ROOT).is_some() {
        let _ = fixture.keep();
    }
    Ok(())
}

#[test]
fn relative_analysis_paths_create_missing_parents_without_overwriting_reports() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    for path in ["report.json", "reports/nested/report.json"] {
        success(run(root, &["analyze", "--out", path], None)?)?;
        let report: ops::Report = ops::read_json(&root.join(path))?;
        assert_eq!(report.findings.len(), 2);
        assert!(
            !run(root, &["analyze", "--out", path], None)?
                .status
                .success()
        );
    }
    Ok(())
}

#[test]
fn stop_refuses_missing_or_pending_state_and_bounds_automatic_continuation() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    let clear = success(run(root, &["stop"], Some(&json!({})))?)?;
    assert_eq!(serde_json::from_slice::<Value>(&clear.stdout)?, json!({}));
    fs::remove_file(root.join("approved.json"))?;
    let first = success(run(root, &["stop"], Some(&json!({})))?)?;
    assert_eq!(
        serde_json::from_slice::<Value>(&first.stdout)?["decision"],
        "block"
    );
    let next = success(run(
        root,
        &["stop"],
        Some(&json!({"stop_hook_active":true})),
    )?)?;
    assert_eq!(
        serde_json::from_slice::<Value>(&next.stdout)?["continue"],
        false
    );
    fs::remove_file(root.join("tree/alpha/SKILL.md"))?;
    let broken_catalog = success(run(root, &["stop"], Some(&json!({})))?)?;
    assert_eq!(
        serde_json::from_slice::<Value>(&broken_catalog.stdout)?["decision"],
        "block"
    );
    assert!(!run(root, &["check"], None)?.status.success());
    Ok(())
}

#[test]
fn triage_retains_history_and_changed_evidence_invalidates_completion() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    success(run(
        root,
        &[
            "note",
            "--kind",
            "mechanize",
            "--skill",
            "alpha",
            "--summary",
            "A repeated boundary needs a gate.",
            "--evidence",
            "alpha/SKILL.md",
        ],
        None,
    )?)?;
    assert!(!counterexample(root, &["check"], None)?.status.success());
    let output = root.join("first.json");
    success(run(
        root,
        &["analyze", "--out", output.to_str().expect("UTF-8 fixture")],
        None,
    )?)?;
    assert!(
        !counterexample(
            root,
            &["analyze", "--out", output.to_str().expect("UTF-8 fixture")],
            None
        )?
        .status
        .success()
    );
    let report: ops::Report = ops::read_json(&output)?;
    let review = Review {
        report_hash: ops::hash(&report)?,
        decisions: report
            .findings
            .iter()
            .map(|finding| Decision {
                id: finding.id.clone(),
                outcome: Outcome::Keep,
                reason: "The fixture isolates evidence freshness and preserves its contract."
                    .into(),
                evidence: vec!["alpha/SKILL.md".into()],
                revisit: None,
            })
            .collect(),
    };
    let decisions = root.join("decisions.json");
    ops::write_json(&decisions, &review, false)?;
    let evidence = root.join("tree");
    success(run(
        root,
        &[
            "triage",
            "--decisions",
            decisions.to_str().expect("UTF-8 fixture"),
            "--evidence-root",
            evidence.to_str().expect("UTF-8 fixture"),
        ],
        None,
    )?)?;
    success(run(root, &["check"], None)?)?;
    assert_eq!(fs::read_dir(root.join("state/reports"))?.count(), 1);
    assert_eq!(fs::read_dir(root.join("state/reviews"))?.count(), 1);
    let old_decision = fs::read(root.join("state/review.json"))?;
    fs::write(evidence.join("alpha/SKILL.md"), "changed evidence")?;
    assert!(!counterexample(root, &["check"], None)?.status.success());
    assert_eq!(fs::read(root.join("state/review.json"))?, old_decision);
    assert_eq!(fs::read_dir(root.join("state/reviews"))?.count(), 1);
    Ok(())
}

/// Triages every finding of a fresh analysis with one evidence file under a disposable checkout.
fn triage_from_checkout(root: &Path, checkout: &Path) -> Result<()> {
    fs::create_dir_all(checkout.join("alpha"))?;
    fs::write(checkout.join("alpha/notes.md"), "reviewed evidence\n")?;
    success(run(root, &["analyze", "--out", "report.json"], None)?)?;
    let report: ops::Report = ops::read_json(&root.join("report.json"))?;
    let review = Review {
        report_hash: ops::hash(&report)?,
        decisions: report
            .findings
            .iter()
            .map(|finding| Decision {
                id: finding.id.clone(),
                outcome: Outcome::Keep,
                reason: "The fixture reviews evidence from a checkout that later disappears."
                    .into(),
                evidence: vec!["alpha/notes.md".into()],
                revisit: None,
            })
            .collect(),
    };
    ops::write_json(&root.join("decisions.json"), &review, false)?;
    success(run(
        root,
        &[
            "triage",
            "--decisions",
            "decisions.json",
            "--evidence-root",
            checkout.to_str().expect("UTF-8 fixture"),
        ],
        None,
    )?)?;
    Ok(())
}

#[test]
fn a_review_survives_its_removed_or_rewritten_evidence_checkout() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    let checkout = root.join("worktree");
    triage_from_checkout(root, &checkout)?;
    success(run(root, &["check"], None)?)?;
    fs::write(checkout.join("alpha/notes.md"), "a later commit\n")?;
    success(run(root, &["check"], None)?)?;
    fs::remove_dir_all(&checkout)?;
    success(run(root, &["check"], None)?)?;
    let stop = success(run(root, &["stop"], Some(&json!({})))?)?;
    assert_eq!(serde_json::from_slice::<Value>(&stop.stdout)?, json!({}));
    Ok(())
}

#[test]
fn a_path_only_review_whose_checkout_is_gone_is_refused_with_the_next_action() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    let checkout = root.join("worktree");
    triage_from_checkout(root, &checkout)?;
    fs::remove_dir_all(root.join("state/review-evidence"))?;
    fs::remove_dir_all(&checkout)?;
    let refused = run(root, &["check"], None)?;
    assert!(!refused.status.success());
    let stderr = String::from_utf8(refused.stderr)?;
    assert!(!stderr.contains("os error"), "{stderr}");
    let line = stderr
        .lines()
        .find_map(platform::refusal::Refusal::parse)
        .unwrap_or_else(|| panic!("no structured refusal: {stderr}"));
    assert_eq!(line.rule, "skill-ops.review-evidence");
    assert!(
        line.next
            .as_deref()
            .is_some_and(|next| next.contains("skill-ops triage")),
        "{line:?}"
    );
    let stop = success(run(root, &["stop"], Some(&json!({})))?)?;
    let reason = serde_json::from_slice::<Value>(&stop.stdout)?["reason"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        reason.contains("skill-ops triage") && !reason.contains("os error"),
        "{reason}"
    );
    Ok(())
}

#[test]
fn concurrent_drafts_preserve_accepted_review_but_new_notes_and_lost_events_block() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    success(run(root, &["analyze", "--out", "accepted.json"], None)?)?;
    let report: ops::Report = ops::read_json(&root.join("accepted.json"))?;
    let review = Review {
        report_hash: ops::hash(&report)?,
        decisions: report
            .findings
            .iter()
            .map(|finding| Decision {
                id: finding.id.clone(),
                outcome: Outcome::Keep,
                reason: "Preserve the fixture contracts while evaluating draft publication.".into(),
                evidence: vec!["beta/SKILL.md".into()],
                revisit: None,
            })
            .collect(),
    };
    ops::write_json(&root.join("decisions.json"), &review, false)?;
    success(run(
        root,
        &["triage", "--decisions", "decisions.json"],
        None,
    )?)?;
    success(run(root, &["check"], None)?)?;
    success(run(
        root,
        &["observe", "--client", "claude"],
        Some(
            &json!({"hook_event_name":"PostToolUse", "session_id":"draft-session", "tool_use_id":"draft-call", "tool_name":"Skill", "tool_input":{"skill":"beta"}, "tool_response":{}}),
        ),
    )?)?;
    success(run(root, &["analyze", "--out", "draft.json"], None)?)?;
    success(run(root, &["check"], None)?)?;
    success(run(
        root,
        &[
            "note",
            "--kind",
            "failure",
            "--skill",
            "beta",
            "--summary",
            "The next independent failure needs a current disposition.",
            "--evidence",
            "beta/SKILL.md",
        ],
        None,
    )?)?;
    assert!(!counterexample(root, &["check"], None)?.status.success());
    let event = ops::recorded(&root.join("state"))?.remove(0);
    fs::remove_file(root.join("state/events").join(format!("{}.json", event.id)))?;
    let lost = counterexample(root, &["check"], None)?;
    assert!(!lost.status.success());
    assert!(String::from_utf8(lost.stderr)?.contains("observed events disappeared"));
    let stored = root.join("state/review.json");
    let mut invalid: Value = ops::read_json(&stored)?;
    invalid["review"]["report_hash"] = json!("../outside");
    ops::write_json(&stored, &invalid, true)?;
    let escaping = counterexample(root, &["check"], None)?;
    assert!(!escaping.status.success());
    assert!(String::from_utf8(escaping.stderr)?.contains("invalid immutable report identity"));
    Ok(())
}

#[test]
fn a_reviewed_installed_catalog_passes_the_gate_without_analysis_or_triage() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    let manifest = root.join("approved.json");
    let mut approved: ops::Approved = ops::read_json(&manifest)?;
    approved.reviewed = approved
        .catalog
        .skills
        .iter()
        .map(|(name, skill)| {
            (
                name.clone(),
                ops::Reviewed {
                    revision: skill.revision.clone(),
                    size: false,
                },
            )
        })
        .collect();
    ops::write_json(&manifest, &approved, true)?;
    for call in ["first", "second", "third"] {
        success(run(
            root,
            &["observe", "--client", "claude"],
            Some(
                &json!({"hook_event_name":"PostToolUse", "session_id":"session", "tool_use_id":call, "tool_name":"Skill", "tool_input":{"skill":"beta"}, "tool_response":{}}),
            ),
        )?)?;
    }
    success(run(root, &["check"], None)?)?;
    let analysis = success(run(root, &["analyze", "--out", "report.json"], None)?)?;
    assert!(String::from_utf8(analysis.stdout)?.contains("No finding needs triage"));
    success(run(root, &["check"], None)?)?;
    success(run(
        root,
        &[
            "note",
            "--kind",
            "failure",
            "--skill",
            "beta",
            "--summary",
            "A demonstrated failure still needs a disposition.",
            "--evidence",
            "beta/SKILL.md",
        ],
        None,
    )?)?;
    assert!(!run(root, &["check"], None)?.status.success());
    let analysis = success(run(root, &["analyze", "--out", "noted.json"], None)?)?;
    let listed = String::from_utf8(analysis.stdout)?;
    assert!(listed.contains("Needs triage: observation:"), "{listed}");
    assert!(!listed.contains("Needs triage: catalog:"), "{listed}");
    assert!(!run(root, &["check"], None)?.status.success());
    Ok(())
}

#[test]
fn opencode_adapter_records_a_native_sdk_read_without_retaining_private_payloads() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    let executable = Path::new(env!("CARGO_BIN_EXE_skill-ops"));
    let runtime = success(
        platform::process::command("bun")
            .args(["-e", "console.log(process.execPath)"])
            .output()?,
    )?;
    let runtime = String::from_utf8(runtime.stdout)?;
    let runtime = Path::new(runtime.trim());
    assert!(runtime.is_absolute() && runtime.is_file());
    let output = fixture_process::command(runtime, root)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opencode-adapter.ts"))
        .arg(root)
        .arg(executable)
        .env("PATH", "")
        .env("SKILL_OPS_TREE", root.join("tree"))
        .env("SKILL_OPS_POLICY", root.join("policy.json"))
        .env("SKILL_OPS_STATE", root.join("state"))
        .env("SKILL_OPS_APPROVED", root.join("approved.json"))
        .output()?;
    success(output)?;
    let observations = ops::recorded(&root.join("state"))?;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].client, "opencode");
    assert_eq!(observations[0].skill, "alpha");
    assert_eq!(observations[0].signal, ops::Signal::FileRead);
    let stored = serde_json::to_string(&observations)?;
    assert!(!stored.contains("private-fixture-session"));
    assert!(!stored.contains("private-fixture-call"));
    Ok(())
}

/// A source checkout and a native user tree that both hold the fixture skills, with the policy and a decision record for each skill.
fn native_source(fixture: &Path) -> Result<(PathBuf, PathBuf, ops::Catalog)> {
    let source = fixture.join("source");
    let user = fixture.join("native-user");
    for name in ["alpha", "beta"] {
        for tree in [
            source.join("home/dot_agents/skills"),
            user.join(".agents/skills"),
        ] {
            fs::create_dir_all(tree.join(name))?;
            fs::copy(
                fixture.join("tree").join(name).join("SKILL.md"),
                tree.join(name).join("SKILL.md"),
            )?;
        }
        fs::create_dir_all(source.join("home/dot_claude/skills"))?;
        fs::write(
            source
                .join("home/dot_claude/skills")
                .join(format!("symlink_{name}")),
            format!("../../.agents/skills/{name}\n"),
        )?;
    }
    let policy: Policy = ops::read_json(&fixture.join("policy.json"))?;
    ops::write_json(
        &source.join("home/dot_config/skill-ops/policy.json"),
        &policy,
        false,
    )?;
    ops::write_json(&user.join(".config/skill-ops/policy.json"), &policy, false)?;
    let source_catalog = ops::catalog(&source.join("home/dot_agents/skills"))?;
    for (name, skill) in &source_catalog.skills {
        ops::write_json(
            &source.join(ops::DECISIONS).join(format!("{name}.json")),
            &ops::SkillDecision {
                revision: skill.revision.clone(),
                outcome: if name == "beta" {
                    Outcome::Deferred
                } else {
                    Outcome::Keep
                },
                reason: "The fixture preserves its separately named native contracts.".into(),
                evidence: vec!["home/dot_agents/skills/alpha/SKILL.md".into()],
                revisit: (name == "beta").then(|| "Reassess once beta is observed.".into()),
                size: None,
            },
            false,
        )?;
    }
    Ok((source, user, source_catalog))
}

/// Runs the built skill-ops for the native user with `input` on its standard input.
fn native(
    fixture: &Path,
    user: &Path,
    arguments: &[&str],
    input: Option<&Value>,
) -> Result<Output> {
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_skill-ops"), fixture)
        .current_dir(fixture)
        .env("HOME", user)
        .env("USERPROFILE", user)
        .arg("--state")
        .arg(fixture.join("native-state"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped input");
    if let Some(input) = input {
        stdin.write_all(&serde_json::to_vec(input)?)?;
    }
    drop(stdin);
    Ok(child.wait_with_output()?)
}

#[test]
fn native_installation_preserves_settings_and_history_across_reinstall() -> Result<()> {
    let fixture = fixture()?;
    let (source, user, source_catalog) = native_source(fixture.path())?;
    let existing = json!({"model":"owner-choice","hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"existing-helper session"}]}]}});
    ops::write_json(&user.join(".codex/hooks.json"), &existing, false)?;
    ops::write_json(&user.join(".claude/settings.json"), &existing, false)?;
    let historical = user.join(".local/state/skill-ops/old-evidence.json");
    ops::write_json(
        &historical,
        &json!({"preserve":"machine-local-history"}),
        false,
    )?;
    let original = fs::read(&historical)?;
    let legacy = user.join(".agents/skills/alpha/legacy-helper");
    fs::write(&legacy, "unmanaged native helper")?;
    let binary = Path::new(env!("CARGO_BIN_EXE_skill-ops"));
    let installed = user
        .join(".local/bin")
        .join(format!("skill-ops{}", std::env::consts::EXE_SUFFIX));
    let adapter = user.join(".config/opencode/plugins/skill-ops.ts");
    let policy_path = source.join("home/dot_config/skill-ops/policy.json");
    let adapter_path = source.join("home/dot_config/opencode/plugins/skill-ops.ts");
    fs::create_dir_all(adapter_path.parent().expect("adapter directory"))?;
    fs::write(&adapter_path, "native fixture adapter")?;
    let mut applied = Vec::new();
    let mut apply = |path: &Path| -> Result<()> {
        applied.push(path.to_path_buf());
        let destination = if path == policy_path {
            user.join(".config/skill-ops/policy.json")
        } else {
            assert_eq!(path, adapter_path);
            assert!(installed.is_file());
            adapter.clone()
        };
        fs::create_dir_all(destination.parent().expect("native parent"))?;
        fs::copy(path, destination)?;
        Ok(())
    };
    let native_skill = user.join(".agents/skills/alpha/SKILL.md");
    let native_bytes = fs::read(&native_skill)?;
    fs::write(&native_skill, "unrelated first-install edit")?;
    assert!(skill_ops::skill_install::deploy(&source, binary, &user, &mut apply).is_err());
    assert!(!adapter.exists() && !installed.exists());
    fs::write(&native_skill, native_bytes)?;
    skill_ops::skill_install::deploy(&source, binary, &user, &mut apply)?;
    assert_eq!(applied, [policy_path.clone(), policy_path, adapter_path]);
    let manifest: ops::Approved = ops::read_json(&user.join(".config/skill-ops/approved.json"))?;
    assert_eq!(
        manifest.reviewed,
        std::collections::BTreeMap::from([(
            "alpha".to_owned(),
            ops::Reviewed {
                revision: source_catalog.skills["alpha"].revision.clone(),
                size: false,
            }
        )])
    );
    let native =
        |arguments: &[&str], input: Option<&Value>| native(fixture.path(), &user, arguments, input);
    for call in ["first", "second", "third"] {
        success(native(
            &["observe", "--client", "claude"],
            Some(
                &json!({"hook_event_name":"PostToolUse", "session_id":"session", "tool_use_id":call, "tool_name":"Skill", "tool_input":{"skill":"beta"}, "tool_response":{}}),
            ),
        )?)?;
    }
    assert!(!native(&["check"], None)?.status.success());
    let analysis = success(native(&["analyze", "--out", "native-report.json"], None)?)?;
    let listed = String::from_utf8(analysis.stdout)?;
    assert!(listed.contains("Needs triage: catalog:beta:"), "{listed}");
    assert!(!listed.contains("Needs triage: catalog:alpha:"), "{listed}");
    assert_eq!(fs::read_to_string(&adapter)?, "native fixture adapter");
    let first: Value = ops::read_json(&user.join(".codex/hooks.json"))?;
    assert_eq!(first["model"], existing["model"]);
    assert_eq!(
        first["hooks"]["SessionStart"],
        existing["hooks"]["SessionStart"]
    );
    skill_ops::skill_install::install(&source, binary, &user)?;
    assert_eq!(
        ops::read_json::<Value>(&user.join(".codex/hooks.json"))?,
        first
    );
    assert_eq!(fs::read(historical)?, original);
    assert_eq!(fs::read_to_string(legacy)?, "unmanaged native helper");
    success(
        fixture_process::command(installed, fixture.path())
            .arg("--help")
            .output()?,
    )?;
    fs::write(
        user.join(".agents/skills/alpha/SKILL.md"),
        "unrelated native edit",
    )?;
    assert!(skill_ops::skill_install::install(&source, binary, &user).is_err());
    assert_eq!(
        fs::read_to_string(user.join(".agents/skills/alpha/SKILL.md"))?,
        "unrelated native edit"
    );
    Ok(())
}

#[test]
fn analysis_without_a_manifest_requires_triage_of_every_finding() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    fs::remove_file(root.join("approved.json"))?;
    let analysis = success(run(root, &["analyze", "--out", "report.json"], None)?)?;
    let listed = String::from_utf8(analysis.stdout)?;
    assert!(listed.contains("Adopted 0 findings"), "{listed}");
    assert!(listed.contains("Needs triage: catalog:alpha:"), "{listed}");
    assert!(listed.contains("Needs triage: catalog:beta:"), "{listed}");
    Ok(())
}

#[test]
fn a_manifest_without_reviewed_revisions_is_refused_with_the_install_command() -> Result<()> {
    let fixture = fixture()?;
    let root = fixture.path();
    let manifest = root.join("approved.json");
    let mut legacy: Value = ops::read_json(&manifest)?;
    legacy
        .as_object_mut()
        .expect("manifest object")
        .remove("reviewed");
    ops::write_json(&manifest, &legacy, true)?;
    for arguments in [&["check"][..], &["analyze", "--out", "report.json"][..]] {
        let refused = run(root, arguments, None)?;
        assert!(!refused.status.success());
        let message = String::from_utf8(refused.stderr)?;
        assert!(message.contains("just install-skill-ops"), "{message}");
    }
    Ok(())
}
