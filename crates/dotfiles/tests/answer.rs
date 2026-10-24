#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

#[test]
fn an_unverified_argument_cannot_create_answer_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let draft = directory.path().join("draft.txt");
    let evidence = directory.path().join("evidence.json");
    std::fs::write(&draft, "The conclusion follows.\n").unwrap();
    std::fs::write(&evidence, r#"{"version":1,"claims":[],"readings":[]}"#).unwrap();
    let output = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), directory.path())
        .args(["answer", "check", "--session", "test-session", "--draft"])
        .arg(draft)
        .arg("--evidence")
        .arg(evidence)
        .env("HOME", directory.path())
        .env("USERPROFILE", directory.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("answer request"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
use std::io::Write as _;
use std::path::Path;
use std::process::{Output, Stdio};

const PROPOSAL_PLAN: &str = "<proposed_plan>\n# Ownership verification\n\n## Changes\n\n- Add the supplied configuration.\n\n```json\n{\"enabled\":true}\n```\n</proposed_plan>\n";

fn proposal_evidence(draft: &str) -> serde_json::Value {
    let claims: Vec<_> = draft
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, text)| {
            serde_json::json!({
                "id": format!("plan-{index}"),
                "text": text,
                "basis": {"kind": "proposal", "reason": "Proposed configuration change"}
            })
        })
        .collect();
    serde_json::json!({"version":1, "atoms":[], "claims":claims, "readings":[]})
}

#[test]
fn a_balanced_plan_supplies_proposal_context_without_losing_line_coverage() {
    let evidence = proposal_evidence(PROPOSAL_PLAN);
    dotfiles::answer::inspect(PROPOSAL_PLAN, &evidence.to_string(), "Plan the change").unwrap();
    let mut missing = evidence.clone();
    missing["claims"].as_array_mut().unwrap().remove(2);
    assert!(
        dotfiles::answer::inspect(PROPOSAL_PLAN, &missing.to_string(), "Plan the change").is_err()
    );
    let mut factual = evidence.clone();
    factual["claims"][3]["basis"] = serde_json::json!({
        "kind":"observation", "record":"not-captured", "excerpt":"passed", "exit":0
    });
    assert!(
        dotfiles::answer::inspect(PROPOSAL_PLAN, &factual.to_string(), "Plan the change").is_err()
    );
    for malformed in [
        "# Unlabeled prose\n",
        "<proposed_plan>\n# Missing closing delimiter\n",
        "</proposed_plan>\n",
        "<proposed_plan>\n<proposed_plan>\n</proposed_plan>\n</proposed_plan>\n",
        "<proposed_plan>\n# First\n</proposed_plan>\n<proposed_plan>\n# Second\n</proposed_plan>\n",
        "<proposed_plan>\n# Plan\n</proposed_plan>\nUnlabeled prose outside the plan.\n",
    ] {
        assert!(
            dotfiles::answer::inspect(
                malformed,
                &proposal_evidence(malformed).to_string(),
                "Plan the change"
            )
            .is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn the_real_hook_binds_a_checked_plan_to_its_exact_draft_and_request() {
    let home = tempfile::tempdir().unwrap();
    let request = r#"{"session_id":"plan-fixture","prompt":"Plan the change"}"#;
    assert!(hook(home.path(), "begin", request).status.success());
    let draft = home.path().join("plan.txt");
    let evidence = home.path().join("plan.json");
    std::fs::write(&draft, PROPOSAL_PLAN).unwrap();
    std::fs::write(&evidence, proposal_evidence(PROPOSAL_PLAN).to_string()).unwrap();
    let checked = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home.path())
        .args(["answer", "check", "--session", "plan-fixture", "--draft"])
        .arg(&draft)
        .arg("--evidence")
        .arg(&evidence)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let mut reply = serde_json::json!({
        "session_id":"plan-fixture", "last_assistant_message":PROPOSAL_PLAN, "stop_hook_active":false
    });
    let allowed = hook(home.path(), "stop", &reply.to_string());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&allowed.stdout).unwrap(),
        serde_json::json!({})
    );
    reply["last_assistant_message"] =
        serde_json::json!(PROPOSAL_PLAN.replace("supplied", "changed"));
    let changed = hook(home.path(), "stop", &reply.to_string());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&changed.stdout).unwrap()["decision"],
        "block"
    );
    reply["last_assistant_message"] = serde_json::json!(PROPOSAL_PLAN);
    hook(home.path(), "begin", request);
    let stale = hook(home.path(), "stop", &reply.to_string());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stale.stdout).unwrap()["decision"],
        "block"
    );
}

#[test]
fn plan_scope_preserves_fenced_examples_and_checks_outside_the_block() {
    for fence in ["```", "````", "~~~"] {
        let draft = format!(
            "Proposal: Prepare the change.\n<proposed_plan>\n# Changes\n{fence}text\n</proposed_plan>\n<proposed_plan>\n{fence}\n- Add the configuration.\n</proposed_plan>\nProposal: Verify the change.\n"
        );
        dotfiles::answer::inspect(
            &draft,
            &proposal_evidence(&draft).to_string(),
            "Plan the change",
        )
        .unwrap();
    }
    let draft = "```text\n<proposed_plan>\n# A literal example\n</proposed_plan>\n```\n";
    let mut evidence = proposal_evidence(draft);
    for index in [0, 4] {
        evidence["claims"][index]["basis"] = serde_json::json!({"kind":"user", "quote":"```"});
    }
    assert!(dotfiles::answer::inspect(draft, &evidence.to_string(), draft).is_err());
    let mut evidence = proposal_evidence(PROPOSAL_PLAN);
    evidence["claims"][1]["basis"]["reason"] = serde_json::json!("");
    assert!(
        dotfiles::answer::inspect(PROPOSAL_PLAN, &evidence.to_string(), "Plan the change").is_err()
    );
}

#[test]
fn a_plan_proposal_cannot_become_a_formal_premise() {
    let draft = "<proposed_plan>\nP.\n</proposed_plan>\nHypothesis: P implies Q.\nUnder these assumptions: Q.\n";
    let mut evidence = proposal_evidence(draft);
    evidence["atoms"] = serde_json::json!(["P", "Q"]);
    evidence["claims"][1]["formula"] = serde_json::json!({"op":"atom", "args":0});
    evidence["claims"][3]["formula"] = serde_json::json!({
        "op":"implies", "args":[{"op":"atom", "args":0}, {"op":"atom", "args":1}]
    });
    evidence["claims"][3]["basis"] =
        serde_json::json!({"kind":"hypothesis", "reason":"Declared implication"});
    evidence["claims"][4]["formula"] = serde_json::json!({"op":"atom", "args":1});
    evidence["claims"][4]["basis"] = serde_json::json!({
        "kind":"inference", "conditional":true,
        "argument":{"premises":["plan-1", "plan-3"], "conclusion":{"op":"atom", "args":1}}
    });
    let error =
        dotfiles::answer::inspect(draft, &evidence.to_string(), "Plan the change").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot support a formal inference"),
        "{error}"
    );
}
fn hook(home: &Path, point: &str, input: &str) -> Output {
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home)
        .args(["answer", point])
        .env("HOME", home)
        .env("USERPROFILE", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn malformed_hook_payloads_end_with_an_explicit_refusal() {
    let home = tempfile::tempdir().unwrap();
    for point in ["begin", "stop"] {
        for input in ["{", "{}", "null"] {
            let output = hook(home.path(), point, input);
            assert!(output.status.success());
            let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(response["continue"], false, "{response}");
            assert_eq!(response["decision"], "block", "{response}");
        }
    }
}

#[test]
fn the_real_hook_accepts_only_the_current_checked_draft() {
    let home = tempfile::tempdir().unwrap();
    let request = r#"{"session_id":"fixture","prompt":"Check the claim"}"#;
    let begun = hook(home.path(), "begin", request);
    assert!(begun.status.success());
    assert!(String::from_utf8_lossy(&begun.stdout).contains("--session fixture"));
    let draft = home.path().join("draft.txt");
    let evidence = home.path().join("evidence.json");
    std::fs::write(&draft, "Can we test this?\n").unwrap();
    std::fs::write(&evidence, r#"{"version":1,"atoms":[],"claims":[{"id":"q","text":"Can we test this?","formula":null,"basis":{"kind":"question"}}],"readings":[]}"#).unwrap();
    let checked = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home.path())
        .args(["answer", "check", "--session", "fixture", "--draft"])
        .arg(&draft)
        .arg("--evidence")
        .arg(&evidence)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let reply = r#"{"session_id":"fixture","last_assistant_message":"Can we test this?","stop_hook_active":false}"#;
    let allowed = hook(home.path(), "stop", reply);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&allowed.stdout).unwrap(),
        serde_json::json!({})
    );
    let changed = hook(home.path(), "stop", &reply.replace("this?", "that?"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&changed.stdout).unwrap()["decision"],
        "block"
    );
    hook(home.path(), "begin", request);
    let stale = hook(home.path(), "stop", reply);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stale.stdout).unwrap()["decision"],
        "block"
    );
    let stopped = hook(home.path(), "stop", &reply.replace("false", "true"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stopped.stdout).unwrap()["continue"],
        false
    );
}
