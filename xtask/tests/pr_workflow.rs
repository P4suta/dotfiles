#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;

fn gh_directory() -> &'static tempfile::TempDir {
    static DIRECTORY: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIRECTORY.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        for program in ["gh", "gh-stack"] {
            let result = Command::new("rustc")
                .arg(format!(
                    "{}/tests/fixtures/{program}.rs",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .arg("-o")
                .arg(
                    directory
                        .path()
                        .join(format!("{program}{}", std::env::consts::EXE_SUFFIX)),
                )
                .status()
                .unwrap();
            assert!(result.success());
        }
        directory
    })
}

struct Fixture {
    directory: tempfile::TempDir,
    body: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new(body: &str) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let file = directory.path().join("body.md");
        fs::write(&file, body)?;
        let log = directory.path().join("gh.log");
        Ok(Self {
            directory,
            body: file,
            log,
        })
    }

    fn command(&self, operation: &str) -> Result<Command> {
        self.command_for_repo(operation, "owner/project")
    }

    fn command_for_repo(&self, operation: &str, repo: &str) -> Result<Command> {
        let mut paths = vec![gh_directory().path().to_path_buf()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").context("PATH")?,
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_pr-workflow"));
        command
            .current_dir(self.directory.path())
            .env("PATH", std::env::join_paths(paths)?)
            .env("HOME", self.directory.path())
            .env("USERPROFILE", self.directory.path())
            .env("GH_FIXTURE_LOG", &self.log)
            .env(
                "GH_STACK_FIXTURE_LOG",
                self.directory.path().join("gh-stack.log"),
            )
            .env_remove("DOTGUARD_STACK")
            .env("GH_FIXTURE_USER", r#"{"login":"P4suta","id":42543015}"#)
            .env("GH_FIXTURE_REPOSITORY", serde_json::json!({
                "full_name":repo,"owner":{"login":repo.split('/').next().unwrap(),"id":1},"fork":false
            }).to_string())
            .args(operation.split_whitespace())
            .args(["--repo", repo]);
        Ok(command)
    }

    fn personal_command(&self, operation: &str) -> Result<Command> {
        let mut command = self.command_for_repo(operation, "P4suta/project")?;
        command
            .env("GH_FIXTURE_REPOSITORY", serde_json::json!({
                "full_name":"P4suta/project","owner":{"login":"P4suta","id":42543015},"fork":false
            }).to_string())
            .env("GH_FIXTURE_ISSUE", serde_json::json!({
                "number":23,"title":"Preserve edits","body":"The original input must survive edits.",
                "state":"open","html_url":"https://github.com/P4suta/project/issues/23"
            }).to_string());
        Ok(command)
    }

    fn document(&self, operation: &str, title: &str) -> Result<Output> {
        Ok(self
            .command(operation)?
            .args(["--title", title, "--body-file"])
            .arg(&self.body)
            .args(if operation == "create" {
                vec!["--head", "feature"]
            } else if operation == "edit" {
                vec!["--pr", "17"]
            } else {
                vec![]
            })
            .output()?)
    }

    fn pause(&self, marker: &str) -> Result<()> {
        let state = self.directory.path().join(".local/state/coderabbit-guard");
        fs::create_dir_all(&state)?;
        fs::write(state.join(marker), "owner pause")?;
        Ok(())
    }

    fn log(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.log)?)
    }
}

/// The refusal record a failed run ends with, checked as plain JSON so the format stands on its own.
fn refusal(output: &Output) -> serde_json::Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().last().unwrap_or_default();
    let json = line
        .strip_prefix("dotfiles-refusal/1 ")
        .unwrap_or_else(|| panic!("no refusal record: {stderr}"));
    let record: serde_json::Value = serde_json::from_str(json).unwrap();
    let object = record.as_object().unwrap();
    assert_eq!(
        object.keys().map(String::as_str).collect::<Vec<_>>(),
        ["cause", "evidence", "next", "rule", "waiver"],
        "{line}"
    );
    for key in ["rule", "cause", "next"] {
        assert!(
            record[key]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty()),
            "{key}: {line}"
        );
    }
    assert!(!record["next"].as_str().unwrap().contains('\n'), "{line}");
    let evidence = record["evidence"].as_array().unwrap();
    assert!(
        !evidence.is_empty()
            && evidence
                .iter()
                .all(|item| item.as_str().is_some_and(|text| !text.trim().is_empty())),
        "{line}"
    );
    assert!(
        record["waiver"].is_null(),
        "pr-workflow has no waiver: {line}"
    );
    assert!(
        dotfiles_xtask::refusal::Refusal::find(&stderr).is_some_and(|parsed| parsed.is_complete()),
        "{line}"
    );
    record
}

#[test]
fn every_refusal_ends_with_one_complete_record_naming_its_rule() -> Result<()> {
    let rule = |output: Output| refusal(&output)["rule"].as_str().unwrap().to_owned();
    let closed = format!("{BODY}\nCloses #23.\n");

    let fixture = Fixture::new(BODY)?;
    assert_eq!(rule(fixture.document("check", "Repair edits")?), "pr.title");
    let fixture = Fixture::new(" \n")?;
    let output = fixture.document("check", "fix: preserve edits")?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.body");
    assert!(
        record["next"]
            .as_str()
            .unwrap()
            .starts_with("pr-workflow check --repo owner/project --title 'fix: preserve edits'"),
        "{record}"
    );

    let fixture = Fixture::new(&closed)?;
    assert_eq!(
        rule(fixture.personal_command("start")?.output()?),
        "pr.issue"
    );
    let fixture = Fixture::new(BODY)?;
    let output = fixture
        .personal_command("create")?
        .args([
            "--issue",
            "23",
            "--title",
            "fix: preserve edits",
            "--body-file",
        ])
        .arg(&fixture.body)
        .args(["--head", "feature"])
        .output()?;
    assert_eq!(rule(output), "pr.body");

    let fixture = Fixture::new(&closed)?;
    let output = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .env(
            "GH_FIXTURE_HEADERS",
            "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 9\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
        )
        .output()?;
    assert_eq!(rule(output), "pr.quota");
    let output = fixture
        .personal_command("start")?
        .args(["--issue", "23"])
        .env("GH_FIXTURE_FAILURE", "user")
        .output()?;
    assert_eq!(rule(output), "pr.auth");

    for (state, draft) in [("CLOSED", true), ("OPEN", false)] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":state,"isDraft":draft});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?;
        assert_eq!(rule(output), "pr.state");
    }
    let fixture = Fixture::new(BODY)?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true,
        "statusCheckRollup":[{"status":"COMPLETED","conclusion":"FAILURE"}]});
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.checks");
    assert_eq!(record["next"], "gh pr checks 17 --repo owner/project");

    let output = fixture.command_for_repo("start", "owner")?.output()?;
    assert_eq!(rule(output), "pr.repository");
    let output = fixture
        .command("start")?
        .args(["--title-policy", "missing.json"])
        .output()?;
    assert_eq!(rule(output), "pr.title-policy");
    let output = fixture
        .command("start")?
        .env("GH_FIXTURE_USER", r#"{"login":"P4suta","id":0}"#)
        .output()?;
    assert_eq!(rule(output), "pr.auth");
    let output = fixture
        .personal_command("start")?
        .env(
            "GH_FIXTURE_REPOSITORY",
            serde_json::json!({
                "full_name":"P4suta/project","owner":{"login":"P4suta","id":1},"fork":false
            })
            .to_string(),
        )
        .output()?;
    assert_eq!(rule(output), "pr.policy");

    let fixture = Fixture::new(BODY)?;
    let output = fixture
        .command("create")?
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "a b"])
        .output()?;
    assert_eq!(rule(output), "pr.branch");
    let output = fixture
        .command("create")?
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature"])
        .env("GH_FIXTURE_FAILURE", "create")
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.failed");
    assert_eq!(record["next"], "pr-workflow create --help");
    Ok(())
}

const PAUSE_MARKER: &str = "<!-- coderabbit-pause -->";

const BODY: &str = "## Why\nAvoid losing edits.\n\n## Changes\nPreserve the original input.\n\n## Validation\nThe targeted regression test passed.\n";

#[test]
fn personal_publication_requires_a_real_open_issue_and_visible_reference() -> Result<()> {
    for operation in ["create", "edit", "ready"] {
        for (issue_argument, issue, body, accepted) in [
            (false, None, format!("{BODY}\nCloses #23.\n"), false),
            (true, None, BODY.to_owned(), false),
            (true, None, format!("{BODY}\n<!-- Closes #23. -->\n"), false),
            (true, None, format!("{BODY}\n`Closes #23.`\n"), false),
            (
                true,
                None,
                format!("{BODY}\n```text\nCloses #23.\n```\n"),
                false,
            ),
            (true, None, format!("{BODY}\nCloses #230.\n"), false),
            (
                true,
                None,
                format!("{BODY}\n- Not run: the stopped engine.\n\nCloses #23.\n"),
                true,
            ),
            (
                true,
                None,
                format!(
                    "{BODY}\n| Check | Result |\n| --- | --- |\n| proofs | passed |\n\nCloses #23.\n"
                ),
                true,
            ),
            (
                true,
                None,
                format!("{BODY}\nCloses https://github.com/other/project/issues/23.\n"),
                false,
            ),
            (
                true,
                Some(
                    r#"{"number":23,"title":"Preserve edits","body":"Input must survive.","state":"closed","html_url":"https://github.com/P4suta/project/issues/23"}"#,
                ),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (
                true,
                Some(
                    r#"{"number":24,"title":"Preserve edits","body":"Input must survive.","state":"open","html_url":"https://github.com/P4suta/project/issues/24"}"#,
                ),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (
                true,
                Some(
                    r#"{"number":23,"title":"Preserve edits","body":null,"state":"open","html_url":"https://github.com/P4suta/project/issues/23"}"#,
                ),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (
                true,
                Some(
                    r#"{"number":23,"title":"Preserve edits","body":"<!-- instructions -->","state":"open","html_url":"https://github.com/P4suta/project/issues/23"}"#,
                ),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (
                true,
                Some(
                    r#"{"number":23,"title":"Preserve edits","body":"TODO: describe intent","state":"open","html_url":"https://github.com/P4suta/project/issues/23"}"#,
                ),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (
                true,
                Some(
                    r#"{"number":23,"title":"Preserve edits","body":"Input must survive.","state":"open","html_url":"https://github.com/other/project/issues/23"}"#,
                ),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (
                true,
                Some(
                    r#"{"number":23,"title":"Preserve edits","body":"Input must survive.","state":"open","html_url":"https://github.com/P4suta/project/issues/23","pull_request":{}}"#,
                ),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (
                true,
                Some("{truncated"),
                format!("{BODY}\nCloses #23.\n"),
                false,
            ),
            (true, None, format!("{BODY}\nCloses #23.\n"), true),
            (
                true,
                None,
                format!(
                    "{BODY}\nFixes [the issue](https://github.com/P4suta/project/issues/23).\n"
                ),
                true,
            ),
        ] {
            let fixture = Fixture::new(&body)?;
            let mut command = fixture.personal_command(operation)?;
            if operation == "ready" {
                command.args(["--pr", "17"]).env(
                    "GH_FIXTURE_VIEW",
                    serde_json::json!({
                        "title":"fix: preserve edits","body":body,"state":"OPEN","isDraft":true
                    })
                    .to_string(),
                );
            } else {
                command
                    .args(["--title", "fix: preserve edits", "--body-file"])
                    .arg(&fixture.body);
                command.args(if operation == "create" {
                    ["--head", "feature"]
                } else {
                    ["--pr", "17"]
                });
            }
            if issue_argument {
                command.args(["--issue", "23"]);
            }
            if let Some(issue) = issue {
                command.env("GH_FIXTURE_ISSUE", issue);
            }
            let output = command.output()?;
            assert_eq!(
                output.status.success(),
                accepted,
                "{operation}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let log = if fixture.log.try_exists()? {
                fixture.log()?
            } else {
                String::new()
            };
            assert_eq!(log.contains(&format!("\n{operation}\n")), accepted, "{log}");
            if accepted && operation == "create" {
                assert!(log.contains("--draft"));
            }
        }
    }
    Ok(())
}

#[test]
fn global_owner_identity_and_forks_preserve_upstream_rules() -> Result<()> {
    for (repository, accepted) in [
        (serde_json::json!({"full_name":"P4suta/project","owner":{"login":"P4suta","id":42543015},"fork":true}).to_string(), true),
        (serde_json::json!({"full_name":"P4suta/project","owner":{"login":"P4suta","id":1},"fork":false}).to_string(), false),
        (serde_json::json!({"full_name":"other/project","owner":{"login":"other","id":1},"fork":false}).to_string(), false),
        (serde_json::json!({"full_name":"P4suta/project","owner":{"login":"other","id":42543015},"fork":false}).to_string(), false),
        (serde_json::json!({"full_name":"P4suta/project","owner":{"login":"P4suta","id":42543015},"fork":null}).to_string(), false),
        ("{truncated".into(), false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let output = fixture.personal_command("create")?
            .env("GH_FIXTURE_REPOSITORY", repository)
            .args(["--title","fix: preserve edits","--body-file"]).arg(&fixture.body)
            .args(["--head","feature"]).output()?;
        assert_eq!(output.status.success(), accepted, "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(fixture.log()?.contains("\ncreate\n"), accepted);
    }
    Ok(())
}

#[test]
fn preparation_is_read_only_and_coderabbit_cannot_skip_issue_prerequisites() -> Result<()> {
    let fixture = Fixture::new("Closes #23.\n@coderabbitai summary\n")?;
    for issue in [false, true] {
        let mut command = fixture.personal_command("start")?;
        if issue {
            command.args(["--issue", "23"]);
        }
        let output = command.output()?;
        assert_eq!(
            output.status.success(),
            issue,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!fixture.log()?.contains("\ncreate\n"));
    let output = fixture
        .personal_command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args([
            "--head",
            "feature",
            "--generation",
            "coderabbit",
            "--issue",
            "23",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.log()?.contains("--draft"));
    let fixture = Fixture::new("@coderabbitai summary\n")?;
    let output = fixture
        .personal_command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args([
            "--head",
            "feature",
            "--generation",
            "coderabbit",
            "--issue",
            "23",
        ])
        .output()?;
    assert!(!output.status.success());
    assert!(!fixture.log()?.contains("\ncreate\n"));
    Ok(())
}

#[test]
fn authentication_quota_and_api_failures_stop_without_retry_or_publication() -> Result<()> {
    for operation in ["start", "create", "edit", "ready", "check"] {
        for (variable, value) in [
            ("GH_FIXTURE_USER", "{truncated"),
            ("GH_FIXTURE_USER", r#"{"login":"","id":42543015}"#),
            ("GH_FIXTURE_USER", r#"{"login":"P4suta","id":0}"#),
            ("GH_FIXTURE_FAILURE", "user"),
            ("GH_FIXTURE_FAILURE", "repos/P4suta/project"),
            ("GH_FIXTURE_FAILURE", "repos/P4suta/project/issues/23"),
            (
                "GH_FIXTURE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 9\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
            (
                "GH_FIXTURE_ISSUE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 0\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
            ("GH_FIXTURE_HEADERS", "HTTP/2.0 200 OK"),
            (
                "GH_FIXTURE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: nope\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
            (
                "GH_FIXTURE_HEADERS",
                "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 4990\nX-Ratelimit-Remaining: 0\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core",
            ),
        ] {
            let fixture = Fixture::new(&format!("{BODY}\nCloses #23.\n"))?;
            let mut command = fixture.personal_command(operation)?;
            command.args(["--issue", "23"]).env(variable, value);
            match operation {
                "start" => {}
                "ready" | "check" => {
                    command.args(["--pr", "17"]).env("GH_FIXTURE_VIEW", serde_json::json!({
                        "title":"fix: preserve edits","body":fs::read_to_string(&fixture.body)?,"state":"OPEN","isDraft":true
                    }).to_string());
                }
                _ => {
                    command
                        .args(["--title", "fix: preserve edits", "--body-file"])
                        .arg(&fixture.body);
                    command.args(if operation == "create" {
                        ["--head", "feature"]
                    } else {
                        ["--pr", "17"]
                    });
                }
            }
            let output = command.output()?;
            assert!(!output.status.success(), "{operation}: {variable}");
            refusal(&output);
            let log = fixture.log()?;
            for mutation in ["create", "edit", "ready"] {
                assert!(!log.contains(&format!("\n{mutation}\n")), "{log}");
            }
            assert_eq!(
                log.matches("\nuser\n").count(),
                1,
                "authentication must not retry: {log}"
            );
            assert!(
                log.matches("invocation\n").count() <= 4,
                "bounded reads: {log}"
            );
        }
    }
    let fixture = Fixture::new(BODY)?;
    let output = fixture.document("check", "fix: preserve edits")?;
    assert!(output.status.success());
    assert!(!fixture.log.try_exists()?);
    let fixture = Fixture::new(&format!("{BODY}\nCloses #23.\n"))?;
    let output = fixture.personal_command("create")?
        .env("GH_HOST", "enterprise.example.org")
        .env("GH_FIXTURE_HEADERS", "HTTP/2.0 200 OK\r\nx-ratelimit-remaining: 10\r\nx-ratelimit-reset: 1800000000\r\nx-ratelimit-resource: core")
        .args(["--issue","23","--title","fix: preserve edits","--body-file"]).arg(&fixture.body)
        .args(["--head","feature"]).output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixture.log()?.matches("\nuser\n").count(), 1);
    Ok(())
}

#[test]
fn invalid_documents_never_start_gh_and_check_has_no_effects() -> Result<()> {
    for (title, body) in [
        ("Repair edits", BODY),
        ("fix(): preserve edits", BODY),
        ("fix: preserve edits\nextra", BODY),
        ("fix: preserve edits", " \n"),
        ("fix: preserve edits", "<!-- Explain your change. -->"),
        ("fix: preserve edits", "## Changes\nTBD\n"),
        ("fix: TODO", BODY),
        ("fix: preserve edits", "Describe your changes here."),
        ("@coderabbitai", "@coderabbitai summary"),
        (
            "fix: preserve edits",
            "Real content.\n<!-- @coderabbitai summary -->",
        ),
        ("feat: <description>", BODY),
        ("feat: {{title}}", BODY),
        ("fix: preserve edits", "## Changes\n{{ description }}\n"),
        ("fix: preserve edits", "## Changes\n[TODO]\n"),
        (
            "fix: preserve edits",
            "## Changes\nTODO: describe the change\n",
        ),
    ] {
        let fixture = Fixture::new(body)?;
        for action in ["check", "create", "edit"] {
            let output = fixture.document(action, title)?;
            assert!(!output.status.success(), "{action}: {title}");
            refusal(&output);
            assert!(!fixture.log.try_exists()?);
        }
    }
    for title in [
        "fix: preserve edits",
        "feat(storage)!: replace the file format",
        "refactor(ui): simplify navigation",
        "custom: add a new task",
    ] {
        let fixture = Fixture::new(BODY)?;
        assert!(fixture.document("check", title)?.status.success());
        assert!(!fixture.log.try_exists()?);
    }
    Ok(())
}

#[test]
fn completed_markdown_and_literal_marker_examples_are_accepted() -> Result<()> {
    let body = "## Changes\nSort the todo list and remove obsolete FIXME comments.\n\n<details>\n<summary>Validation results</summary>\nThe regression passed.\n</details>\n\n```yaml\nvalue: ${{ secrets.X }}\nTODO\n{{description}}\n```\n\n~~~text\n[insert description]\n~~~\n\nAn inline example uses `{{title}}` and `TBD`.\nThe Actions expression is ${{ secrets.X }}.\n";
    let fixture = Fixture::new(body)?;
    for operation in ["check", "create", "edit"] {
        let output = fixture.document(operation, "feat: add todo list sorting")?;
        assert!(
            output.status.success(),
            "{operation}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let view = serde_json::json!({"title":"feat: add todo list sorting","body":body,"state":"OPEN","isDraft":true});
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("\nready\n"));
    Ok(())
}

#[test]
fn review_exclusion_is_published_without_allowing_unfinished_generation() -> Result<()> {
    let body = format!("{BODY}\n@coderabbitai ignore\n");
    let fixture = Fixture::new(&body)?;
    for operation in ["check", "create", "edit"] {
        let output = fixture.document(operation, "fix: preserve the review pause")?;
        assert!(
            output.status.success(),
            "{operation}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let view = serde_json::json!({"title":"fix: preserve the review pause","body":body,"state":"OPEN","isDraft":true});
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    assert!(output.status.success());
    assert!(fixture.log()?.contains("@coderabbitai ignore"));
    for body in [
        "@coderabbitai ignore\n".to_owned(),
        "<!-- Explain the change. -->\n@coderabbitai ignore\n".to_owned(),
        format!("{BODY}\n@coderabbitai ignore\n@coderabbitai summary\n"),
        format!("{BODY}\n@coderabbitai ignore extra\n"),
        format!("{BODY}\n@coderabbitai review\n"),
    ] {
        let rejected = Fixture::new(&body)?;
        for operation in ["check", "create", "edit"] {
            assert!(
                !rejected
                    .document(operation, "fix: preserve the review pause")?
                    .status
                    .success()
            );
            assert!(!rejected.log.try_exists()?);
        }
    }
    Ok(())
}

#[test]
fn paused_pr_reviews_are_neither_requested_nor_awaited() -> Result<()> {
    let excluded = format!("{BODY}\n@coderabbitai ignore\n");
    let pause_excluded = format!("{excluded}{PAUSE_MARKER}\n");
    for (marker, paused) in [
        (None, false),
        (Some("paused"), true),
        (Some("paused-pr"), true),
        (Some("paused-cli"), false),
    ] {
        let fixture =
            Fixture::new("@coderabbitai summary\n\n## Validation\nThe regression passed.\n")?;
        if let Some(marker) = marker {
            fixture.pause(marker)?;
        }
        for operation in ["create", "edit"] {
            let mut command = fixture.command(operation)?;
            command
                .args(["--title", "@coderabbitai", "--body-file"])
                .arg(&fixture.body)
                .args(["--generation", "coderabbit"]);
            if operation == "create" {
                command.args(["--head", "feature", "--draft"]);
            } else {
                command.args(["--pr", "17"]);
            }
            let output = command.output()?;
            assert_eq!(output.status.success(), !paused, "{marker:?} {operation}");
            if paused {
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(
                    error.contains("PR reviews are paused") && error.contains("--generation local"),
                    "{error}"
                );
                assert!(!fixture.log.try_exists()?);
            }
        }
        for (body, generation, accepted) in [
            (BODY.to_owned(), "local", !paused),
            (excluded.clone(), "local", !paused),
            (pause_excluded.clone(), "local", true),
            ("@coderabbitai summary\n".to_owned(), "coderabbit", !paused),
        ] {
            let fixture = Fixture::new(BODY)?;
            if let Some(marker) = marker {
                fixture.pause(marker)?;
            }
            let title = if generation == "local" {
                "fix: preserve edits"
            } else {
                "@coderabbitai"
            };
            let view = serde_json::json!({"title":title,"body":body,"state":"OPEN","isDraft":true});
            let output = fixture
                .command("ready")?
                .args(["--pr", "17", "--generation", generation])
                .env("GH_FIXTURE_VIEW", view.to_string())
                .output()?;
            assert_eq!(
                output.status.success(),
                accepted,
                "{marker:?} {generation}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fixture.log()?.contains("\nready\n"), accepted);
            if !accepted {
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(error.contains("PR reviews are paused"), "{error}");
                if generation == "local" {
                    assert!(error.contains(PAUSE_MARKER), "{error}");
                }
            }
        }
        for (body, expected) in [
            (
                BODY.to_owned(),
                if paused {
                    "not required while the owner pauses PR reviews"
                } else {
                    "required for the current head"
                },
            ),
            (
                excluded.clone(),
                if paused {
                    "not required while the owner pauses PR reviews"
                } else {
                    "excluded by the body"
                },
            ),
            (
                pause_excluded.clone(),
                if paused {
                    "not required while the owner pauses PR reviews"
                } else {
                    "required again now that PR reviews have resumed"
                },
            ),
        ] {
            let fixture = Fixture::new(BODY)?;
            if let Some(marker) = marker {
                fixture.pause(marker)?;
            }
            let view = serde_json::json!({"title":"fix: preserve edits","body":body,"state":"OPEN","isDraft":false});
            let output = fixture
                .command("check")?
                .args(["--pr", "17", "--final"])
                .env("GH_FIXTURE_VIEW", view.to_string())
                .output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&output.stdout).contains(expected),
                "{marker:?}"
            );
        }
    }
    let fixture = Fixture::new(BODY)?;
    fixture.pause("paused-pr")?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    let ready = |fixture: &Fixture| -> Result<Output> {
        Ok(fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?)
    };
    let output = ready(&fixture)?;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("@coderabbitai ignore") && error.contains(PAUSE_MARKER),
        "{error}"
    );
    fs::remove_file(
        fixture
            .directory
            .path()
            .join(".local/state/coderabbit-guard/paused-pr"),
    )?;
    let output = ready(&fixture)?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("required for the current head"));
    Ok(())
}

#[test]
fn a_pr_readied_during_the_pause_owes_its_review_after_resumption() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    fixture.pause("paused-pr")?;
    let body = format!("{BODY}\n@coderabbitai ignore\n{PAUSE_MARKER}\n");
    let view = |draft: bool| {
        serde_json::json!({"title":"fix: preserve edits","body":body,"state":"OPEN","isDraft":draft})
            .to_string()
    };
    let output = fixture
        .command("ready")?
        .args(["--pr", "17"])
        .env("GH_FIXTURE_VIEW", view(true))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("not required while the owner pauses")
    );
    fs::remove_file(
        fixture
            .directory
            .path()
            .join(".local/state/coderabbit-guard/paused-pr"),
    )?;
    let output = fixture
        .command("check")?
        .args(["--pr", "17", "--final"])
        .env("GH_FIXTURE_VIEW", view(false))
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("required again now that PR reviews have resumed")
            && stdout.contains("@coderabbitai review"),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn an_edit_cannot_restore_review_of_a_ready_pr_during_the_pause() -> Result<()> {
    let excluded = format!("{BODY}\n@coderabbitai ignore\n");
    let pause_excluded = format!("{excluded}{PAUSE_MARKER}\n");
    for (marker, draft, live, edited, accepted) in [
        (
            Some("paused-pr"),
            false,
            pause_excluded.as_str(),
            excluded.as_str(),
            false,
        ),
        (
            Some("paused-pr"),
            true,
            pause_excluded.as_str(),
            excluded.as_str(),
            true,
        ),
        (
            None,
            false,
            pause_excluded.as_str(),
            excluded.as_str(),
            true,
        ),
        (
            Some("paused-pr"),
            false,
            excluded.as_str(),
            pause_excluded.as_str(),
            true,
        ),
        (
            Some("paused-pr"),
            false,
            pause_excluded.as_str(),
            pause_excluded.as_str(),
            true,
        ),
        (Some("paused-pr"), false, excluded.as_str(), BODY, false),
        (Some("paused"), false, excluded.as_str(), BODY, false),
        (Some("paused-pr"), true, excluded.as_str(), BODY, true),
        (
            Some("paused-pr"),
            false,
            excluded.as_str(),
            excluded.as_str(),
            true,
        ),
        (Some("paused-pr"), false, BODY, BODY, true),
        (Some("paused-cli"), false, excluded.as_str(), BODY, true),
        (None, false, excluded.as_str(), BODY, true),
    ] {
        let fixture = Fixture::new(edited)?;
        if let Some(marker) = marker {
            fixture.pause(marker)?;
        }
        let view = serde_json::json!({"title":"fix: preserve edits","body":live,"state":"OPEN","isDraft":draft});
        let output = fixture
            .command("edit")?
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?;
        let error = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.success(),
            accepted,
            "{marker:?} {draft} {error}"
        );
        assert_eq!(
            fs::read_to_string(&fixture.log)
                .unwrap_or_default()
                .contains("\nedit\n"),
            accepted
        );
        if !accepted {
            assert!(
                error.contains("PR reviews are paused") && error.contains("keep the standalone"),
                "{error}"
            );
        }
    }
    Ok(())
}

#[test]
fn local_create_is_draft_and_passes_the_checked_body_by_file() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let output = fixture.document("create", "fix: preserve `$(literal)` edits")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fixture.log()?;
    assert!(log.contains("\n--draft\n"));
    assert!(log.contains("fix: preserve `$(literal)` edits"));
    assert!(log.contains(BODY));
    assert!(!log.contains(fixture.body.to_str().unwrap()));
    Ok(())
}

#[test]
fn coderabbit_request_is_explicit_and_final_checks_refuse_pending_generation() -> Result<()> {
    let fixture = Fixture::new("@coderabbitai summary\n\n## Validation\nThe regression passed.\n")?;
    let output = fixture
        .command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature", "--generation", "coderabbit"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.log()?.contains("--draft"));
    assert!(!fixture.document("check", "@coderabbitai")?.status.success());
    let final_check = fixture
        .command("check")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args(["--generation", "coderabbit", "--final"])
        .output()?;
    assert!(!final_check.status.success());
    fs::write(&fixture.body, "@coderabbitai summary\nTODO\n")?;
    let output = fixture
        .command("create")?
        .args(["--title", "@coderabbitai", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature", "--generation", "coderabbit"])
        .output()?;
    assert!(!output.status.success());
    Ok(())
}

#[test]
fn external_title_policy_requires_matching_repository_and_source() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let policy = fixture.directory.path().join("policy.json");
    for value in [
        r#"{"repository":"other/project","source":"https://example.org/contributing","reason":"Upstream requires a sentence."}"#,
        r#"{"repository":"owner/project","source":"","reason":"Upstream requires a sentence."}"#,
    ] {
        fs::write(&policy, value)?;
        let output = fixture
            .command("create")?
            .args(["--title", "Preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(["--head", "feature", "--title-policy"])
            .arg(&policy)
            .output()?;
        assert!(!output.status.success());
        assert!(!fixture.log.try_exists()?);
    }
    fs::write(
        &policy,
        r#"{"repository":"owner/project","source":"https://example.org/contributing","reason":"Upstream requires a sentence."}"#,
    )?;
    let output = fixture
        .command("create")?
        .args(["--title", "Preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature", "--title-policy"])
        .arg(&policy)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.log()?.contains("--draft"));
    Ok(())
}

#[test]
fn ready_checks_live_contents_and_draft_state_before_mutation() -> Result<()> {
    for (title, body, state, draft, accepted) in [
        ("fix: preserve edits", BODY, "OPEN", true, true),
        ("Repair edits", BODY, "OPEN", true, false),
        (
            "fix: preserve edits",
            "<!-- instructions -->",
            "OPEN",
            true,
            false,
        ),
        (
            "@coderabbitai",
            "@coderabbitai summary",
            "OPEN",
            true,
            false,
        ),
        ("fix: preserve edits", BODY, "CLOSED", true, false),
        ("fix: preserve edits", BODY, "OPEN", false, false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":title,"body":body,"state":state,"isDraft":draft});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?;
        assert_eq!(
            output.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fixture.log()?.contains("\nready\n"), accepted);
    }
    for (checks, accepted) in [
        (
            r#"[{"status":"COMPLETED","conclusion":"SUCCESS"},{"state":"SUCCESS"}]"#,
            true,
        ),
        (r#"[{"status":"COMPLETED","conclusion":"NEUTRAL"}]"#, true),
        ("[]", false),
        (
            r#"[{"status":"COMPLETED","conclusion":"SUCCESS"},{"status":"IN_PROGRESS","conclusion":null}]"#,
            false,
        ),
        (r#"[{"status":"COMPLETED","conclusion":"FAILURE"}]"#, false),
        (
            r#"[{"status":"COMPLETED","conclusion":"CANCELLED"}]"#,
            false,
        ),
        (r#"[{"status":"COMPLETED","conclusion":"SKIPPED"}]"#, false),
        (r#"[{"state":"PENDING"}]"#, false),
        (r#"[{"state":"ERROR"}]"#, false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .env("GH_FIXTURE_CHECKS", checks)
            .output()?;
        assert_eq!(output.status.success(), accepted, "{checks}");
        assert_eq!(fixture.log()?.contains("\nready\n"), accepted, "{checks}");
    }
    let fixture = Fixture::new(BODY)?;
    let view = serde_json::json!({"title":"@coderabbitai","body":"@coderabbitai summary","state":"OPEN","isDraft":true});
    assert!(
        fixture
            .command("ready")?
            .args(["--pr", "17", "--generation", "coderabbit"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .output()?
            .status
            .success()
    );
    Ok(())
}

#[test]
fn edit_and_gh_failure_preserve_one_attempt_and_report_the_failure() -> Result<()> {
    for operation in ["create", "edit"] {
        let fixture = Fixture::new(BODY)?;
        let mut command = fixture.command(operation)?;
        command
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body);
        if operation == "create" {
            command.args(["--head", "feature"]);
        } else {
            command.args(["--pr", "17"]);
        }
        let output = command.env("GH_FIXTURE_FAILURE", operation).output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("42"));
        // Creation also lists the open PRs it could depend on.
        assert_eq!(
            fixture.log()?.matches("invocation\n").count(),
            if operation == "create" { 4 } else { 3 }
        );
        assert_eq!(
            fixture.log()?.matches(&format!("\n{operation}\n")).count(),
            1
        );
    }
    let fixture = Fixture::new(BODY)?;
    assert!(
        fixture
            .document("edit", "fix: preserve edits")?
            .status
            .success()
    );
    assert!(fixture.log()?.contains("\nedit\n17\n"));
    Ok(())
}

#[test]
fn live_final_check_is_read_only_and_rejects_malformed_or_pending_documents() -> Result<()> {
    for (response, accepted) in [
        (serde_json::json!({"title":"fix: preserve edits", "body":BODY,"state":"OPEN","isDraft":false}).to_string(), true),
        (serde_json::json!({"title":"@coderabbitai", "body":"@coderabbitai summary","state":"OPEN","isDraft":false}).to_string(), false),
        ("{truncated".into(), false),
        (r#"{"title":"fix: preserve edits","body":null,"state":"OPEN","isDraft":true}"#.into(), false),
    ] {
        let fixture = Fixture::new(BODY)?;
        let output = fixture.command("check")?.args(["--pr", "17", "--generation", "coderabbit", "--final"]).env("GH_FIXTURE_VIEW", response).output()?;
        assert_eq!(output.status.success(), accepted);
        assert_eq!(fixture.log()?.matches("invocation\n").count(), if accepted { 3 } else { 2 });
        assert!(fixture.log()?.contains("\nview\n"));
    }
    for failure in ["view", "ready"] {
        let fixture = Fixture::new(BODY)?;
        let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
        let output = fixture
            .command("ready")?
            .args(["--pr", "17"])
            .env("GH_FIXTURE_VIEW", view.to_string())
            .env("GH_FIXTURE_FAILURE", failure)
            .output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("42"));
        assert_eq!(
            fixture.log()?.matches("invocation\n").count(),
            if failure == "view" { 2 } else { 4 }
        );
    }
    Ok(())
}

#[test]
fn body_input_changes_and_cleanup_cannot_change_checked_publication_bytes() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let output = fixture
        .command("create")?
        .args(["--title", "fix: preserve edits", "--body-file"])
        .arg(&fixture.body)
        .args(["--head", "feature"])
        .env("GH_FIXTURE_REWRITE_FILE", &fixture.body)
        .output()?;
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(&fixture.body)?,
        "changed after validation"
    );
    let log = fixture.log()?;
    assert!(log.contains(BODY));
    let lines: Vec<_> = log.lines().collect();
    let index = lines
        .iter()
        .position(|line| *line == "--body-file")
        .unwrap();
    assert!(!std::path::Path::new(lines[index + 1]).try_exists()?);
    Ok(())
}

#[test]
fn invalid_repository_missing_files_and_invalid_cli_never_start_gh() -> Result<()> {
    for repo in [
        "project",
        "https://github.com/owner/project",
        "owner/project/other",
        "../project",
        "owner/--help",
    ] {
        let fixture = Fixture::new(BODY)?;
        let output = fixture
            .command_for_repo("create", repo)?
            .args(["--title", "fix: preserve edits", "--body-file"])
            .arg(&fixture.body)
            .args(["--head", "feature"])
            .output()?;
        assert!(!output.status.success());
        assert!(!fixture.log.try_exists()?);
    }
    let fixture = Fixture::new(BODY)?;
    fs::remove_file(&fixture.body)?;
    assert!(
        !fixture
            .document("create", "fix: preserve edits")?
            .status
            .success()
    );
    assert!(!fixture.log.try_exists()?);
    assert!(
        !fixture
            .command("ready")?
            .args(["--pr", "0"])
            .output()?
            .status
            .success()
    );
    assert!(!fixture.log.try_exists()?);
    Ok(())
}

#[test]
fn scoped_install_preserves_other_managed_skills_policy_and_history() -> Result<()> {
    use dotfiles_xtask::skill_ops as ops;
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let home = tempfile::tempdir()?;
    let skills = home.path().join(".agents/skills");
    fs::create_dir_all(&skills)?;
    let mut catalog = ops::catalog(&source.join("dot_agents/skills"))?;
    catalog.skills.remove("pull-request");
    for skill in catalog.skills.values() {
        for relative in &skill.files {
            let destination = skills.join(relative);
            fs::create_dir_all(destination.parent().unwrap())?;
            fs::copy(source.join("dot_agents/skills").join(relative), destination)?;
        }
    }
    fs::create_dir(skills.join("other-owned"))?;
    fs::write(
        skills.join("other-owned/SKILL.md"),
        "---\nname: other-owned\ndescription: Inspect an independent existing skill.\n---\n# Independent\nPreserve this skill.\n",
    )?;
    catalog = ops::catalog(&skills)?;
    let policy: ops::Policy = ops::read_json(&source.join("dot_config/skill-ops/policy.json"))?;
    ops::write_json(
        &home.path().join(".config/skill-ops/policy.json"),
        &policy,
        false,
    )?;
    let manifest = home.path().join(".config/skill-ops/approved.json");
    ops::write_json(
        &manifest,
        &ops::Approved {
            catalog_hash: ops::hash(&catalog)?,
            catalog,
            policy_hash: ops::hash(&policy)?,
            engine_hash: "existing-native-engine".into(),
            reviewed: Default::default(),
        },
        false,
    )?;
    let history = home.path().join(".local/state/skill-ops/event.json");
    ops::write_json(&history, &serde_json::json!({"preserved":true}), false)?;
    let binary = home.path().join("built-binary");
    fs::write(&binary, "native executable fixture")?;
    dotfiles_xtask::pr_workflow::deploy(source, &binary, home.path(), |paths| {
        for path in paths {
            if let Ok(relative) = path.strip_prefix(source.join("dot_agents/skills/pull-request")) {
                let destination = skills.join("pull-request").join(relative);
                fs::create_dir_all(destination.parent().unwrap())?;
                fs::copy(path, destination)?;
            } else {
                assert_eq!(*path, source.join("dot_claude/skills/symlink_pull-request"));
            }
        }
        Ok(())
    })?;
    let installed: ops::Approved = ops::read_json(&manifest)?;
    assert!(installed.catalog.skills.contains_key("other-owned"));
    assert!(installed.catalog.skills.contains_key("pull-request"));
    assert_eq!(installed.engine_hash, "existing-native-engine");
    assert_eq!(
        installed
            .reviewed
            .get("pull-request")
            .map(|reviewed| &reviewed.revision),
        Some(&installed.catalog.skills["pull-request"].revision)
    );
    assert_eq!(installed.policy_hash, ops::hash(&policy)?);
    assert_eq!(
        ops::read_json::<serde_json::Value>(&history)?,
        serde_json::json!({"preserved":true})
    );
    assert_eq!(
        fs::read(
            home.path()
                .join(".local/bin")
                .join(format!("pr-workflow{}", std::env::consts::EXE_SUFFIX))
        )?,
        fs::read(&binary)?
    );
    let before = fs::read(&manifest)?;
    assert!(
        dotfiles_xtask::pr_workflow::deploy(source, &binary, home.path(), |_| anyhow::bail!(
            "injected apply failure"
        ))
        .is_err()
    );
    assert_eq!(fs::read(&manifest)?, before);
    fs::write(skills.join("other-owned/SKILL.md"), "changed locally")?;
    assert!(
        dotfiles_xtask::pr_workflow::deploy(source, &binary, home.path(), |_| panic!(
            "must not apply over unrelated changes"
        ))
        .is_err()
    );
    Ok(())
}

/// A body that does not close the issue is refused with the edit that closes it, never the unchanged command.
#[test]
fn a_body_without_the_closing_reference_names_the_edit_that_adds_it() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let create = |fixture: &Fixture| -> Result<Output> {
        Ok(fixture
            .personal_command("create")?
            .args([
                "--issue",
                "23",
                "--title",
                "fix: preserve edits",
                "--body-file",
            ])
            .arg(&fixture.body)
            .args(["--head", "feature"])
            .output()?)
    };
    let record = refusal(&create(&fixture)?);
    assert_eq!(record["rule"], "pr.body");
    let body = fixture.body.to_str().context("UTF-8 path")?;
    let quoted = dotfiles_xtask::refusal::command(&[body]);
    let next = record["next"].as_str().unwrap();
    assert_eq!(
        next,
        format!(
            "printf '\\n\\nCloses #23\\n' >> {quoted} && pr-workflow create --repo P4suta/project --issue 23 --title 'fix: preserve edits' --body-file {quoted} --head feature"
        )
    );
    let mut appended = fs::read_to_string(&fixture.body)?;
    appended.push_str("\n\nCloses #23\n");
    fs::write(&fixture.body, appended)?;
    let output = create(&fixture)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let fixture = Fixture::new(BODY)?;
    let view = serde_json::json!({"title":"fix: preserve edits","body":BODY,"state":"OPEN","isDraft":true});
    let output = fixture
        .personal_command("ready")?
        .args(["--issue", "23", "--pr", "17"])
        .env("GH_FIXTURE_VIEW", view.to_string())
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.body");
    assert_eq!(
        record["next"],
        "gh pr edit 17 --repo P4suta/project --body-file <body-file>"
    );
    Ok(())
}

const PASSING: &str = r#"[{"status":"COMPLETED","conclusion":"SUCCESS"}]"#;
const FAILING: &str = r#"[{"status":"COMPLETED","conclusion":"FAILURE"}]"#;

/// One read of a PR for `GH_FIXTURE_VIEWS`; `merge` is GitHub's merge state, or `MERGED` for a merged PR.
fn read(number: u64, head: &str, draft: bool, body: &str, checks: &str, merge: &str) -> String {
    let checks: serde_json::Value = serde_json::from_str(checks).unwrap();
    format!(
        "{number}={}",
        serde_json::json!({
            "title":"fix: preserve edits","body":body,
            "state":if merge == "MERGED" {"MERGED"} else {"OPEN"},"isDraft":draft,
            "headRefOid":head,"baseRefName":if number == 17 {"parent"} else {"main"},
            "mergeable":if merge == "DIRTY" {"CONFLICTING"} else {"MERGEABLE"},
            "mergeStateStatus":merge,"statusCheckRollup":checks,
            "closingIssuesReferences":[{"number":23}]
        })
    )
}

/// The PR mutations the log records, in order.
fn mutations(log: &str) -> Vec<String> {
    log.split("invocation\n")
        .filter_map(|call| {
            let words: Vec<&str> = call.lines().collect();
            (words.first() == Some(&"pr")
                && words
                    .get(1)
                    .is_some_and(|word| ["update-branch", "edit", "ready", "merge"].contains(word)))
            .then(|| words[1].to_owned())
        })
        .collect()
}

#[test]
fn independent_prs_merge_in_order_through_update_checks_ready_and_the_verified_head() -> Result<()>
{
    let closed = format!("{BODY}\nCloses #23.\n");
    let excluded = format!("{closed}\n@coderabbitai ignore\n{PAUSE_MARKER}\n");
    let fixture = Fixture::new(BODY)?;
    fixture.pause("paused-pr")?;
    let views = [
        read(17, "old", true, &closed, PASSING, "DRAFT"),
        read(17, "new", true, &closed, "[]", "DRAFT"),
        read(17, "new", true, &closed, PASSING, "DRAFT"),
        read(17, "new", true, &excluded, PASSING, "DRAFT"),
        read(17, "new", false, &excluded, PASSING, "UNKNOWN"),
        read(17, "new", false, &excluded, PASSING, "CLEAN"),
        read(17, "new", false, &excluded, PASSING, "MERGED"),
        read(18, "other", false, &excluded, PASSING, "CLEAN"),
        read(18, "other", false, &excluded, PASSING, "MERGED"),
    ]
    .join("\n");
    let output = fixture
        .personal_command("merge")?
        .args(["--pr", "17", "--pr", "18", "--interval", "0"])
        .env("GH_FIXTURE_VIEWS", views)
        .env("GH_FIXTURE_BEHIND", "old")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fixture.log()?;
    assert_eq!(
        mutations(&log),
        ["update-branch", "edit", "ready", "merge", "merge"]
    );
    assert!(log.contains("\nupdate-branch\n--repo\nP4suta/project\n17\n"));
    assert!(
        log.contains("\nmerge\n--repo\nP4suta/project\n17\n--squash\n--match-head-commit\nnew\n")
    );
    assert!(
        log.contains("\nmerge\n--repo\nP4suta/project\n18\n--squash\n--match-head-commit\nother\n")
    );
    let edited = log.split("\nedit\n").nth(1).unwrap_or_default();
    assert!(
        edited.contains("@coderabbitai ignore") && edited.contains(PAUSE_MARKER),
        "{edited}"
    );
    Ok(())
}

#[test]
fn a_merge_stops_with_a_structured_refusal_and_merges_nothing() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let excluded = format!("{closed}\n@coderabbitai ignore\n{PAUSE_MARKER}\n");
    for (view, paused, timeout, rule) in [
        (
            read(17, "h", false, &excluded, FAILING, "BLOCKED"),
            true,
            "120",
            "pr.checks",
        ),
        (
            read(17, "h", false, &excluded, PASSING, "DIRTY"),
            true,
            "120",
            "pr.conflict",
        ),
        (
            read(17, "h", false, &closed, PASSING, "CLEAN"),
            false,
            "120",
            "pr.review",
        ),
        (
            read(17, "h", false, &excluded, "[]", "BLOCKED"),
            true,
            "0",
            "pr.timeout",
        ),
    ] {
        let fixture = Fixture::new(BODY)?;
        if paused {
            fixture.pause("paused-pr")?;
        }
        let output = fixture
            .personal_command("merge")?
            .args(["--pr", "17", "--interval", "0", "--timeout", timeout])
            .env("GH_FIXTURE_VIEWS", view)
            .output()?;
        assert_eq!(refusal(&output)["rule"], rule);
        assert!(mutations(&fixture.log()?).is_empty(), "{rule}");
    }
    Ok(())
}

/// Runs Git in `directory` with an isolated configuration, so no hook or signing of the caller applies.
fn git(directory: &std::path::Path, scope: &std::path::Path, arguments: &[&str]) -> String {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .current_dir(directory)
        .env("GIT_CONFIG_GLOBAL", scope.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A checkout with an `origin` holding `main` and the branches `a`, `feature` on top of `a`, `solo` touching what `a` touches, and `clean`.
fn branches(scope: &std::path::Path) -> Result<PathBuf> {
    fs::write(
        scope.join("gitconfig"),
        "[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n",
    )?;
    git(scope, scope, &["init", "-q", "--bare", "remote.git"]);
    git(scope, scope, &["init", "-q", "-b", "main", "checkout"]);
    let checkout = scope.join("checkout");
    let commit = |branch: &str, from: &str, file: &str| -> Result<()> {
        git(&checkout, scope, &["checkout", "-q", "-B", branch, from]);
        fs::write(checkout.join(file), branch)?;
        git(&checkout, scope, &["add", file]);
        git(&checkout, scope, &["commit", "-q", "-m", branch]);
        git(&checkout, scope, &["push", "-q", "origin", branch]);
        Ok(())
    };
    git(
        &checkout,
        scope,
        &["remote", "add", "origin", "../remote.git"],
    );
    fs::write(checkout.join("README"), "base")?;
    git(&checkout, scope, &["add", "README"]);
    git(&checkout, scope, &["commit", "-q", "-m", "base"]);
    git(&checkout, scope, &["push", "-q", "origin", "main"]);
    commit("a", "main", "shared.txt")?;
    commit("feature", "a", "feature.txt")?;
    commit("solo", "main", "shared.txt")?;
    commit("clean", "main", "clean.txt")?;
    Ok(checkout)
}

#[test]
fn a_branch_sharing_work_with_an_open_pr_is_refused_with_the_stack_command() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let fixture = Fixture::new(&closed)?;
    let checkout = branches(fixture.directory.path())?;
    let create = |head: &str, extra: &[&str]| -> Result<Output> {
        let mut command = fixture.personal_command("create")?;
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        Ok(command
            .current_dir(&checkout)
            .env(
                "GIT_CONFIG_GLOBAL",
                fixture.directory.path().join("gitconfig"),
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GH_FIXTURE_LIST",
                r#"[{"number":5,"headRefName":"a","baseRefName":"main"}]"#,
            )
            .args([
                "--issue",
                "23",
                "--title",
                "fix: preserve edits",
                "--body-file",
            ])
            .arg(&fixture.body)
            .args(["--head", head, "--base", "main"])
            .args(extra)
            .output()?)
    };
    let quoted = dotfiles_xtask::refusal::command(&[fixture.body.display().to_string()]);
    let record = refusal(&create("feature", &["--independent"])?);
    assert_eq!(record["rule"], "pr.dependency");
    assert_eq!(
        record["next"],
        format!(
            "pr-workflow stack create --repo P4suta/project --issue 23 --title 'fix: preserve edits' --body-file {quoted} --head feature --base a"
        )
    );
    assert_eq!(refusal(&create("solo", &[])?)["rule"], "pr.dependency");
    assert!(!fixture.log()?.contains("\ncreate\n"));
    for (head, extra) in [("solo", &["--independent"][..]), ("clean", &[][..])] {
        let output = create(head, extra)?;
        assert!(
            output.status.success(),
            "{head}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(fixture.log()?.matches("\ncreate\n").count(), 2);
    Ok(())
}

impl Fixture {
    fn stack_log(&self) -> String {
        fs::read_to_string(self.directory.path().join("gh-stack.log")).unwrap_or_default()
    }

    /// A checkout of `P4suta/project` on `topic`, whose lease file the gh-stack fixture compares with the token it receives.
    fn stack_checkout(&self, origin: &str) -> Result<(PathBuf, PathBuf)> {
        let scope = self.directory.path();
        fs::write(scope.join("gitconfig"), "")?;
        git(scope, scope, &["init", "-q", "-b", "topic", "checkout"]);
        let checkout = scope.join("checkout");
        git(&checkout, scope, &["remote", "add", "origin", origin]);
        Ok((checkout.clone(), checkout.join(".git/dotguard-stack.lease")))
    }

    fn stack_command(
        &self,
        checkout: &std::path::Path,
        lease: &std::path::Path,
        arguments: &[&str],
    ) -> Result<Command> {
        let mut command = self.personal_command(&format!("stack {}", arguments[0]))?;
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(checkout)
            .env("GIT_CONFIG_GLOBAL", self.directory.path().join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GH_STACK_FIXTURE_LEASE", lease)
            .args(&arguments[1..]);
        Ok(command)
    }
}

#[test]
fn a_stacked_pr_is_created_on_its_parent_and_linked_down_to_the_trunk() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let fixture = Fixture::new(&closed)?;
    let (checkout, lease) = fixture.stack_checkout("https://github.com/P4suta/project.git")?;
    let list = r#"[{"number":4,"headRefName":"bottom","baseRefName":"main"},{"number":5,"headRefName":"parent","baseRefName":"bottom"}]"#;
    let create = |base: &str| -> Result<Output> {
        Ok(fixture
            .stack_command(&checkout, &lease, &["create"])?
            .env("GH_FIXTURE_LIST", list)
            .args([
                "--issue",
                "23",
                "--title",
                "fix: preserve edits",
                "--body-file",
            ])
            .arg(&fixture.body)
            .args(["--head", "topic", "--base", base])
            .output()?)
    };
    let output = create("parent")?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fixture
            .log()?
            .contains("\ncreate\n--repo\nP4suta/project\n--head\ntopic\n--base\nparent\n--draft\n")
    );
    assert_eq!(
        fixture.stack_log(),
        "gh-stack link --base main 4 5 17 repo=P4suta/project leased=false notifier=1\n"
    );
    let record = refusal(&create("orphan")?);
    assert_eq!(record["rule"], "pr.stack");
    assert_eq!(fixture.log()?.matches("\ncreate\n").count(), 1);
    Ok(())
}

#[test]
fn stack_sync_and_rebase_run_gh_stack_under_the_lease_and_release_it() -> Result<()> {
    let fixture = Fixture::new(BODY)?;
    let (checkout, lease) = fixture.stack_checkout("git@github.com:P4suta/project.git")?;
    let output = fixture
        .stack_command(&checkout, &lease, &["sync"])?
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.stack_log(),
        "gh-stack checkout topic repo=P4suta/project leased=true notifier=1\n\
         gh-stack sync repo=P4suta/project leased=true notifier=1\n"
    );
    assert!(!lease.exists());

    let output = fixture
        .stack_command(&checkout, &lease, &["rebase"])?
        .env("GH_STACK_FIXTURE_EXIT", "3")
        .output()?;
    let record = refusal(&output);
    assert_eq!(record["rule"], "pr.conflict");
    assert_eq!(
        record["next"],
        "pr-workflow stack rebase --repo P4suta/project --continue"
    );
    assert!(
        fixture
            .stack_log()
            .ends_with("gh-stack rebase repo=P4suta/project leased=true notifier=1\n")
    );
    assert!(!lease.exists());

    fs::write(&lease, "held")?;
    let record = refusal(
        &fixture
            .stack_command(&checkout, &lease, &["sync"])?
            .output()?,
    );
    assert_eq!(record["rule"], "pr.stack");
    assert_eq!(fs::read_to_string(&lease)?, "held");

    let other = Fixture::new(BODY)?;
    let (checkout, lease) = other.stack_checkout("https://github.com/other/project.git")?;
    let record = refusal(
        &other
            .stack_command(&checkout, &lease, &["sync"])?
            .output()?,
    );
    assert_eq!(record["rule"], "pr.stack");
    assert!(other.stack_log().is_empty());
    Ok(())
}

#[test]
fn a_stack_merges_together_after_restacking_and_verifies_each_head() -> Result<()> {
    let closed = format!("{BODY}\nCloses #23.\n");
    let excluded = format!("{closed}\n@coderabbitai ignore\n{PAUSE_MARKER}\n");
    let fixture = Fixture::new(BODY)?;
    fixture.pause("paused-pr")?;
    let (checkout, lease) = fixture.stack_checkout("https://github.com/P4suta/project.git")?;
    let views = [
        read(5, "parent-head", false, &excluded, PASSING, "BLOCKED"),
        read(5, "parent-head", false, &excluded, PASSING, "BLOCKED"),
        read(5, "parent-head", false, &excluded, PASSING, "MERGED"),
        read(17, "stale", false, &excluded, PASSING, "BLOCKED"),
        read(17, "top-head", false, &excluded, PASSING, "BLOCKED"),
        read(17, "top-head", false, &excluded, PASSING, "MERGED"),
    ]
    .join("\n");
    let output = fixture
        .stack_command(&checkout, &lease, &["merge", "--pr", "17", "--interval", "0"])?
        .env(
            "GH_FIXTURE_LIST",
            r#"[{"number":5,"headRefName":"parent","baseRefName":"main"},{"number":17,"headRefName":"topic","baseRefName":"parent"}]"#,
        )
        .env("GH_FIXTURE_VIEWS", views)
        .env("GH_FIXTURE_BEHIND", "stale")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.stack_log(),
        "gh-stack checkout topic repo=P4suta/project leased=true notifier=1\n\
         gh-stack sync repo=P4suta/project leased=true notifier=1\n\
         gh-stack merge 17 --yes --squash repo=P4suta/project leased=false notifier=1\n"
    );
    assert!(mutations(&fixture.log()?).is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("#5 merged at parent-head") && stdout.contains("#17 merged at top-head"),
        "{stdout}"
    );
    Ok(())
}

/// pr-workflow and dotguard agree on the variable and file of the stack lease.
#[test]
fn the_stack_lease_names_match_dotguard() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let guard = fs::read_to_string(root.join("guard/src/stack.rs"))?;
    let workflow = fs::read_to_string(root.join("xtask/src/pr_workflow.rs"))?;
    for (guard_line, workflow_line) in [
        (
            "pub const TOKEN: &str = \"DOTGUARD_STACK\";",
            "const STACK_TOKEN: &str = \"DOTGUARD_STACK\";",
        ),
        (
            "pub const LEASE: &str = \"dotguard-stack.lease\";",
            "const STACK_LEASE: &str = \"dotguard-stack.lease\";",
        ),
    ] {
        assert!(guard.contains(guard_line), "{guard_line}");
        assert!(workflow.contains(workflow_line), "{workflow_line}");
    }
    Ok(())
}
