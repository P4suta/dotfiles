// `dotguard:allow-foreign` covers the Chinese reply that the language gate test refuses.
#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use anyhow::Result;
use dotfiles_xtask::prose::{
    self, Bundle, Channel, Client, Exempt, Finding, LegacyLedger, Policy, Scope,
};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ATTRIBUTED: &str = "As requested, the parser keeps every edit.";
const COMPLIANT: &str = "The parser keeps every edit.";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn source() -> Result<Bundle> {
    Bundle::source(&root())
}

fn rules(findings: &[Finding]) -> Vec<&str> {
    findings
        .iter()
        .map(|finding| finding.rule.as_str())
        .collect()
}

fn attribution_reported(findings: &[Finding], sentence: &str) -> bool {
    findings
        .iter()
        .any(|finding| finding.rule == "Dotfiles.Attribution" && finding.sentence == sentence)
}

fn binary(arguments: &[&str], input: &str, runtime: &Path) -> Result<(i32, String, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_prose"))
        .args(arguments)
        .env("PROSE_CONFIG", root().join("dot_config/prose"))
        .env("PROSE_RUNTIME", runtime)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped input")
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    Ok((
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

#[test]
fn documents_report_the_rule_and_the_offending_sentence() -> Result<()> {
    let bundle = source()?;
    let text = format!("# Parser\n\n{ATTRIBUTED}\n");
    let findings = prose::check(&bundle, Channel::Document, "guide.md", &text)?;
    assert!(attribution_reported(&findings, ATTRIBUTED), "{findings:?}");
    assert_eq!(findings[0].path, "guide.md");
    let rendered = prose::render(&findings);
    assert!(
        rendered.contains("guide.md:3:1: Dotfiles.Attribution") && rendered.contains(ATTRIBUTED)
    );
    let compliant = format!("# Parser\n\n{COMPLIANT}\n");
    assert_eq!(
        prose::check(&bundle, Channel::Document, "guide.md", &compliant)?,
        []
    );
    Ok(())
}

#[test]
fn repository_rules_cover_hedging_comparison_lines_and_language() -> Result<()> {
    let bundle = source()?;
    let document = |text: &str| prose::check(&bundle, Channel::Document, "guide.md", text);
    assert!(
        rules(&document("The cache probably holds the index.\n")?).contains(&"Dotfiles.Hedging")
    );
    assert!(
        rules(&document("Unlike on macOS, the cache holds the index.\n")?)
            .contains(&"Dotfiles.Comparison")
    );
    assert!(
        rules(&document(
            "The cache holds the index. The parser reads it.\n"
        )?)
        .contains(&"Dotfiles.SentencePerLine")
    );
    assert!(
        rules(&document(
            "The cache holds the index.\nThe parser reads it.\n"
        )?)
        .is_empty()
    );
    assert!(rules(&document("キャッシュを読む。\n")?).contains(&"Dotfiles.English"));
    let commit = prose::check(
        &bundle,
        Channel::Commit,
        "message",
        "fix(cache): keep the index\n\nUnlike on macOS, the cache holds the index.\n",
    )?;
    assert!(!rules(&commit).contains(&"Dotfiles.Comparison"));
    Ok(())
}

#[test]
fn sentence_boundaries_ignore_abbreviations_numbers_and_code() {
    assert_eq!(prose::sentence_starts("One sentence. Two sentences."), [15]);
    assert_eq!(
        prose::sentence_starts("Use e.g. Vale or i.e. the gate."),
        [] as [usize; 0]
    );
    assert_eq!(
        prose::sentence_starts("Version 1.2. Then"),
        [] as [usize; 0]
    );
    assert_eq!(prose::sentence_starts("Read it (twice). Then stop."), [18]);
    assert_eq!(prose::sentence_starts("読む。書く。"), [4]);
    assert_eq!(prose::sentence_starts("1. Install it."), [] as [usize; 0]);
    let masked = prose::masked("Run `a. B` now.\n\n```\nOne. Two.\n```\n");
    assert!(
        masked
            .lines()
            .all(|line| prose::sentence_starts(line).is_empty())
    );
    assert_eq!(masked.lines().count(), 5);
}

#[test]
fn commit_messages_are_checked_without_commentary_trailers_or_type() -> Result<()> {
    let raw = "feat(prose)!: check messages\n\nThe gate reads the body.\n\nRefs: #44\nSigned-off-by: A <a@example.com>\n# Please enter the commit message\n";
    assert_eq!(
        prose::commit_message(raw),
        "check messages\n\nThe gate reads the body.\n"
    );
    let runtime = tempfile::tempdir()?;
    let directory = tempfile::tempdir()?;
    let message = directory.path().join("COMMIT_EDITMSG");
    let source = root();
    let arguments = |path: &Path| {
        vec![
            "--source".to_owned(),
            source.to_string_lossy().into_owned(),
            "check".into(),
            "--channel".into(),
            "commit".into(),
            path.to_string_lossy().into_owned(),
        ]
    };
    fs::write(
        &message,
        format!("fix(parser): keep edits\n\n{ATTRIBUTED}\n"),
    )?;
    let argv = arguments(&message);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let (code, _, stderr) = binary(&argv, "", runtime.path())?;
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("Dotfiles.Attribution") && stderr.contains(ATTRIBUTED));
    assert!(stderr.contains("rerun `prose check --channel commit`"));
    fs::write(
        &message,
        format!("fix(parser): keep edits\n\n{COMPLIANT}\n\nRefs #44\n"),
    )?;
    let (code, _, stderr) = binary(&argv, "", runtime.path())?;
    assert_eq!(code, 0, "{stderr}");
    Ok(())
}

#[test]
fn comments_from_ocomment_map_findings_to_source_lines() -> Result<()> {
    let scan = json!({
        "path": "src\\lib.rs",
        "language": "rust",
        "report": {"comments": [
            {"line": 1, "column": 1, "end_line": 1, "kind": "line", "text": "// Parses the header."},
            {"line": 2, "column": 1, "end_line": 2, "kind": "line", "text": format!("// {ATTRIBUTED}")},
            {"line": 4, "column": 1, "end_line": 4, "kind": "directive", "text": "// SAFETY-IGNORED: kept"},
            {"line": 5, "column": 1, "end_line": 5, "kind": "doc-line", "text": "/// Returns the count."},
            {"line": 6, "column": 25, "end_line": 6, "kind": "line", "text": "// Holds the count."}
        ]}
    });
    let inputs = prose::scanned_comments(&format!("{scan}\n"))?;
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].path, "src/lib.rs");
    assert_eq!(
        inputs[0].text,
        format!("Parses the header.\n{ATTRIBUTED}\n\nReturns the count.\n\nHolds the count.\n")
    );
    let findings = prose::check_inputs(&source()?, &inputs)?;
    assert!(attribution_reported(&findings, ATTRIBUTED), "{findings:?}");
    let finding = findings
        .iter()
        .find(|finding| finding.rule == "Dotfiles.Attribution")
        .expect("attribution finding");
    assert_eq!((finding.path.as_str(), finding.line), ("src/lib.rs", 2));
    let clean = json!({"path": "src/lib.rs", "report": {"comments": [
        {"line": 1, "column": 1, "end_line": 1, "kind": "line", "text": format!("// {COMPLIANT}")}
    ]}});
    assert_eq!(
        prose::check_inputs(&source()?, &prose::scanned_comments(&clean.to_string())?)?,
        []
    );
    assert_eq!(prose::comment_prose("  /** Reads it. */"), "Reads it.");
    assert_eq!(prose::comment_prose("## - item"), "- item");
    Ok(())
}

#[test]
fn claude_and_codex_hooks_request_one_rewrite_of_a_failing_reply() -> Result<()> {
    let runtime = tempfile::tempdir()?;
    for client in ["claude", "codex"] {
        let arguments = ["reply", "--client", client];
        let failing = json!({"last_assistant_message": ATTRIBUTED, "stop_hook_active": false});
        let (code, stdout, stderr) = binary(&arguments, &failing.to_string(), runtime.path())?;
        assert_eq!(code, 0, "{stderr}");
        let response: Value = serde_json::from_str(&stdout)?;
        assert_eq!(response["decision"], "block");
        let reason = response["reason"].as_str().unwrap_or_default();
        assert!(reason.contains("Dotfiles.Attribution") && reason.contains(ATTRIBUTED));
        let continued = json!({"last_assistant_message": ATTRIBUTED, "stop_hook_active": true});
        let (_, stdout, _) = binary(&arguments, &continued.to_string(), runtime.path())?;
        let response: Value = serde_json::from_str(&stdout)?;
        assert_eq!(response["continue"], false);
        let passing = json!({"last_assistant_message": COMPLIANT});
        let (_, stdout, _) = binary(&arguments, &passing.to_string(), runtime.path())?;
        assert_eq!(serde_json::from_str::<Value>(&stdout)?, json!({}));
    }
    let missing = tempfile::tempdir()?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_prose"));
    let output = command
        .args(["reply", "--client", "claude"])
        .env("PROSE_CONFIG", missing.path())
        .env("PROSE_RUNTIME", missing.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().expect("piped input").write_all(
                json!({"last_assistant_message": COMPLIANT})
                    .to_string()
                    .as_bytes(),
            )?;
            child.wait_with_output()
        })?;
    let response: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(response["decision"], "block");
    assert!(
        response["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("mise run install:prose")
    );
    Ok(())
}

#[test]
fn japanese_replies_pass_through_textlint_and_the_japanese_rules() -> Result<()> {
    let bundle = || source();
    let refused = prose::stop(
        &json!({"last_assistant_message": "ご依頼どおりパーサーを修正しました。"}),
        bundle(),
    );
    assert_eq!(refused["decision"], "block");
    assert!(
        refused["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("Dotfiles.Attribution")
    );
    let redundant = prose::stop(
        &json!({"last_assistant_message": "このコマンドを実行することができます。"}),
        bundle(),
    );
    assert!(
        redundant["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("textlint.ja-technical-writing/ja-no-redundant-expression"),
        "{redundant}"
    );
    let accepted = prose::stop(
        &json!({"last_assistant_message": "パーサーを修正しました。"}),
        bundle(),
    );
    assert_eq!(accepted, json!({}));
    let chinese = prose::stop(
        &json!({"last_assistant_message": "这个提交信息是中文的。"}),
        bundle(),
    );
    assert_eq!(chinese["decision"], "block");
    Ok(())
}

#[test]
fn the_reply_falls_back_to_the_last_assistant_turn_of_the_transcript() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let transcript = directory.path().join("session.jsonl");
    let lines = [
        json!({"type": "user", "message": {"content": "Fix it."}}),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "Old reply."}]}}),
        json!({"type": "user", "message": {"content": [{"type": "text", "text": "Again."}]}}),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "First part."}, {"type": "tool_use"}]}}),
        json!({"type": "user", "message": {"content": [{"type": "tool_result"}]}}),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "Second part."}]}}),
    ];
    fs::write(
        &transcript,
        lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )?;
    let input = json!({"transcript_path": transcript});
    assert_eq!(prose::reply_text(&input)?, "First part.\n\nSecond part.");
    Ok(())
}

#[test]
fn the_opencode_plugin_requests_one_rewrite_through_the_native_checker() -> Result<()> {
    let runtime = Command::new("mise")
        .current_dir(root())
        .args(["which", "bun"])
        .output()?;
    assert!(runtime.status.success());
    let bun = PathBuf::from(String::from_utf8(runtime.stdout)?.trim());
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opencode-prose.ts");
    let empty = tempfile::tempdir()?;
    let run = |reply: &str| -> Result<Vec<String>> {
        let output = Command::new(&bun)
            .current_dir(root())
            .arg(&fixture)
            .arg(env!("CARGO_BIN_EXE_prose"))
            .arg(reply)
            .env("PROSE_CONFIG", root().join("dot_config/prose"))
            .env("PROSE_RUNTIME", empty.path())
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let prompts = run(ATTRIBUTED)?;
    assert_eq!(prompts.len(), 1, "{prompts:?}");
    assert!(prompts[0].contains("Dotfiles.Attribution") && prompts[0].contains(ATTRIBUTED));
    assert_eq!(run(COMPLIANT)?, Vec::<String>::new());
    Ok(())
}

fn finding(path: &str, rule: &str) -> Finding {
    Finding {
        path: path.into(),
        line: 1,
        column: 1,
        rule: rule.into(),
        message: "message".into(),
        sentence: "Sentence.".into(),
    }
}

fn policy(exempt: Vec<Exempt>, allowed: &[(&str, &str, u32)]) -> Policy {
    let mut ledger = LegacyLedger {
        reason: "Written before the standard.".into(),
        documents: Default::default(),
        comments: Default::default(),
    };
    for (path, rule, count) in allowed {
        ledger
            .documents
            .entry((*path).into())
            .or_default()
            .insert((*rule).into(), *count);
    }
    Policy {
        exempt,
        legacy: ledger,
    }
}

#[test]
fn the_ledger_refuses_new_findings_stale_counts_and_unused_exemptions() {
    let judge = |findings: Vec<Finding>, mut policy: Policy| {
        let used = vec![false; policy.exempt.len()];
        prose::judge(Scope::Documents, findings, &mut policy, used)
    };
    let two = vec![
        finding("a.md", "Google.Passive"),
        finding("a.md", "Google.Passive"),
    ];
    assert!(
        judge(
            two.clone(),
            policy(vec![], &[("a.md", "Google.Passive", 2)])
        )
        .is_empty()
    );
    let exceeded = judge(
        two.clone(),
        policy(vec![], &[("a.md", "Google.Passive", 1)]),
    );
    assert!(exceeded[0].contains("allows 1") && exceeded[0].contains("just prose"));
    let stale = judge(two, policy(vec![], &[("a.md", "Google.Passive", 3)]));
    assert!(stale[0].contains("just prose-tighten"));
    let exempt = |reason: &str| Exempt {
        scope: Scope::Documents,
        paths: vec!["vendor/".into()],
        rules: vec!["Google.Passive".into()],
        reason: reason.into(),
    };
    assert!(
        judge(
            vec![finding("vendor/a.md", "Google.Passive")],
            policy(vec![exempt("Upstream text.")], &[])
        )
        .is_empty()
    );
    assert_eq!(
        judge(vec![], policy(vec![exempt("Upstream text.")], &[])).len(),
        1
    );
    assert_eq!(
        judge(
            vec![finding("vendor/a.md", "Google.Passive")],
            policy(vec![exempt(" ")], &[])
        )
        .len(),
        1
    );
    assert_eq!(
        judge(
            vec![finding("vendor.md", "Google.Passive")],
            policy(vec![exempt("Upstream text.")], &[])
        )
        .len(),
        2
    );
}

#[test]
fn the_repository_policy_parses_and_names_reasons() -> Result<()> {
    let policy = prose::read_policy(&root())?;
    assert!(!policy.legacy.reason.trim().is_empty());
    assert!(
        policy
            .exempt
            .iter()
            .all(|exempt| !exempt.reason.trim().is_empty())
    );
    Ok(())
}

#[test]
fn a_changed_style_package_or_a_missing_configuration_is_refused() -> Result<()> {
    let bundle = source()?;
    bundle.verify()?;
    let copy = tempfile::tempdir()?;
    let config = copy.path().join("dot_config/prose");
    let mut stack = vec![root().join("dot_config/prose")];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let target = config.join(path.strip_prefix(root().join("dot_config/prose"))?);
            if path.is_dir() {
                stack.push(path);
            } else {
                fs::create_dir_all(target.parent().expect("parent"))?;
                fs::copy(&path, &target)?;
            }
        }
    }
    let copied = Bundle::source(copy.path())?;
    copied.verify()?;
    fs::write(config.join("styles/Google/We.yml"), "extends: existence\n")?;
    let error = format!("{:#}", copied.verify().expect_err("tampered style"));
    assert!(
        error.contains("Google") && error.contains("install:prose"),
        "{error}"
    );
    fs::remove_file(config.join("packages.toml"))?;
    let error = format!("{:#}", copied.verify().expect_err("missing lock"));
    assert!(
        error.contains("missing") && error.contains("install:prose"),
        "{error}"
    );
    Ok(())
}

#[test]
fn the_pinned_vale_matches_the_mise_pin() -> Result<()> {
    let mise: toml::Value = toml::from_str(&fs::read_to_string(root().join("mise.toml"))?)?;
    let lock: toml::Value = toml::from_str(&fs::read_to_string(
        root().join("dot_config/prose/packages.toml"),
    )?)?;
    assert_eq!(mise["tools"]["vale"].as_str(), lock["vale"].as_str());
    Ok(())
}

#[test]
fn hook_registration_is_idempotent_and_keeps_other_handlers() -> Result<()> {
    let settings = json!({"hooks": {"Stop": [
        {"hooks": [{"type": "command", "command": "skill-ops stop", "timeout": 10}]},
        {"hooks": [{"type": "command", "command": "prose reply --client codex", "timeout": 5}]}
    ]}, "theme": "auto"});
    let merged = prose::merge_hooks(settings, Client::Claude)?;
    assert_eq!(prose::merge_hooks(merged.clone(), Client::Claude)?, merged);
    let stop = merged["hooks"]["Stop"].as_array().expect("Stop hooks");
    assert_eq!(stop.len(), 2);
    assert_eq!(stop[0]["hooks"][0]["command"], "skill-ops stop");
    assert_eq!(
        stop[1]["hooks"][0]["command"],
        "prose reply --client claude"
    );
    assert_eq!(merged["theme"], "auto");
    assert!(prose::merge_hooks(json!({}), Client::Opencode).is_err());
    Ok(())
}
