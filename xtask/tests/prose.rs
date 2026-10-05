// `dotguard:allow-foreign` covers the Chinese reply that the language gate test refuses.
#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use anyhow::Result;
use dotfiles_xtask::prose::{self, Bundle, Channel, Client, Exempt, Finding, Policy, Scope};
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
        rules(&document("Unlike on Linux, the cache holds the index.\n")?)
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
fn git_generated_messages_pass_and_added_prose_is_checked() -> Result<()> {
    let bundle = source()?;
    for raw in [
        "Revert \"feat(prose): enforce one writing standard\"\n\nThis reverts commit 39f8a76.\n",
        "Revert \"Revert \"feat(prose): enforce one writing standard\"\"\n\nThis reverts commit 39f8a76.\n",
        "Reapply \"feat(prose): enforce one writing standard\"\n\nThis reverts commit 39f8a76d.\n",
        "Revert \"Merge branch 'feat/x'\"\n\nThis reverts commit 39f8a76, reversing\nchanges made to b158608.\n",
        "Revert \"feat(parser): add file\"\n\nThis reverts commit dbf0602 (feat(parser): add file, 2026-10-05).\n",
        "revert(parser): drop the file cache\n\n# *** SAY WHY WE ARE REVERTING ON THE TITLE LINE ***\n\nThis reverts commit dbf0602 (feat(parser): add file, 2026-10-05).\n",
        "revert(parser): drop the merge\n\nThis reverts commit 39f8a76 (Merge branch 'feat/x', 2026-10-04), reversing\nchanges made to b158608 (fix(parser): keep edits, 2026-10-03).\n",
        "fixup! feat(prose): enforce one writing standard\n",
        "fixup! fixup! feat(prose): enforce one writing standard\n",
        "squash! feat(prose): enforce one writing standard\n\nThe gate reads the body.\n",
        "amend! feat(prose): enforce one writing standard\n\nfeat(prose): enforce the writing standard\n\nThe gate reads the body.\n\nRefs #44\n",
        "Merge branch 'feat/writing-standard' into main\n",
        "Merge remote-tracking branch 'origin/main' (early part)\n\n# Conflicts:\n#\tREADME.md\n",
        "fix(parser): keep edits\n\nThe parser keeps every edit.\n\n(cherry picked from commit 39f8a76d0c1e)\n",
    ] {
        let findings = prose::check(&bundle, Channel::Commit, "commit message", raw)?;
        assert!(findings.is_empty(), "{raw:?}: {findings:?}");
    }
    for raw in [
        format!("Revert \"feat(prose): x\"\n\nThis reverts commit 39f8a76.\n\n{ATTRIBUTED}\n"),
        format!(
            "revert(prose): x\n\nThis reverts commit 39f8a76 (feat(prose): x, 2026-10-05).\n\n{ATTRIBUTED}\n"
        ),
        format!("squash! feat(prose): x\n\n{ATTRIBUTED}\n"),
        format!("amend! feat(prose): x\n\nfix(parser): keep edits\n\n{ATTRIBUTED}\n"),
        format!("Merge branch 'feat/x'\n\n{ATTRIBUTED}\n"),
        format!("fixup! {ATTRIBUTED}\n\n{ATTRIBUTED}\n"),
    ] {
        let findings = prose::check(&bundle, Channel::Commit, "commit message", &raw)?;
        assert!(
            attribution_reported(&findings, ATTRIBUTED),
            "{raw:?}: {findings:?}"
        );
    }
    let findings = prose::check(
        &bundle,
        Channel::Commit,
        "commit message",
        &format!("Revert the parser cache\n\n{ATTRIBUTED}\n"),
    )?;
    assert!(attribution_reported(&findings, ATTRIBUTED));
    let undated = format!("This reverts commit 39f8a76 ({ATTRIBUTED}).");
    let findings = prose::check(
        &bundle,
        Channel::Commit,
        "commit message",
        &format!("revert(prose): x\n\n{undated}\n"),
    )?;
    assert!(
        attribution_reported(&findings, &undated),
        "a sentence that only resembles a commit reference is checked: {findings:?}"
    );
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
    let session = |active: bool| {
        json!({"session_id": "session-1", "last_assistant_message": ATTRIBUTED, "stop_hook_active": active})
            .to_string()
    };
    let arguments = ["reply", "--client", "claude"];
    let (_, stdout, _) = binary(&arguments, &session(true), runtime.path())?;
    assert_eq!(
        serde_json::from_str::<Value>(&stdout)?["decision"],
        "block",
        "another hook's continuation still earns one rewrite request"
    );
    let (_, stdout, _) = binary(&arguments, &session(true), runtime.path())?;
    assert_eq!(serde_json::from_str::<Value>(&stdout)?["continue"], false);
    let (_, stdout, _) = binary(&arguments, &session(true), runtime.path())?;
    assert_eq!(
        serde_json::from_str::<Value>(&stdout)?["decision"],
        "block",
        "ending the turn clears this hook's mark"
    );
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
    let run_with = |executable: &Path, reply: &str| -> Result<(Vec<String>, Vec<String>)> {
        let output = Command::new(&bun)
            .current_dir(root())
            .arg(&fixture)
            .arg(executable)
            .arg(reply)
            .env("PROSE_CONFIG", root().join("dot_config/prose"))
            .env("PROSE_RUNTIME", empty.path())
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout)?;
        let list = |name: &str| -> Vec<String> {
            result[name]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        };
        Ok((list("prompts"), list("toasts")))
    };
    let run = |reply: &str| run_with(Path::new(env!("CARGO_BIN_EXE_prose")), reply);
    let (prompts, toasts) = run(ATTRIBUTED)?;
    assert_eq!(prompts.len(), 1, "{prompts:?}");
    assert!(prompts[0].contains("Dotfiles.Attribution") && prompts[0].contains(ATTRIBUTED));
    assert_eq!(
        toasts.len(),
        1,
        "the second failure shows its findings: {toasts:?}"
    );
    assert!(toasts[0].contains("Dotfiles.Attribution") && toasts[0].contains(ATTRIBUTED));
    assert_eq!(run(COMPLIANT)?, (Vec::new(), Vec::new()));
    let missing = empty.path().join("bin").join("prose");
    let (prompts, toasts) = run_with(&missing, ATTRIBUTED)?;
    assert!(prompts.is_empty(), "{prompts:?}");
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert!(toasts[0].contains("mise run install:prose"));
    Ok(())
}

fn finding(path: &str, rule: &str, sentence: &str) -> Finding {
    Finding {
        path: path.into(),
        line: 1,
        column: 1,
        rule: rule.into(),
        message: "message".into(),
        sentence: sentence.into(),
    }
}

#[test]
fn every_unexempted_finding_is_refused() {
    let judge = |findings: Vec<Finding>, exempt: Vec<Exempt>| {
        let policy = Policy { exempt };
        let used = vec![false; policy.exempt.len()];
        prose::judge(Scope::Documents, findings, &policy, used)
    };
    let old = "The index is read by the parser.";
    let refused = judge(vec![finding("a.md", "Google.Passive", old)], vec![]);
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(refused[0].contains(old) && refused[0].contains("just prose"));
    assert!(judge(vec![], vec![]).is_empty());
    let exempt = |reason: &str| Exempt {
        scope: Scope::Documents,
        paths: vec!["vendor/".into()],
        rules: vec!["Google.Passive".into()],
        reason: reason.into(),
    };
    assert!(
        judge(
            vec![finding("vendor/a.md", "Google.Passive", old)],
            vec![exempt("Upstream text.")]
        )
        .is_empty()
    );
    assert_eq!(judge(vec![], vec![exempt("Upstream text.")]).len(), 1);
    assert_eq!(
        judge(
            vec![finding("vendor/a.md", "Google.Passive", old)],
            vec![exempt(" ")]
        )
        .len(),
        1
    );
    assert_eq!(
        judge(
            vec![finding("vendor.md", "Google.Passive", old)],
            vec![exempt("Upstream text.")]
        )
        .len(),
        2
    );
}

#[test]
fn the_repository_policy_parses_and_names_reasons() -> Result<()> {
    let policy = prose::read_policy(&root())?;
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
    copy_tree(&root().join("dot_config/prose"), &config)?;
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

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let mut stack = vec![from.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let target = to.join(path.strip_prefix(from)?);
            if path.is_dir() {
                stack.push(path);
            } else {
                fs::create_dir_all(target.parent().expect("parent"))?;
                fs::copy(&path, &target)?;
            }
        }
    }
    Ok(())
}

fn git(directory: &Path, arguments: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(directory)
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .stdout(Stdio::null())
        .status()?;
    assert!(status.success(), "git {arguments:?}");
    Ok(())
}

/// A repository holding the pinned configuration, its exemptions, and the given files.
fn repository(files: &[(&str, &str)]) -> Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    copy_tree(
        &root().join("dot_config/prose"),
        &directory.path().join("dot_config/prose"),
    )?;
    fs::create_dir_all(directory.path().join("policy"))?;
    fs::write(
        directory.path().join("policy/prose.toml"),
        "[[exempt]]\nscope = \"documents\"\npaths = [\"dot_config/prose/styles/\"]\nreason = \"Vendored Vale packages keep their upstream text.\"\n\n[[exempt]]\nscope = \"comments\"\npaths = [\"dot_config/prose/\"]\nreason = \"The pinned configuration is data.\"\n",
    )?;
    for (path, text) in files {
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().expect("parent"))?;
        fs::write(path, text)?;
    }
    git(directory.path(), &["init", "--quiet"])?;
    git(directory.path(), &["add", "--all"])?;
    Ok(directory)
}

/// The prose arguments of a command in the repository's `lefthook.yml`.
fn hook_arguments(hook: &str, command: &str) -> Result<Vec<String>> {
    let documents =
        yaml_rust2::YamlLoader::load_from_str(&fs::read_to_string(root().join("lefthook.yml"))?)?;
    let run = documents[0][hook]["commands"][command]["run"]
        .as_str()
        .expect("the hook command")
        .to_owned();
    let (_, arguments) = run
        .split_once(" --bin prose -- ")
        .expect("the hook runs the prose binary");
    assert!(run.starts_with("mise x -- cargo run --locked --manifest-path xtask/Cargo.toml"));
    Ok(arguments.split_whitespace().map(str::to_owned).collect())
}

fn run_in(directory: &Path, arguments: &[String]) -> Result<(i32, String)> {
    let output = Command::new(env!("CARGO_BIN_EXE_prose"))
        .current_dir(directory)
        .args(arguments)
        .env("PROSE_RUNTIME", directory.join(".prose-runtime"))
        .stdin(Stdio::null())
        .output()?;
    Ok((
        output.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    ))
}

#[test]
fn the_document_gate_refuses_new_prose_in_a_repository() -> Result<()> {
    let refused = repository(&[("guide.md", &format!("# Parser\n\n{ATTRIBUTED}\n"))])?;
    let bundle = Bundle::source(refused.path())?;
    let error = format!(
        "{:#}",
        prose::repository(refused.path(), Scope::Documents, &bundle)
            .expect_err("an attributing document")
    );
    assert!(
        error.contains("guide.md:3:1: Dotfiles.Attribution")
            && error.contains(ATTRIBUTED)
            && error.contains("just prose"),
        "{error}"
    );
    let accepted = repository(&[("guide.md", &format!("# Parser\n\n{COMPLIANT}\n"))])?;
    prose::repository(
        accepted.path(),
        Scope::Documents,
        &Bundle::source(accepted.path())?,
    )?;
    Ok(())
}

#[test]
fn the_pre_commit_comment_gate_reads_source_comments_through_ocomment() -> Result<()> {
    let arguments = hook_arguments("pre-commit", "comments")?;
    assert_eq!(
        arguments,
        ["--source", ".", "repository", "--scope", "comments"]
    );
    let refused = repository(&[(
        "src/lib.rs",
        &format!(
            "// The cache holds the index. The parser reads it.\n// {ATTRIBUTED}\npub fn parse() {{}}\n"
        ),
    )])?;
    let (code, output) = run_in(refused.path(), &arguments)?;
    assert_ne!(code, 0, "{output}");
    assert!(
        output.contains("src/lib.rs:1:") && output.contains("Dotfiles.SentencePerLine"),
        "{output}"
    );
    assert!(
        output.contains("src/lib.rs:2:1: Dotfiles.Attribution") && output.contains(ATTRIBUTED),
        "{output}"
    );
    let (code, output) = run_in(
        refused.path(),
        &[
            "--source",
            ".",
            "check",
            "--channel",
            "comment",
            "src/lib.rs",
        ]
        .map(str::to_owned),
    )?;
    assert_eq!(code, 1, "{output}");
    assert!(output.contains("Dotfiles.SentencePerLine"), "{output}");
    let accepted = repository(&[(
        "src/lib.rs",
        &format!("// The cache holds the index.\n// {COMPLIANT}\npub fn parse() {{}}\n"),
    )])?;
    let (code, output) = run_in(accepted.path(), &arguments)?;
    assert_eq!(code, 0, "{output}");
    Ok(())
}

#[test]
fn the_commit_msg_hook_runs_the_tested_check() -> Result<()> {
    let arguments = hook_arguments("commit-msg", "prose")?;
    assert_eq!(
        arguments,
        ["--source", ".", "check", "--channel", "commit", "{1}"]
    );
    let directory = repository(&[])?;
    let message = directory.path().join("COMMIT_EDITMSG");
    let arguments: Vec<String> = arguments
        .iter()
        .map(|argument| {
            if argument == "{1}" {
                message.to_string_lossy().into_owned()
            } else {
                argument.clone()
            }
        })
        .collect();
    fs::write(
        &message,
        format!("fix(parser): keep edits\n\n{ATTRIBUTED}\n"),
    )?;
    let (code, output) = run_in(directory.path(), &arguments)?;
    assert_eq!(code, 1, "{output}");
    assert!(output.contains("Dotfiles.Attribution") && output.contains(ATTRIBUTED));
    fs::write(
        &message,
        format!("fix(parser): keep edits\n\n{COMPLIANT}\n"),
    )?;
    assert_eq!(run_in(directory.path(), &arguments)?.0, 0);
    Ok(())
}

#[test]
fn the_destination_decides_which_repositories_take_the_standard() {
    use dotfiles_xtask::prose_rules::Standard::{Applies, Skips, Undetermined};
    let personal = ["P4suta".to_owned()];
    let remotes = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|(name, url)| ((*name).to_owned(), (*url).to_owned()))
            .collect()
    };
    // OpenSSH resolves the `gh-personal` host entry to github.com.
    let resolve = |host: &str| (host == "gh-personal").then(|| "github.com".to_owned());
    for (declared, list, standard) in [
        (
            None,
            vec![("origin", "git@github.com:P4suta/dotfiles.git")],
            Applies,
        ),
        (
            None,
            vec![("origin", "https://github.com/p4suta/dotfiles")],
            Applies,
        ),
        (
            None,
            vec![("origin", "ssh://git@github.com/P4suta/dotfiles.git")],
            Applies,
        ),
        (
            None,
            vec![("origin", "ssh://git@github.com:22/P4suta/dotfiles.git")],
            Applies,
        ),
        (
            None,
            vec![("origin", "gh-personal:P4suta/dotfiles.git")],
            Applies,
        ),
        (
            None,
            vec![("origin", "ssh://gh-personal/P4suta/dotfiles.git")],
            Applies,
        ),
        (
            None,
            vec![("origin", "https://github.com/other/project.git")],
            Skips,
        ),
        (
            None,
            vec![("origin", "gh-personal:other/project.git")],
            Skips,
        ),
        (
            None,
            vec![
                ("origin", "git@github.com:P4suta/project.git"),
                ("upstream", "https://github.com/other/project.git"),
            ],
            Skips,
        ),
        (
            None,
            vec![
                ("origin", "git@github.com:P4suta/project.git"),
                ("backup", "git@github.com:P4suta/project-backup.git"),
            ],
            Applies,
        ),
        (
            None,
            vec![("upstream", "git@github.com:P4suta/project.git")],
            Applies,
        ),
        (
            None,
            vec![("github", "git@github.com:P4suta/project.git")],
            Applies,
        ),
        (
            None,
            vec![
                ("github", "git@github.com:P4suta/project.git"),
                ("upstream", "https://github.com/other/project.git"),
            ],
            Skips,
        ),
        (
            None,
            vec![("upstream", "https://github.com/other/project.git")],
            Skips,
        ),
        (
            None,
            vec![("backup", "hub:repositories/project.git")],
            Undetermined,
        ),
        (None, vec![], Undetermined),
        (Some(false), vec![], Skips),
        (
            None,
            vec![("origin", "hub:repositories/dotfiles.git")],
            Undetermined,
        ),
        (
            None,
            vec![("origin", "/srv/hub/dotfiles.git")],
            Undetermined,
        ),
        (
            None,
            vec![("origin", "git@gitlab.com:P4suta/x.git")],
            Undetermined,
        ),
        (
            None,
            vec![
                ("origin", "hub:repositories/dotfiles.git"),
                ("github", "git@github.com:P4suta/dotfiles.git"),
            ],
            Applies,
        ),
        (
            None,
            vec![
                ("origin", "hub:repositories/project.git"),
                ("upstream", "https://github.com/other/project.git"),
            ],
            Skips,
        ),
        (
            Some(true),
            vec![("origin", "hub:repositories/dotfiles.git")],
            Applies,
        ),
        (
            Some(false),
            vec![("origin", "hub:repositories/dotfiles.git")],
            Skips,
        ),
        (
            Some(false),
            vec![("origin", "git@github.com:P4suta/dotfiles.git")],
            Skips,
        ),
        (Some(true), vec![], Applies),
    ] {
        assert_eq!(
            prose::remotes_standard(declared, &remotes(&list), &personal, resolve),
            standard,
            "{declared:?} {list:?}"
        );
    }
}

#[test]
fn the_global_commit_msg_hook_checks_personal_repositories() -> Result<()> {
    let tools = tempfile::tempdir()?;
    let succeed = tools.path().join("succeed.rs");
    fs::write(&succeed, "fn main() {}\n")?;
    let home = tempfile::tempdir()?;
    let bin = home.path().join(".local/bin");
    fs::create_dir_all(&bin)?;
    let status = Command::new("rustc")
        .arg(&succeed)
        .arg("-o")
        .arg(bin.join(format!("dotguard{}", std::env::consts::EXE_SUFFIX)))
        .status()?;
    assert!(status.success());
    let remotes_commit =
        |remotes: &[(&str, &str)], declared: Option<&str>, text: &str| -> Result<(bool, String)> {
            let directory = repository(&[])?;
            for (name, url) in remotes {
                git(directory.path(), &["remote", "add", name, url])?;
            }
            if let Some(declared) = declared {
                git(directory.path(), &["config", "prose.standard", declared])?;
            }
            let message = directory.path().join(".git/COMMIT_EDITMSG");
            fs::write(&message, text)?;
            let output = Command::new(env!("CARGO_BIN_EXE_dotfiles-xtask"))
                .current_dir(directory.path())
                .args(["hook", "commit-msg", "--"])
                .arg(&message)
                .env(
                    if cfg!(windows) { "USERPROFILE" } else { "HOME" },
                    home.path(),
                )
                .env("PROSE_CONFIG", root().join("dot_config/prose"))
                .env("PROSE_RUNTIME", home.path().join("runtime"))
                .stdin(Stdio::null())
                .output()?;
            Ok((
                output.status.success(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        };
    let declared_commit =
        |origin: &str, upstream: Option<&str>, declared: Option<&str>, text: &str| {
            let mut remotes = vec![("origin", origin)];
            remotes.extend(upstream.map(|upstream| ("upstream", upstream)));
            remotes_commit(&remotes, declared, text)
        };
    let commit = |origin: &str, upstream: Option<&str>, text: &str| {
        declared_commit(origin, upstream, None, text)
    };
    let attributed = format!("fix(parser): keep edits\n\n{ATTRIBUTED}\n");
    let hub = "hub:repositories/project.git";
    let (accepted, stderr) = commit(hub, None, &attributed)?;
    assert!(accepted, "{stderr}");
    assert!(
        stderr.contains(hub) && stderr.contains("git config prose.standard true"),
        "an unresolved destination is reported with the setting that decides it: {stderr}"
    );
    let personal_remote = ("github", "git@github.com:P4suta/project.git");
    let (accepted, stderr) = remotes_commit(&[personal_remote], None, &attributed)?;
    assert!(
        !accepted && stderr.contains("Dotfiles.Attribution"),
        "a personal GitHub remote names the destination of a repository without origin: {stderr}"
    );
    let (accepted, stderr) = remotes_commit(&[], None, &attributed)?;
    assert!(accepted, "{stderr}");
    assert!(
        stderr.contains("no origin remote") && stderr.contains("git config prose.standard true"),
        "a repository without remotes reports the setting that decides it: {stderr}"
    );
    let (accepted, stderr) = remotes_commit(&[], Some("true"), &attributed)?;
    assert!(
        !accepted && stderr.contains("Dotfiles.Attribution"),
        "{stderr}"
    );
    let (accepted, stderr) = declared_commit(hub, None, Some("true"), &attributed)?;
    assert!(
        !accepted && stderr.contains("Dotfiles.Attribution"),
        "{stderr}"
    );
    let (accepted, stderr) = declared_commit(hub, None, Some("false"), &attributed)?;
    assert!(accepted && stderr.is_empty(), "{stderr}");
    let (accepted, stderr) = declared_commit(hub, None, Some("maybe"), &attributed)?;
    assert!(
        !accepted && stderr.contains("prose.standard"),
        "an invalid setting is refused: {stderr}"
    );
    let (accepted, stderr) = commit("git@github.com:P4suta/project.git", None, &attributed)?;
    assert!(!accepted);
    assert!(
        stderr.contains("Dotfiles.Attribution")
            && stderr.contains(ATTRIBUTED)
            && stderr.contains("commit again"),
        "{stderr}"
    );
    let compliant = format!("fix(parser): keep edits\n\n{COMPLIANT}\n");
    let (accepted, stderr) = commit("git@github.com:P4suta/project.git", None, &compliant)?;
    assert!(accepted, "{stderr}");
    for generated in [
        "Revert \"feat(prose): enforce one writing standard\"\n\nThis reverts commit 39f8a76.\n",
        "fixup! feat(prose): enforce one writing standard\n",
        "squash! feat(prose): enforce one writing standard\n",
    ] {
        let (accepted, stderr) = commit("git@github.com:P4suta/project.git", None, generated)?;
        assert!(accepted, "Git generated {generated:?}: {stderr}");
    }
    let (accepted, stderr) = commit("https://github.com/other/project.git", None, &attributed)?;
    assert!(
        accepted,
        "an external destination keeps its own rules: {stderr}"
    );
    let (accepted, stderr) = commit(
        "git@github.com:P4suta/project.git",
        Some("https://github.com/other/project.git"),
        &attributed,
    )?;
    assert!(accepted, "a fork keeps its upstream rules: {stderr}");
    Ok(())
}
