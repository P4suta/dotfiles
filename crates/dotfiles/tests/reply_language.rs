//! Stop hook inputs from Claude Code and Codex use the final assistant text.

use anyhow::Result;
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::{Output, Stdio};

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

fn begin(home: &Path) -> Result<()> {
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home)
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .args(["answer", "begin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&json!({
            "session_id": "synthetic-session",
            "cwd": home,
            "prompt": "Check the reply language."
        }))?)?;
    let output = child.wait_with_output()?;
    assert!(output.status.success(), "{output:?}");
    Ok(())
}

fn stop(home: &Path, language: &str, input: &Value) -> Result<Output> {
    reply_gate(home, "stop", language, input)
}

fn reply_gate(home: &Path, point: &str, language: &str, input: &Value) -> Result<Output> {
    begin(home)?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home)
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("PROSE_CONFIG", root.join("home/dot_config/prose"))
        .args(["assure", point, "--reply-language", language])
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
fn a_japanese_reply_follows_the_installed_word_rules() -> Result<()> {
    let home = tempfile::tempdir()?;
    begin(home.path())?;
    let rules = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/prose");
    let stop_with_rules = |reply: &str| -> Result<Value> {
        let mut child = fixture_process::command(env!("CARGO_BIN_EXE_dotfiles"), home.path())
            .current_dir(home.path())
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("PROSE_CONFIG", &rules)
            .args(["assure", "stop", "--reply-language", "Japanese"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&input("claude", json!(reply), false))?)?;
        answer(&child.wait_with_output()?)
    };
    let narrowed = stop_with_rules(
        "\u{5909}\u{66f4}\u{3057}\u{305f}\u{306e}\u{306f}\u{3053}\u{308c}\u{3060}\u{3051}\u{3067}\u{3059}\u{3002}",
    )?;
    assert_eq!(narrowed["decision"], "block", "{narrowed}");
    assert!(
        narrowed["reason"]
            .as_str()
            .unwrap()
            .contains("Dotfiles.Restriction"),
        "{narrowed}"
    );
    assert_eq!(
        stop_with_rules(
            "\u{3060}\u{3051}\u{3069}\u{3001}\u{5909}\u{66f4}\u{6e08}\u{307f}\u{3067}\u{3059}\u{3002}\n`\u{3060}\u{3051}`\u{3092}\u{8aad}\u{307f}\u{307e}\u{3059}\u{3002}"
        )?,
        json!({}),
        "the conjunction and code stay allowed"
    );
    assert_eq!(
        stop_with_rules(
            "\u{898b}\u{3066}\u{3044}\u{305f}\u{3060}\u{3051}\u{308c}\u{3070}\u{7d9a}\u{3051}\u{307e}\u{3059}\u{3002}\n\u{6c7a}\u{3081}\u{3066}\u{3044}\u{305f}\u{3060}\u{3051}\u{307e}\u{3059}\u{304b}\u{3002}"
        )?,
        json!({}),
        "the polite verb stays allowed"
    );
    let past = stop_with_rules("\u{66f8}\u{3044}\u{305f}\u{3060}\u{3051}\u{3067}\u{3059}\u{3002}")?;
    assert_eq!(past["decision"], "block", "{past}");
    for reply in ["Ownerに確認します。", "オーナーの承認を待ちます。"] {
        let blocked = stop_with_rules(reply)?;
        assert_eq!(blocked["decision"], "block", "{reply}: {blocked}");
        assert!(
            blocked["reason"]
                .as_str()
                .unwrap()
                .contains("Dotfiles.UserAddress"),
            "{reply}: {blocked}"
        );
    }
    for reply in [
        "ファイルの所有者を確認しました。",
        "`Owner`はAPI名です。",
        "> オーナーの承認を待ちます。",
    ] {
        assert_eq!(stop_with_rules(reply)?, json!({}), "{reply}");
    }
    Ok(())
}

#[test]
fn reply_gates_distinguish_personal_address_from_qualified_ownership() -> Result<()> {
    let home = tempfile::tempdir()?;
    for point in ["stop", "reply"] {
        for (language, reply, rejected) in [
            ("English", "You are the owner.", true),
            ("English", "As the owner, you can approve this.", true),
            ("English", "The owner opens the certificate.", true),
            ("English", "The owner saves the certificate.", true),
            ("English", "Repository owner approval is required.", false),
            ("Japanese", "オーナーの承認を待ちます。", true),
            (
                "Japanese",
                "Rustのオーナーシップは値の有効期間を管理します。",
                false,
            ),
            ("Japanese", "ファイルの所有者の確認が必要です。", false),
        ] {
            let checked = answer(&reply_gate(
                home.path(),
                point,
                language,
                &input("claude", json!(reply), false),
            )?)?;
            let address = checked["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("Dotfiles.UserAddress"));
            assert_eq!(address, rejected, "{point}: {reply}: {checked}");
            if !rejected {
                assert_eq!(checked, json!({}), "{point}: {reply}");
            }
        }
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
fn both_clients_refuse_reply_sentences_that_share_or_cross_lines() -> Result<()> {
    let home = tempfile::tempdir()?;
    for client in ["claude", "codex"] {
        for (language, reply, line) in [
            (
                "Japanese",
                "4 本とも ready になりました。ただ、最後の merge が失敗しています。",
                1,
            ),
            ("Japanese", "確認した変更は\n反映されています。", 1),
            (
                "English",
                "The envelope entry docs feed the published JSON schemas, and the\nrelease gate refuses a schema change without a version bump.",
                1,
            ),
            ("English", "The checks passed. the change is ready.", 1),
        ] {
            let blocked = answer(&stop(
                home.path(),
                language,
                &input(client, json!(reply), false),
            )?)?;
            assert_eq!(blocked["decision"], "block", "{client}: {reply}: {blocked}");
            let reason = blocked["reason"].as_str().unwrap();
            assert!(reason.contains("one sentence per line"), "{reason}");
            assert!(reason.contains(&format!("line {line}")), "{reason}");
            let continued = answer(&stop(
                home.path(),
                language,
                &input(client, json!(reply), true),
            )?)?;
            assert_eq!(continued["continue"], false, "{client}: {continued}");
        }
    }
    Ok(())
}

#[test]
fn both_clients_admit_sentence_lines_and_markdown_reply_shapes() -> Result<()> {
    let home = tempfile::tempdir()?;
    for client in ["claude", "codex"] {
        for (language, reply) in [
            (
                "Japanese",
                "# 検証結果\n**結果**\n確認しました。\n変更は反映済みです。\n検証結果：\n- ビルドに成功しました。\n- テストに成功しました。\n\n| 項目 | 結果 |\n| --- | --- |\n| ビルド | 成功しました。確認済みです。 |\n\n```text\nOne. Two.\nA wrapped\nsentence.\n```\n\n> 引用しました。変更しません。\n",
            ),
            (
                "English",
                "# Results\n**Validation**\nThe checks passed.\nThe change is ready.\nResults:\n- The build passed.\n- The tests passed.\n\n| Check | Result |\n| --- | --- |\n| Build | One. Two. |\n\n```text\nOne. Two.\nA wrapped\nsentence.\n```\n",
            ),
        ] {
            let admitted = answer(&stop(
                home.path(),
                language,
                &input(client, json!(reply), false),
            )?)?;
            assert_eq!(admitted, json!({}), "{client}: {language}: {admitted}");
        }
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
