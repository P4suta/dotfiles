use anyhow::Result;
use prose::prose_rules::Standard;
use prose::{Bundle, Channel, Exempt, Finding, Policy, Scope};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[path = "../../dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

use fixture_git::fixture_process;

const ATTRIBUTED: &str = "As requested, the parser keeps every edit.";
const COMPLIANT: &str = "The parser keeps every edit.";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
    let mut child = fixture_process::command(env!("CARGO_BIN_EXE_prose"), runtime)
        .args(arguments)
        .env("PROSE_CONFIG", root().join("home/dot_config/prose"))
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
    assert_eq!(
        rules(&document(
            "The cache holds the index.\nThe parser reads it.\n"
        )?),
        Vec::<&str>::new()
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
        Vec::<usize>::new()
    );
    assert_eq!(
        prose::sentence_starts("Version 1.2. Then"),
        Vec::<usize>::new()
    );
    assert_eq!(prose::sentence_starts("Read it (twice). Then stop."), [18]);
    assert_eq!(prose::sentence_starts("読む。書く。"), [4]);
    assert_eq!(
        prose::sentence_starts("1. Install it."),
        Vec::<usize>::new()
    );
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
            {"line": 3, "column": 5, "end_line": 3, "kind": "block", "text": "/* chezmoi:modify-template */"},
            {"line": 4, "column": 1, "end_line": 4, "kind": "directive", "text": "// SAFETY-IGNORED: kept"},
            {"line": 4, "column": 30, "end_line": 4, "kind": "line", "text": "// shellcheck disable=SC2034"},
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
    assert_eq!(judge(vec![], vec![]), Vec::<String>::new());
    let exempt = |reason: &str| Exempt {
        scope: Scope::Documents,
        paths: vec!["vendor/".into()],
        rules: vec!["Google.Passive".into()],
        reason: reason.into(),
    };
    assert_eq!(
        judge(
            vec![finding("vendor/a.md", "Google.Passive", old)],
            vec![exempt("Upstream text.")]
        ),
        Vec::<String>::new()
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
    let config = copy.path().join("home/dot_config/prose");
    copy_tree(&root().join("home/dot_config/prose"), &config)?;
    let copied = Bundle::source(copy.path())?;
    copied.verify()?;
    fs::write(config.join("styles/Google/We.yml"), "extends: existence\n")?;
    let error = format!("{:#}", copied.verify().expect_err("tampered style"));
    assert!(
        error.contains("Google") && error.contains("install-prose"),
        "{error}"
    );
    fs::remove_file(config.join("packages.toml"))?;
    let error = format!("{:#}", copied.verify().expect_err("missing lock"));
    assert!(
        error.contains("missing") && error.contains("install-prose"),
        "{error}"
    );
    Ok(())
}

#[test]
fn the_source_style_packages_match_their_pinned_digests() -> Result<()> {
    let lock: toml::Value = toml::from_str(&fs::read_to_string(
        root().join("home/dot_config/prose/packages.toml"),
    )?)?;
    let packages = lock["package"].as_array().expect("pinned packages");
    for package in packages {
        let name = package["name"].as_str().expect("package name");
        let directory = root().join("home/dot_config/prose/styles").join(name);
        assert_eq!(
            prose::tree_digest(&directory)?,
            package["tree_sha256"].as_str().expect("tree digest"),
            "source {name}"
        );
    }
    Ok(())
}

#[test]
fn the_pinned_vale_matches_the_mise_pin() -> Result<()> {
    let mise: toml::Value = toml::from_str(&fs::read_to_string(root().join("mise.toml"))?)?;
    let lock: toml::Value = toml::from_str(&fs::read_to_string(
        root().join("home/dot_config/prose/packages.toml"),
    )?)?;
    assert_eq!(mise["tools"]["vale"].as_str(), lock["vale"].as_str());
    Ok(())
}

#[test]
fn hook_removal_is_idempotent_and_keeps_other_handlers() -> Result<()> {
    let settings = json!({"hooks": {"Stop": [
        {"hooks": [{"type": "command", "command": "skill-ops stop", "timeout": 10}]},
        {"matcher": "*", "hooks": [
            {"type": "command", "command": "prose reply --client claude", "timeout": 120},
            {"type": "prompt", "prompt": "Check the task."},
            {"type": "command", "command": "prose check --channel document guide.md"}
        ]},
        {"hooks": []},
        {"hooks": [{"type": "command", "command": "prose reply --client codex", "timeout": 5}]}
    ], "PreToolUse": [{"hooks": [{"type": "command", "command": "dotfiles hook claude"}]}]}, "theme": "auto"});
    let expected = json!({"hooks": {"Stop": [
        {"hooks": [{"type": "command", "command": "skill-ops stop", "timeout": 10}]},
        {"matcher": "*", "hooks": [
            {"type": "prompt", "prompt": "Check the task."},
            {"type": "command", "command": "prose check --channel document guide.md"}
        ]},
        {"hooks": []}
    ], "PreToolUse": [{"hooks": [{"type": "command", "command": "dotfiles hook claude"}]}]}, "theme": "auto"});
    let merged = prose::remove_reply_hooks(settings)?;
    assert_eq!(merged, expected);
    assert_eq!(prose::remove_reply_hooks(merged.clone())?, merged);
    Ok(())
}

#[test]
fn hook_removal_keeps_absent_events_and_refuses_malformed_settings() -> Result<()> {
    for settings in [
        json!({}),
        json!({"hooks": {}}),
        json!({"hooks": {"Stop": []}}),
    ] {
        assert_eq!(prose::remove_reply_hooks(settings.clone())?, settings);
    }
    for settings in [
        json!([]),
        json!({"hooks": []}),
        json!({"hooks": {"Stop": {}}}),
        json!({"hooks": {"Stop": [{}]}}),
        json!({"hooks": {"Stop": [{"hooks": {}}]}}),
    ] {
        assert!(prose::remove_reply_hooks(settings).is_err());
    }
    Ok(())
}

#[test]
fn apply_and_install_cleanup_remove_existing_reply_checks() -> Result<()> {
    let home = tempfile::tempdir()?;
    let expected = json!({"hooks": {"Stop": [
        {"hooks": [{"type": "command", "command": "skill-ops stop"}]}
    ]}, "theme": "auto"});
    for (client, relative) in [
        ("claude", ".claude/settings.json"),
        ("codex", ".codex/hooks.json"),
    ] {
        let path = home.path().join(relative);
        fs::create_dir_all(path.parent().expect("settings parent"))?;
        let settings = json!({"hooks": {"Stop": [
            {"hooks": [{"type": "command", "command": "skill-ops stop"}]},
            {"hooks": [{"type": "command", "command": format!("prose reply --client {client}"), "timeout": 120}]}
        ]}, "theme": "auto"});
        fs::write(&path, serde_json::to_vec(&settings)?)?;
    }
    let plugin = home.path().join(prose::REPLY_PLUGIN);
    fs::create_dir_all(plugin.parent().expect("plugin parent"))?;
    fs::write(&plugin, "retired reply plugin")?;
    let other_plugin = plugin.with_file_name("other.ts");
    fs::write(&other_plugin, "other plugin")?;
    for _ in 0..2 {
        prose::remove_reply_checks(home.path())?;
        assert!(!plugin.try_exists()?);
        assert_eq!(fs::read_to_string(&other_plugin)?, "other plugin");
        for relative in [".claude/settings.json", ".codex/hooks.json"] {
            assert_eq!(
                skill_ops::read_json::<Value>(&home.path().join(relative))?,
                expected
            );
        }
    }
    let untouched = "{\"theme\":\"auto\"}\n";
    let settings = home.path().join(".claude/settings.json");
    fs::write(&settings, untouched)?;
    prose::remove_reply_checks(home.path())?;
    assert_eq!(fs::read_to_string(&settings)?, untouched);
    let fresh = tempfile::tempdir()?;
    prose::remove_reply_checks(fresh.path())?;
    assert_eq!(fs::read_dir(fresh.path())?.count(), 0);
    Ok(())
}

#[test]
fn reply_cleanup_reports_a_plugin_that_cannot_be_removed() -> Result<()> {
    let home = tempfile::tempdir()?;
    let plugin = home.path().join(prose::REPLY_PLUGIN);
    fs::create_dir_all(&plugin)?;
    let error = format!(
        "{:#}",
        prose::remove_reply_checks(home.path()).expect_err("a directory at the plugin path")
    );
    assert!(error.contains("remove the retired reply plugin"));
    Ok(())
}

#[test]
fn the_cli_no_longer_accepts_replies() -> Result<()> {
    let runtime = tempfile::tempdir()?;
    for arguments in [
        vec!["reply", "--client", "claude"],
        vec!["check", "--channel", "reply"],
    ] {
        let (code, _, stderr) = binary(&arguments, "", runtime.path())?;
        assert_eq!(code, 2, "{stderr}");
    }
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
    let status = fixture_git::command("git", &directory.join(".fixture-gitconfig"))
        .current_dir(directory)
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(arguments)
        .stdout(Stdio::null())
        .status()?;
    assert!(status.success(), "git {arguments:?}");
    Ok(())
}

/// A repository holding the pinned configuration, its exemptions, and the given files.
fn repository(files: &[(&str, &str)]) -> Result<tempfile::TempDir> {
    let directory = tempfile::tempdir()?;
    copy_tree(
        &root().join("home/dot_config/prose"),
        &directory.path().join("home/dot_config/prose"),
    )?;
    fs::create_dir_all(directory.path().join("policy"))?;
    fs::write(
        directory.path().join("policy/prose.toml"),
        "[[exempt]]\nscope = \"documents\"\npaths = [\"home/dot_config/prose/styles/\"]\nreason = \"Vendored Vale packages keep their upstream text.\"\n\n[[exempt]]\nscope = \"comments\"\npaths = [\"home/dot_config/prose/\"]\nreason = \"The pinned configuration is data.\"\n",
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

/// The prose arguments of the hook's CLI or library entry point.
/// A recipe parameter becomes the argument lefthook passes in its place, such as `{1}`.
fn hook_arguments(hook: &str, command: &str) -> Result<Vec<String>> {
    let documents =
        yaml_rust2::YamlLoader::load_from_str(&fs::read_to_string(root().join("lefthook.yml"))?)?;
    let run = documents[0][hook]["commands"][command]["run"]
        .as_str()
        .expect("the hook command")
        .to_owned();
    let mut words = run
        .strip_prefix("mise x -- just ")
        .expect("the hook runs a just recipe")
        .split_whitespace();
    let recipe = words.next().expect("the recipe name");
    let passed: Vec<&str> = words.collect();
    let justfile = fs::read_to_string(root().join("justfile"))?;
    let mut lines = justfile.lines();
    let header = lines
        .by_ref()
        .filter(|line| !line.starts_with(' '))
        .filter_map(|line| line.split_once(':').map(|(signature, _)| signature))
        .find(|signature| signature.split_whitespace().next() == Some(recipe))
        .expect("the recipe the hook names");
    let parameters: Vec<&str> = header.split_whitespace().skip(1).collect();
    if recipe == "check-staged" {
        assert!(lines.any(|line| line.trim() == "mise x -- cargo xtask check --staged"));
        let source = fs::read_to_string(root().join("xtask/src/main.rs"))?;
        let check = source
            .split("fn check_staged(root: &Path)")
            .nth(1)
            .expect("staged checker");
        let check = check
            .split("\nfn check(")
            .next()
            .expect("staged checker body");
        assert!(check.contains("xtask::staged_snapshot::export(root)?"));
        assert!(check.contains("prose::Scope::Comments"));
        return Ok(["--source", ".", "repository", "--scope", "comments"]
            .map(str::to_owned)
            .into());
    }
    let body = lines
        .take_while(|line| line.starts_with("    "))
        .find(|line| line.contains(" --bin prose -- "))
        .expect("the recipe runs the prose binary")
        .trim();
    assert!(body.starts_with("mise x -- cargo run --locked --package prose --bin prose -- "));
    let (_, arguments) = body
        .split_once(" --bin prose -- ")
        .expect("prose arguments");
    Ok(arguments
        .split_whitespace()
        .map(|argument| {
            parameters
                .iter()
                .position(|parameter| argument == format!("{{{{quote({parameter})}}}}"))
                .map_or_else(|| argument.to_owned(), |index| passed[index].to_owned())
        })
        .collect())
}

/// The directory holding a stand-in `ocomment`, compiled once, because CI hosts don't install the owner's tool.
fn ocomment_directory() -> Result<&'static Path> {
    static DIRECTORY: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    if let Some(directory) = DIRECTORY.get() {
        return Ok(directory);
    }
    let directory = tempfile::tempdir()?.keep();
    let status = platform::process::command("rustc")
        .args(["--edition", "2024"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocomment.rs"))
        .arg("-o")
        .arg(directory.join(format!("ocomment{}", std::env::consts::EXE_SUFFIX)))
        .status()?;
    anyhow::ensure!(status.success(), "fixture ocomment compilation failed");
    Ok(DIRECTORY.get_or_init(|| directory))
}

fn run_in(directory: &Path, arguments: &[String]) -> Result<(i32, String)> {
    let mut paths = vec![ocomment_directory()?.to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let output = fixture_process::command(env!("CARGO_BIN_EXE_prose"), directory)
        .current_dir(directory)
        .env("PATH", std::env::join_paths(paths)?)
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
    let arguments = hook_arguments("pre-commit", "staged")?;
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

/// The declared `prose.standard`, the remotes, and the standard they decide.
type Destination = (
    Option<bool>,
    &'static [(&'static str, &'static str)],
    Standard,
);

const DESTINATIONS: &[Destination] = &[
    (
        None,
        &[("origin", "git@github.com:P4suta/dotfiles.git")],
        Standard::Applies,
    ),
    (
        None,
        &[("origin", "https://github.com/p4suta/dotfiles")],
        Standard::Applies,
    ),
    (
        None,
        &[("origin", "ssh://git@github.com/P4suta/dotfiles.git")],
        Standard::Applies,
    ),
    (
        None,
        &[("origin", "ssh://git@github.com:22/P4suta/dotfiles.git")],
        Standard::Applies,
    ),
    (
        None,
        &[("origin", "gh-personal:P4suta/dotfiles.git")],
        Standard::Applies,
    ),
    (
        None,
        &[("origin", "ssh://gh-personal/P4suta/dotfiles.git")],
        Standard::Applies,
    ),
    (
        None,
        &[("origin", "https://github.com/other/project.git")],
        Standard::Skips,
    ),
    (
        None,
        &[("origin", "gh-personal:other/project.git")],
        Standard::Skips,
    ),
    (
        None,
        &[
            ("origin", "git@github.com:P4suta/project.git"),
            ("upstream", "https://github.com/other/project.git"),
        ],
        Standard::Skips,
    ),
    (
        None,
        &[
            ("origin", "git@github.com:P4suta/project.git"),
            ("backup", "git@github.com:P4suta/project-backup.git"),
        ],
        Standard::Applies,
    ),
    (
        None,
        &[("upstream", "git@github.com:P4suta/project.git")],
        Standard::Applies,
    ),
    (
        None,
        &[("github", "git@github.com:P4suta/project.git")],
        Standard::Applies,
    ),
    (
        None,
        &[
            ("github", "git@github.com:P4suta/project.git"),
            ("upstream", "https://github.com/other/project.git"),
        ],
        Standard::Skips,
    ),
    (
        None,
        &[("upstream", "https://github.com/other/project.git")],
        Standard::Skips,
    ),
    (
        None,
        &[("backup", "hub:repositories/project.git")],
        Standard::Undetermined,
    ),
    (None, &[], Standard::Undetermined),
    (Some(false), &[], Standard::Skips),
    (
        None,
        &[("origin", "hub:repositories/dotfiles.git")],
        Standard::Undetermined,
    ),
    (
        None,
        &[("origin", "/srv/hub/dotfiles.git")],
        Standard::Undetermined,
    ),
    (
        None,
        &[("origin", "git@gitlab.com:P4suta/x.git")],
        Standard::Undetermined,
    ),
    (
        None,
        &[
            ("origin", "hub:repositories/dotfiles.git"),
            ("github", "git@github.com:P4suta/dotfiles.git"),
        ],
        Standard::Applies,
    ),
    (
        None,
        &[
            ("origin", "hub:repositories/project.git"),
            ("upstream", "https://github.com/other/project.git"),
        ],
        Standard::Skips,
    ),
    (
        Some(true),
        &[("origin", "hub:repositories/dotfiles.git")],
        Standard::Applies,
    ),
    (
        Some(false),
        &[("origin", "hub:repositories/dotfiles.git")],
        Standard::Skips,
    ),
    (
        Some(false),
        &[("origin", "git@github.com:P4suta/dotfiles.git")],
        Standard::Skips,
    ),
    (Some(true), &[], Standard::Applies),
];

#[test]
fn the_destination_decides_which_repositories_take_the_standard() {
    let personal = ["P4suta".to_owned()];
    // OpenSSH resolves the `gh-personal` host entry to github.com.
    let resolve = |host: &str| (host == "gh-personal").then(|| "github.com".to_owned());
    for &(declared, list, standard) in DESTINATIONS {
        let remotes: Vec<(String, String)> = list
            .iter()
            .map(|(name, url)| ((*name).to_owned(), (*url).to_owned()))
            .collect();
        assert_eq!(
            prose::remotes_standard(declared, &remotes, &personal, resolve),
            standard,
            "{declared:?} {list:?}"
        );
    }
}
