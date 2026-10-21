//! Stop hook inputs from Claude Code and Codex use the final assistant text.

use anyhow::Result;
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::{Output, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn stop(home: &Path, language: &str, input: &Value) -> Result<Output> {
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home)
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .args(["assure", "stop", "--reply-language", language])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(input)?)?;
    Ok(child.wait_with_output()?)
}

fn input(client: &str, reply: Value, continued: bool) -> Value {
    let mut input = json!({
        "session_id": "synthetic-session",
        "cwd": "/fixture/project",
        "hook_event_name": "Stop",
        "permission_mode": "default",
        "transcript_path": "/absent/transcript.jsonl",
        "stop_hook_active": continued
    });
    input["last_assistant_message"] = reply;
    if client == "codex" {
        input["turn_id"] = json!("synthetic-turn");
        input["model"] = json!("synthetic-model");
    }
    input
}

fn answer(output: &Output) -> Result<Value> {
    assert!(output.status.success(), "{output:?}");
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn both_clients_block_english_then_admit_a_japanese_rewrite() -> Result<()> {
    let home = tempfile::tempdir()?;
    for client in ["claude", "codex"] {
        let english = input(
            client,
            json!("The requested change is now complete."),
            false,
        );
        let blocked = answer(&stop(home.path(), "Japanese", &english)?)?;
        assert_eq!(blocked["decision"], "block", "{client}: {blocked}");
        assert!(blocked["reason"].as_str().unwrap().contains(
            "Rewrite your last reply to the user in Japanese; keep code, commands, and outward artifacts as they are."
        ));
        let rewritten = input(
            client,
            json!(
                "変更しました。\nCI と PR は確認済みです。\n```text\nfeat: add the requested hook\n```"
            ),
            true,
        );
        assert_eq!(
            answer(&stop(home.path(), "Japanese", &rewritten)?)?,
            json!({})
        );
    }
    Ok(())
}

#[test]
fn both_clients_end_an_unfixed_continuation_without_blocking_again() -> Result<()> {
    let home = tempfile::tempdir()?;
    for client in ["claude", "codex"] {
        let repeated = input(client, json!("The requested change is now complete."), true);
        let ended = answer(&stop(home.path(), "Japanese", &repeated)?)?;
        assert_eq!(ended["continue"], false, "{client}: {ended}");
        assert!(ended.get("decision").is_none());
        assert!(ended["stopReason"].as_str().unwrap().contains("Japanese"));
    }
    Ok(())
}

#[test]
fn english_configuration_and_replies_without_prose_are_admitted() -> Result<()> {
    let home = tempfile::tempdir()?;
    for client in ["claude", "codex"] {
        for reply in [
            json!(""),
            Value::Null,
            json!("OK, done."),
            json!("```sh\njust check\n```"),
        ] {
            let payload = input(client, reply, false);
            assert_eq!(
                answer(&stop(home.path(), "Japanese", &payload)?)?,
                json!({})
            );
        }
        let payload = input(
            client,
            json!("The requested change is now complete."),
            false,
        );
        assert_eq!(answer(&stop(home.path(), "English", &payload)?)?, json!({}));
    }
    Ok(())
}

#[test]
fn unsupported_languages_and_malformed_stop_inputs_fail_explicitly() -> Result<()> {
    let home = tempfile::tempdir()?;
    for (language, payload) in [
        ("French", input("codex", json!("Done."), false)),
        ("Japanese", json!({"last_assistant_message": 42})),
        ("Japanese", json!({"stop_hook_active": "true"})),
    ] {
        assert!(!stop(home.path(), language, &payload)?.status.success());
    }
    Ok(())
}
