use anyhow::Result;
use dotfiles_xtask::skill_install::merge_hooks;
use dotfiles_xtask::skill_ops::{
    Catalog, Note, Observation, Policy, Runtime, Signal, analyze, catalog, hash, observations,
    record, recorded, runtime_pending,
};
use dotfiles_xtask::skill_ops::{
    Decision, Finding, Outcome, Requirement, Review, check_review, decision_complete,
    dependency_plan,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;

fn fixture() -> Result<(tempfile::TempDir, Catalog, Policy)> {
    let root = tempfile::tempdir()?;
    for name in ["alpha", "beta"] {
        fs::create_dir(root.path().join(name))?;
        fs::write(
            root.path().join(name).join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: Inspect the {name} contract.\n---\n# Contract\nCheck the actual boundary.\n"
            ),
        )?;
    }
    let catalog = catalog(root.path())?;
    let policy = Policy {
        schema: 1,
        review_every: 3,
        minimum_pair_sessions: 2,
        pair_percent: 80,
        max_entry_words: 100,
        requirements: vec![Requirement {
            skill: "alpha".into(),
            also: names(&["beta"]),
        }],
        bundles: Default::default(),
    };
    Ok((root, catalog, policy))
}

fn observed(
    client: &str,
    session: &str,
    call: &str,
    name: &str,
    catalog: &Catalog,
    root: &std::path::Path,
) -> Result<Observation> {
    Ok(observations(client, &json!({"hook_event_name":"PostToolUse", "session_id": session, "tool_use_id":call, "tool_name":"Skill", "tool_input":{"skill":name}, "tool_response":{}}), catalog, root)?.remove(0))
}

#[test]
fn catalog_changes_follow_all_resources_and_nested_broken_links_fail() -> Result<()> {
    let (root, first, _) = fixture()?;
    fs::write(root.path().join("alpha/reference.md"), "# Reference\n")?;
    let second = catalog(root.path())?;
    assert_ne!(hash(&first)?, hash(&second)?);
    fs::write(
        root.path().join("alpha/reference.md"),
        "[Missing](missing.md)\n",
    )?;
    assert!(catalog(root.path()).is_err());
    Ok(())
}

#[test]
fn native_observation_signals_are_distinct_and_private_payloads_are_not_retained() -> Result<()> {
    let (root, catalog, _) = fixture()?;
    let mut input = json!({"hook_event_name":"PostToolUse","session_id":"private-session","tool_use_id":"private-call","tool_name":"Read","tool_input":{"file_path":root.path().join("alpha/SKILL.md"),"private":"secret-marker"},"tool_response":{}});
    let events = observations("claude", &input, &catalog, root.path())?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].signal, Signal::FileRead);
    let serialized = serde_json::to_string(&events)?;
    for private in ["private-session", "private-call", "secret-marker"] {
        assert!(!serialized.contains(private));
    }
    input["tool_response"]["is_error"] = json!(true);
    assert!(observations("claude", &input, &catalog, root.path())?.is_empty());
    input["tool_response"] = json!({"output":"SKILL_OPS_LOAD [\"beta\",\"alpha\"]\nInstructions"});
    input["tool_name"] = json!("Bash");
    input["tool_input"] = json!({"command":"skill-ops load alpha"});
    assert!(
        observations("codex", &input, &catalog, root.path())?
            .iter()
            .all(|event| event.signal == Signal::Loader)
    );
    input["tool_response"] = json!({});
    input["tool_input"] = json!({"command":"cat ~/.agents/skills/alpha/SKILL.md"});
    assert_eq!(
        observations("codex", &input, &catalog, root.path())?[0].signal,
        Signal::ShellReference
    );
    Ok(())
}

#[test]
fn concurrent_replays_publish_one_complete_event_and_corruption_is_not_reset() -> Result<()> {
    let (root, catalog, _) = fixture()?;
    let state = tempfile::tempdir()?;
    let event = observed("codex", "session", "call", "alpha", &catalog, root.path())?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let mut threads = Vec::new();
    for _ in 0..8 {
        let barrier = barrier.clone();
        let event = event.clone();
        let state = state.path().to_path_buf();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            record(&state, &event)
        }));
    }
    let mut published = 0;
    for thread in threads {
        published += usize::from(
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("recording thread panicked"))??,
        );
    }
    assert_eq!(published, 1);
    assert_eq!(recorded(state.path())?, vec![event.clone()]);
    let mut conflicting = event.clone();
    conflicting.skill = "beta".into();
    assert!(record(state.path(), &conflicting).is_err());
    let path = state
        .path()
        .join("events")
        .join(format!("{}.json", event.id));
    fs::write(&path, "{truncated")?;
    assert!(recorded(state.path()).is_err());
    assert!(record(state.path(), &event).is_err());
    assert_eq!(fs::read_to_string(path)?, "{truncated");
    Ok(())
}

#[test]
fn analysis_uses_unique_sessions_and_creates_composition_and_workflow_obligations() -> Result<()> {
    let (root, catalog, policy) = fixture()?;
    let mut events = Vec::new();
    for session in ["first", "second"] {
        for name in ["alpha", "beta"] {
            events.push(observed(
                "codex",
                session,
                name,
                name,
                &catalog,
                root.path(),
            )?);
        }
    }
    let note = Note {
        kind: "missing-skill".into(),
        skills: vec![],
        summary: "A repeated procedure has no shared entry point.".into(),
        evidence: names(&["alpha/SKILL.md"]),
    };
    let report = analyze(&catalog, &policy, &events, &[note])?;
    assert_eq!(report.sessions, 2);
    assert_eq!(report.usage["alpha"].loads, 2);
    assert_eq!(report.usage["alpha"].sessions.len(), 2);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.kind == "composition-review")
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.kind == "missing-skill")
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.kind == "dependency-observation-gap")
    );
    events.push(events[0].clone());
    assert!(analyze(&catalog, &policy, &events, &[]).is_err());
    Ok(())
}

#[test]
fn maintenance_is_driven_by_new_evidence_and_content_instead_of_elapsed_time() -> Result<()> {
    let (root, catalog, policy) = fixture()?;
    let approved = dotfiles_xtask::skill_ops::Approved {
        catalog: catalog.clone(),
        catalog_hash: hash(&catalog)?,
        policy_hash: hash(&policy)?,
        engine_hash: dotfiles_xtask::skill_ops::engine_hash()?,
    };
    let mut events = Vec::new();
    for index in 0..3 {
        events.push(observed(
            "codex",
            "session",
            &index.to_string(),
            "alpha",
            &catalog,
            root.path(),
        )?);
    }
    let pending = |events: &[Observation], report, review, notes| {
        runtime_pending(Runtime {
            catalog: &catalog,
            policy: &policy,
            events,
            notes,
            report,
            review,
            approved: &approved,
            evidence_root: root.path(),
        })
    };
    assert!(!pending(&events[..2], None, None, &[])?);
    assert!(pending(&events, None, None, &[])?);
    let report = analyze(&catalog, &policy, &events, &[])?;
    assert!(pending(&events, Some(&report), None, &[])?);
    let review = Review {
        report_hash: hash(&report)?,
        decisions: report
            .findings
            .iter()
            .map(|finding| Decision {
                id: finding.id.clone(),
                outcome: Outcome::Keep,
                reason: "The observed procedure remains separately useful.".into(),
                evidence: names(&["alpha/SKILL.md"]),
                revisit: None,
            })
            .collect(),
    };
    assert!(!pending(&events, Some(&report), Some(&review), &[])?);
    assert!(pending(&events[..2], Some(&report), Some(&review), &[]).is_err());
    let note = Note {
        kind: "mechanize".into(),
        skills: names(&["alpha"]),
        summary: "A demonstrated invariant needs a gate.".into(),
        evidence: names(&["alpha/SKILL.md"]),
    };
    assert!(pending(&events, Some(&report), Some(&review), &[note])?);
    Ok(())
}

#[test]
fn client_hook_registration_preserves_other_settings_and_is_idempotent() -> Result<()> {
    let original = json!({"model":"owner-choice","hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"existing-service session"}]}],"PostToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"existing-check"}]}]}});
    let next = merge_hooks(original.clone(), "codex")?;
    assert_eq!(next["model"], original["model"]);
    assert_eq!(
        next["hooks"]["SessionStart"],
        original["hooks"]["SessionStart"]
    );
    assert_eq!(
        next["hooks"]["PostToolUse"][0],
        original["hooks"]["PostToolUse"][0]
    );
    assert_eq!(merge_hooks(next.clone(), "codex")?, next);
    assert!(merge_hooks(json!({"hooks":false}), "codex").is_err());
    Ok(())
}

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn dependencies_load_once_before_the_requested_skill_and_cycles_are_rejected() -> Result<()> {
    let available: BTreeSet<String> = names(&["checks", "types", "tools"]).into_iter().collect();
    let requirements = vec![
        Requirement {
            skill: "tools".into(),
            also: names(&["types", "checks"]),
        },
        Requirement {
            skill: "types".into(),
            also: names(&["checks"]),
        },
    ];
    assert_eq!(
        dependency_plan(&available, &names(&["tools"]), &requirements)?,
        names(&["checks", "types", "tools"])
    );
    assert!(dependency_plan(&available, &names(&["missing"]), &requirements).is_err());
    let mut cyclic = requirements;
    cyclic.push(Requirement {
        skill: "checks".into(),
        also: names(&["tools"]),
    });
    assert!(dependency_plan(&available, &names(&["tools"]), &cyclic).is_err());
    Ok(())
}

fn finding() -> Finding {
    Finding {
        id: "catalog:types:revision".into(),
        kind: "catalog-review".into(),
        skills: names(&["types"]),
        detail: "Review the changed contract.".into(),
    }
}

fn reviewed() -> Review {
    Review {
        report_hash: "current-report".into(),
        decisions: vec![Decision {
            id: finding().id,
            outcome: Outcome::Keep,
            reason: "The procedure contains judgment while the linked gate enforces syntax.".into(),
            evidence: names(&["contract.md"]),
            revisit: None,
        }],
    }
}

#[test]
fn current_complete_review_passes_but_stale_missing_duplicate_and_foreign_decisions_fail()
-> Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("contract.md"), "# Contract\n")?;
    let findings = vec![finding()];
    let valid = reviewed();
    check_review("current-report", &findings, &valid, root.path())?;
    let mut bad = valid.clone();
    bad.report_hash = "old-report".into();
    assert!(check_review("current-report", &findings, &bad, root.path()).is_err());
    bad = valid.clone();
    bad.decisions.clear();
    assert!(check_review("current-report", &findings, &bad, root.path()).is_err());
    bad = valid.clone();
    bad.decisions.push(bad.decisions[0].clone());
    assert!(check_review("current-report", &findings, &bad, root.path()).is_err());
    bad = valid;
    bad.decisions[0].id = "unrelated".into();
    assert!(check_review("current-report", &findings, &bad, root.path()).is_err());
    Ok(())
}

#[test]
fn accepted_improvements_need_real_local_evidence_and_deferral_needs_a_revisit_condition()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let findings = vec![finding()];
    let mut review = reviewed();
    review.decisions[0].outcome = Outcome::Implemented;
    assert!(check_review("current-report", &findings, &review, root.path()).is_err());
    fs::write(root.path().join("contract.md"), "# Contract\n")?;
    check_review("current-report", &findings, &review, root.path())?;
    review.decisions[0].evidence = names(&["../outside.md"]);
    assert!(check_review("current-report", &findings, &review, root.path()).is_err());
    review.decisions[0].outcome = Outcome::Deferred;
    assert!(check_review("current-report", &findings, &review, root.path()).is_err());
    review.decisions[0].evidence = names(&["contract.md"]);
    review.decisions[0].revisit = Some("Reassess at the next observed typing failure.".into());
    check_review("current-report", &findings, &review, root.path())?;
    assert!(!decision_complete(Outcome::Deferred, true, true, false));
    assert!(!decision_complete(Outcome::Implemented, true, false, false));
    assert!(!decision_complete(Outcome::Keep, false, true, false));
    Ok(())
}
