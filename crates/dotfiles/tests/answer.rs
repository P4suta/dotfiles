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
