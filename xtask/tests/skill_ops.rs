use anyhow::Result;
use dotfiles_xtask::skill_install::merge_hooks;
use dotfiles_xtask::skill_ops::{
    Catalog, Note, Observation, Policy, Runtime, Signal, analyze, catalog, hash, observations,
    record, recorded, runtime_pending,
};
use dotfiles_xtask::skill_ops::{
    Decision, Finding, Outcome, Requirement, Review, Reviewed, SizeDecision, SkillDecision,
    adoptable_revisions, adopted_findings, check_catalog, check_decisions, check_review, decide,
    decision_complete, dependency_plan, draft_decision, tracked_decisions, write_json,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
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
        reviewed: BTreeMap::new(),
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
    check_review(
        "current-report",
        &findings,
        &BTreeSet::new(),
        &valid,
        root.path(),
    )?;
    let mut bad = valid.clone();
    bad.report_hash = "old-report".into();
    assert!(
        check_review(
            "current-report",
            &findings,
            &BTreeSet::new(),
            &bad,
            root.path()
        )
        .is_err()
    );
    bad = valid.clone();
    bad.decisions.clear();
    assert!(
        check_review(
            "current-report",
            &findings,
            &BTreeSet::new(),
            &bad,
            root.path()
        )
        .is_err()
    );
    bad = valid.clone();
    bad.decisions.push(bad.decisions[0].clone());
    assert!(
        check_review(
            "current-report",
            &findings,
            &BTreeSet::new(),
            &bad,
            root.path()
        )
        .is_err()
    );
    bad = valid;
    bad.decisions[0].id = "unrelated".into();
    assert!(
        check_review(
            "current-report",
            &findings,
            &BTreeSet::new(),
            &bad,
            root.path()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn accepted_improvements_need_real_local_evidence_and_deferral_needs_a_revisit_condition()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let findings = vec![finding()];
    let mut review = reviewed();
    review.decisions[0].outcome = Outcome::Implemented;
    assert!(
        check_review(
            "current-report",
            &findings,
            &BTreeSet::new(),
            &review,
            root.path()
        )
        .is_err()
    );
    fs::write(root.path().join("contract.md"), "# Contract\n")?;
    check_review(
        "current-report",
        &findings,
        &BTreeSet::new(),
        &review,
        root.path(),
    )?;
    review.decisions[0].evidence = names(&["../outside.md"]);
    assert!(
        check_review(
            "current-report",
            &findings,
            &BTreeSet::new(),
            &review,
            root.path()
        )
        .is_err()
    );
    review.decisions[0].outcome = Outcome::Deferred;
    assert!(
        check_review(
            "current-report",
            &findings,
            &BTreeSet::new(),
            &review,
            root.path()
        )
        .is_err()
    );
    review.decisions[0].evidence = names(&["contract.md"]);
    review.decisions[0].revisit = Some("Reassess at the next observed typing failure.".into());
    check_review(
        "current-report",
        &findings,
        &BTreeSet::new(),
        &review,
        root.path(),
    )?;
    assert!(!decision_complete(Outcome::Deferred, true, true, false));
    assert!(!decision_complete(Outcome::Implemented, true, false, false));
    assert!(!decision_complete(Outcome::Keep, false, true, false));
    Ok(())
}

fn settled(revision: &str) -> SkillDecision {
    SkillDecision {
        revision: revision.into(),
        outcome: Outcome::Keep,
        reason: "The contract remains separately useful.".into(),
        evidence: names(&["alpha/SKILL.md"]),
        revisit: None,
        size: None,
    }
}

fn current_decisions(catalog: &Catalog) -> BTreeMap<String, SkillDecision> {
    catalog
        .skills
        .iter()
        .map(|(name, skill)| (name.clone(), settled(&skill.revision)))
        .collect()
}

fn refusal(result: Result<()>) -> String {
    format!(
        "{:#}",
        result.expect_err("the tracked decisions must be refused")
    )
}

#[test]
fn each_skill_is_bound_to_its_own_revision_and_refusals_name_the_draft_command() -> Result<()> {
    let (root, catalog, policy) = fixture()?;
    let decisions = current_decisions(&catalog);
    check_decisions(&catalog, &policy, &decisions, root.path())?;

    fs::write(
        root.path().join("beta/SKILL.md"),
        "---\nname: beta\ndescription: Inspect the beta contract.\n---\n# Contract\nChanged.\n",
    )?;
    let changed = dotfiles_xtask::skill_ops::catalog(root.path())?;
    assert_eq!(
        changed.skills["alpha"].revision,
        catalog.skills["alpha"].revision
    );
    let stale = refusal(check_decisions(&changed, &policy, &decisions, root.path()));
    assert!(
        stale.contains("beta") && stale.contains("just decide beta"),
        "{stale}"
    );
    assert!(!stale.contains("alpha"), "{stale}");

    let mut missing = current_decisions(&catalog);
    missing.remove("beta");
    let message = refusal(check_decisions(&catalog, &policy, &missing, root.path()));
    assert!(
        message.contains("beta") && message.contains("just decide beta"),
        "{message}"
    );

    let mut orphan = current_decisions(&catalog);
    orphan.insert("gamma".into(), settled(&catalog.skills["alpha"].revision));
    assert!(refusal(check_decisions(&catalog, &policy, &orphan, root.path())).contains("gamma"));

    let mut incomplete = current_decisions(&catalog);
    incomplete.get_mut("alpha").expect("alpha").reason.clear();
    assert!(
        refusal(check_decisions(&catalog, &policy, &incomplete, root.path())).contains("alpha")
    );
    let mut deferred = current_decisions(&catalog);
    deferred.get_mut("alpha").expect("alpha").outcome = Outcome::Deferred;
    assert!(check_decisions(&catalog, &policy, &deferred, root.path()).is_err());
    let mut escaping = current_decisions(&catalog);
    escaping.get_mut("alpha").expect("alpha").evidence = names(&["../outside.md"]);
    assert!(check_decisions(&catalog, &policy, &escaping, root.path()).is_err());
    Ok(())
}

#[test]
fn tracked_decisions_round_trip_one_file_per_skill() -> Result<()> {
    let (_root, catalog, _) = fixture()?;
    let directory = tempfile::tempdir()?;
    assert!(tracked_decisions(&directory.path().join("absent"))?.is_empty());
    let decisions = current_decisions(&catalog);
    for (name, decision) in &decisions {
        dotfiles_xtask::skill_ops::write_json(
            &directory.path().join(format!("{name}.json")),
            decision,
            false,
        )?;
    }
    assert_eq!(tracked_decisions(directory.path())?, decisions);
    fs::write(directory.path().join("notes.txt"), "stray")?;
    assert!(tracked_decisions(directory.path()).is_err());
    Ok(())
}

#[test]
fn drafts_keep_the_previous_judgment_and_require_a_new_reason() -> Result<()> {
    let (root, catalog, policy) = fixture()?;
    let mut previous = settled("old-revision");
    previous.outcome = Outcome::Implemented;
    let current = &catalog.skills["alpha"].revision;
    let draft = draft_decision(Some(&previous), current, false, "alpha")?;
    assert_eq!(draft.revision, *current);
    assert_eq!(draft.outcome, Outcome::Implemented);
    assert_eq!(draft.evidence, previous.evidence);
    assert!(draft.reason.is_empty() && draft.size.is_none());
    assert!(draft_decision(Some(&draft), current, false, "alpha").is_err());
    let mut decisions = current_decisions(&catalog);
    decisions.insert("alpha".into(), draft);
    assert!(
        refusal(check_decisions(&catalog, &policy, &decisions, root.path())).contains("reason")
    );

    let fresh = draft_decision(None, current, false, "alpha")?;
    assert_eq!(fresh.outcome, Outcome::Keep);
    assert_eq!(fresh.evidence, names(&["dot_agents/skills/alpha/SKILL.md"]));
    assert!(fresh.reason.is_empty() && fresh.revisit.is_none());
    Ok(())
}

#[test]
fn installed_reviewed_revisions_are_adopted_and_only_observation_findings_need_triage() -> Result<()>
{
    let (root, catalog, policy) = fixture()?;
    let mut decisions = current_decisions(&catalog);
    decisions.get_mut("beta").expect("beta").outcome = Outcome::Deferred;
    decisions.get_mut("beta").expect("beta").revisit = Some("Reassess on first use.".into());
    let reviewed = adoptable_revisions(&catalog, &decisions);
    assert_eq!(
        reviewed,
        BTreeMap::from([(
            "alpha".to_owned(),
            Reviewed {
                revision: catalog.skills["alpha"].revision.clone(),
                size: false,
            }
        )])
    );
    let manifest = |reviewed| -> Result<dotfiles_xtask::skill_ops::Approved> {
        Ok(dotfiles_xtask::skill_ops::Approved {
            catalog: catalog.clone(),
            catalog_hash: hash(&catalog)?,
            policy_hash: hash(&policy)?,
            engine_hash: dotfiles_xtask::skill_ops::engine_hash()?,
            reviewed,
        })
    };
    let approved = manifest(adoptable_revisions(&catalog, &current_decisions(&catalog)))?;
    let unreviewed = manifest(BTreeMap::new())?;
    let pending = |approved, events: &[Observation], report, review| {
        runtime_pending(Runtime {
            catalog: &catalog,
            policy: &policy,
            events,
            notes: &[],
            report,
            review,
            approved,
            evidence_root: root.path(),
        })
    };
    let quiet: Vec<_> = (0..3)
        .map(|index| {
            observed(
                "codex",
                "session",
                &index.to_string(),
                "beta",
                &catalog,
                root.path(),
            )
        })
        .collect::<Result<_>>()?;
    let report = analyze(&catalog, &policy, &quiet, &[])?;
    assert!(!pending(&approved, &quiet, None, None)?);
    assert!(pending(&unreviewed, &quiet, None, None)?);
    assert!(!pending(&approved, &quiet, Some(&report), None)?);
    assert!(pending(&unreviewed, &quiet, Some(&report), None)?);

    let gap: Vec<_> = (0..3)
        .map(|index| {
            observed(
                "codex",
                "session",
                &index.to_string(),
                "alpha",
                &catalog,
                root.path(),
            )
        })
        .collect::<Result<_>>()?;
    let report = analyze(&catalog, &policy, &gap, &[])?;
    assert!(pending(&approved, &gap, Some(&report), None)?);
    let only_gap = Review {
        report_hash: hash(&report)?,
        decisions: vec![Decision {
            id: "dependency:alpha:beta".into(),
            outcome: Outcome::Rejected,
            reason: "The dependency is loaded through the loader outside this sample.".into(),
            evidence: names(&["alpha/SKILL.md"]),
            revisit: None,
        }],
    };
    assert!(!pending(&approved, &gap, Some(&report), Some(&only_gap))?);
    assert!(pending(&unreviewed, &gap, Some(&report), Some(&only_gap)).is_err());
    Ok(())
}

fn sized(decision: &mut SkillDecision) {
    decision.size = Some(SizeDecision {
        outcome: Outcome::Keep,
        reason: "Every section applies to each use of the skill.".into(),
        revisit: None,
    });
}

#[test]
fn oversized_skills_need_their_own_size_disposition_under_the_current_limit() -> Result<()> {
    let (root, catalog, mut policy) = fixture()?;
    let decisions = current_decisions(&catalog);
    check_decisions(&catalog, &policy, &decisions, root.path())?;

    policy.max_entry_words = 3;
    let message = refusal(check_decisions(&catalog, &policy, &decisions, root.path()));
    assert!(
        message.contains("alpha has") && message.contains("just decide alpha"),
        "{message}"
    );
    let mut settled_sizes = current_decisions(&catalog);
    settled_sizes.values_mut().for_each(sized);
    check_decisions(&catalog, &policy, &settled_sizes, root.path())?;

    let mut incomplete = settled_sizes.clone();
    let beta = |decisions: &BTreeMap<String, SkillDecision>| -> SizeDecision {
        decisions["beta"].size.clone().expect("size")
    };
    let mut size = beta(&incomplete);
    size.reason.clear();
    incomplete.get_mut("beta").expect("beta").size = Some(size);
    let message = refusal(check_decisions(&catalog, &policy, &incomplete, root.path()));
    assert!(message.contains("size disposition for beta"), "{message}");
    assert!(!message.contains("alpha"), "{message}");

    let mut open = settled_sizes.clone();
    let mut size = beta(&open);
    size.outcome = Outcome::Deferred;
    open.get_mut("beta").expect("beta").size = Some(size.clone());
    assert!(check_decisions(&catalog, &policy, &open, root.path()).is_err());
    size.revisit = Some("Split once a second workflow appears.".into());
    open.get_mut("beta").expect("beta").size = Some(size);
    check_decisions(&catalog, &policy, &open, root.path())?;

    let reviewed = adoptable_revisions(&catalog, &open);
    assert!(reviewed["alpha"].size && !reviewed["beta"].size);
    let adopted = adopted_findings(&analyze(&catalog, &policy, &[], &[])?, &reviewed);
    let revision = |name: &str| catalog.skills[name].revision.clone();
    assert!(adopted.contains(&format!("size:alpha:{}", revision("alpha"))));
    assert!(!adopted.contains(&format!("size:beta:{}", revision("beta"))));
    assert!(adopted.contains(&format!("catalog:beta:{}", revision("beta"))));

    policy.max_entry_words = 100;
    let message = refusal(check_decisions(
        &catalog,
        &policy,
        &settled_sizes,
        root.path(),
    ));
    assert!(
        message.contains("within the limit") && message.contains("alpha"),
        "{message}"
    );
    Ok(())
}

#[test]
fn drafts_add_or_remove_the_size_disposition_that_the_limit_requires() -> Result<()> {
    let (_root, catalog, _) = fixture()?;
    let current = &catalog.skills["alpha"].revision;
    let mut previous = settled("old-revision");
    previous.size = Some(SizeDecision {
        outcome: Outcome::Deferred,
        reason: "Split after the next workflow lands.".into(),
        revisit: Some("A second workflow appears.".into()),
    });
    let draft = draft_decision(Some(&previous), current, true, "alpha")?;
    let size = draft.size.as_ref().expect("size draft");
    assert_eq!(size.outcome, Outcome::Deferred);
    assert_eq!(size.revisit, previous.size.as_ref().expect("size").revisit);
    assert!(size.reason.is_empty() && draft.reason.is_empty());
    assert!(
        draft_decision(Some(&previous), current, false, "alpha")?
            .size
            .is_none()
    );

    let bound = settled(current);
    let added = draft_decision(Some(&bound), current, true, "alpha")?;
    assert_eq!(added.reason, bound.reason);
    assert!(
        added
            .size
            .is_some_and(|size| size.reason.is_empty() && size.outcome == Outcome::Keep)
    );
    let mut extra = settled(current);
    sized(&mut extra);
    let removed = draft_decision(Some(&extra), current, false, "alpha")?;
    assert_eq!(removed.reason, extra.reason);
    assert!(removed.size.is_none());
    assert!(draft_decision(Some(&extra), current, true, "alpha").is_err());
    assert!(draft_decision(Some(&bound), current, false, "alpha").is_err());
    Ok(())
}

fn repository() -> Result<tempfile::TempDir> {
    let (skills, _, policy) = fixture()?;
    let root = tempfile::tempdir()?;
    for name in ["alpha", "beta"] {
        let tree = root.path().join("dot_agents/skills").join(name);
        fs::create_dir_all(&tree)?;
        fs::copy(
            skills.path().join(name).join("SKILL.md"),
            tree.join("SKILL.md"),
        )?;
        fs::create_dir_all(root.path().join("dot_claude/skills"))?;
        fs::write(
            root.path()
                .join("dot_claude/skills")
                .join(format!("symlink_{name}")),
            format!("../../.agents/skills/{name}\n"),
        )?;
    }
    write_json(
        &root.path().join("dot_config/skill-ops/policy.json"),
        &policy,
        false,
    )?;
    let catalog = catalog(&root.path().join("dot_agents/skills"))?;
    for (name, skill) in &catalog.skills {
        let mut decision = settled(&skill.revision);
        decision.evidence = vec![format!("dot_agents/skills/{name}/SKILL.md")];
        write_json(
            &root
                .path()
                .join("docs/skills/decisions")
                .join(format!("{name}.json")),
            &decision,
            false,
        )?;
    }
    Ok(root)
}

#[test]
fn the_required_check_depends_only_on_skills_policy_and_decisions() -> Result<()> {
    let root = repository()?;
    let decisions = tracked_decisions(&root.path().join("docs/skills/decisions"))?;
    check_catalog(root.path())?;
    for (engine, content) in [
        ("xtask/src/skill_ops.rs", "changed engine"),
        ("xtask/src/skill_rules.rs", "changed rules"),
        ("xtask/Cargo.lock", "changed lock"),
    ] {
        let path = root.path().join(engine);
        fs::create_dir_all(path.parent().expect("engine directory"))?;
        fs::write(path, content)?;
        check_catalog(root.path())?;
    }
    assert_eq!(
        tracked_decisions(&root.path().join("docs/skills/decisions"))?,
        decisions
    );
    fs::write(
        root.path().join("dot_agents/skills/beta/SKILL.md"),
        "---\nname: beta\ndescription: Inspect the beta contract.\n---\n# Contract\nChanged.\n",
    )?;
    let message = refusal(check_catalog(root.path()));
    assert!(
        message.contains("just decide beta") && !message.contains("alpha"),
        "{message}"
    );
    Ok(())
}

fn git(root: &std::path::Path, arguments: &[&str]) -> Result<String> {
    let config = root.join(".fixture-gitconfig");
    if !config.try_exists()? {
        fs::write(&config, "")?;
    }
    // The tool command drops the repository variables that a commit hook exports, so the fixture never writes to the hook's repository.
    let output = dotfiles_xtask::tool::Tool::Git
        .command()
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", &config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(arguments)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

#[test]
fn decide_writes_the_draft_and_shows_the_content_diff_since_the_recorded_decision() -> Result<()> {
    let root = repository()?;
    let path = root.path().join("docs/skills/decisions/beta.json");
    let previous: SkillDecision = dotfiles_xtask::skill_ops::read_json(&path)?;

    let refused = decide(root.path(), "beta").expect_err("a current decision has nothing to draft");
    assert!(
        format!("{refused:#}").contains("edit docs/skills/decisions/beta.json"),
        "{refused:#}"
    );
    assert!(decide(root.path(), "gamma").is_err());

    git(root.path(), &["init", "--quiet"])?;
    git(root.path(), &["add", "--all"])?;
    git(root.path(), &["commit", "--quiet", "--message", "record"])?;
    let recorded = git(root.path(), &["rev-parse", "HEAD"])?;
    let skill = root.path().join("dot_agents/skills/beta/SKILL.md");
    fs::write(&skill, skill_text("beta", "Committed change."))?;
    let context = decide(root.path(), "beta")?;
    assert!(context.contains(recorded.trim()), "{context}");
    assert!(context.contains("+Committed change."), "{context}");
    assert!(context.contains(&previous.reason), "{context}");

    let draft: SkillDecision = dotfiles_xtask::skill_ops::read_json(&path)?;
    let revision = catalog(&root.path().join("dot_agents/skills"))?.skills["beta"]
        .revision
        .clone();
    assert_eq!(draft.revision, revision);
    assert!(draft.reason.is_empty());
    assert_eq!(draft.outcome, previous.outcome);
    assert_eq!(draft.evidence, previous.evidence);
    let message = refusal(check_catalog(root.path()));
    assert!(
        message.contains("decision for beta is incomplete"),
        "{message}"
    );
    Ok(())
}

fn skill_text(name: &str, body: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Inspect the {name} contract.\n---\n# Contract\n{body}\n"
    )
}

#[test]
fn decide_diffs_from_the_commit_holding_the_assessed_content() -> Result<()> {
    let root = repository()?;
    git(root.path(), &["init", "--quiet"])?;
    git(root.path(), &["add", "dot_agents"])?;
    git(root.path(), &["commit", "--quiet", "--message", "assessed"])?;
    let assessed = git(root.path(), &["rev-parse", "HEAD"])?;
    let skill = root.path().join("dot_agents/skills/beta/SKILL.md");
    fs::write(&skill, skill_text("beta", "Intermediate change."))?;
    // The first commit of the decision holds content newer than the content it assessed.
    git(root.path(), &["add", "--all"])?;
    git(
        root.path(),
        &["commit", "--quiet", "--message", "intermediate"],
    )?;
    fs::write(&skill, skill_text("beta", "Final change."))?;

    let context = decide(root.path(), "beta")?;
    assert!(context.contains(assessed.trim()), "{context}");
    assert!(context.contains("-Check the actual boundary."), "{context}");
    assert!(context.contains("+Final change."), "{context}");
    assert!(!context.contains("Intermediate"), "{context}");
    Ok(())
}

#[test]
fn decide_says_when_no_commit_holds_the_assessed_content() -> Result<()> {
    let root = repository()?;
    let skill = root.path().join("dot_agents/skills/beta/SKILL.md");
    fs::write(&skill, skill_text("beta", "Unassessed change."))?;
    git(root.path(), &["init", "--quiet"])?;
    git(root.path(), &["add", "--all"])?;
    git(
        root.path(),
        &["commit", "--quiet", "--message", "unassessed"],
    )?;
    fs::write(&skill, skill_text("beta", "Final change."))?;

    let context = decide(root.path(), "beta")?;
    assert!(
        context.contains("No commit holds the content the previous decision assessed"),
        "{context}"
    );
    assert!(!context.contains("+Final change."), "{context}");
    Ok(())
}

fn decide_on_branch(root: &std::path::Path, branch: &str, skill: &str) -> Result<()> {
    git(root, &["switch", "--quiet", "--create", branch, "main"])?;
    fs::write(
        root.join("dot_agents/skills").join(skill).join("SKILL.md"),
        skill_text(skill, &format!("Changed on {branch}.")),
    )?;
    decide(root, skill)?;
    let path = root
        .join("docs/skills/decisions")
        .join(format!("{skill}.json"));
    let mut draft: SkillDecision = dotfiles_xtask::skill_ops::read_json(&path)?;
    draft.reason = format!("The {branch} change keeps the contract useful.");
    write_json(&path, &draft, true)?;
    check_catalog(root)?;
    git(root, &["add", "--all"])?;
    git(root, &["commit", "--quiet", "--message", branch])?;
    Ok(())
}

#[test]
fn decisions_for_different_skills_merge_in_either_order_without_conflict() -> Result<()> {
    let root = repository()?;
    git(
        root.path(),
        &["init", "--quiet", "--initial-branch", "main"],
    )?;
    git(root.path(), &["add", "--all"])?;
    git(root.path(), &["commit", "--quiet", "--message", "base"])?;
    check_catalog(root.path())?;
    decide_on_branch(root.path(), "first", "alpha")?;
    decide_on_branch(root.path(), "second", "beta")?;

    for (name, order) in [
        ("forward", ["first", "second"]),
        ("reverse", ["second", "first"]),
    ] {
        git(
            root.path(),
            &["switch", "--quiet", "--create", name, "main"],
        )?;
        for branch in order {
            git(root.path(), &["merge", "--quiet", "--no-edit", branch])?;
            check_catalog(root.path())?;
        }
    }
    assert_eq!(
        git(root.path(), &["diff", "forward", "reverse"])?,
        "",
        "both merge orders reach the same tree"
    );
    Ok(())
}

#[test]
fn the_assessed_commit_hashes_nested_resources_in_catalog_order() -> Result<()> {
    let root = repository()?;
    let tree = root.path().join("dot_agents/skills/beta");
    fs::create_dir_all(tree.join("guide/target"))?;
    fs::write(tree.join("guide.md"), "# Guide\n")?;
    fs::write(tree.join("guide/steps.md"), "# Steps\n")?;
    fs::write(tree.join("guide/target/ignored.md"), "# Ignored\n")?;
    let path = root.path().join("docs/skills/decisions/beta.json");
    let mut decision: SkillDecision = dotfiles_xtask::skill_ops::read_json(&path)?;
    decision.revision = catalog(&root.path().join("dot_agents/skills"))?.skills["beta"]
        .revision
        .clone();
    write_json(&path, &decision, true)?;
    git(root.path(), &["init", "--quiet"])?;
    git(root.path(), &["add", "--all"])?;
    git(root.path(), &["commit", "--quiet", "--message", "assessed"])?;
    let assessed = git(root.path(), &["rev-parse", "HEAD"])?;
    fs::write(tree.join("guide/steps.md"), "# Steps\nChanged.\n")?;

    let context = decide(root.path(), "beta")?;
    assert!(context.contains(assessed.trim()), "{context}");
    assert!(context.contains("+Changed."), "{context}");
    Ok(())
}
