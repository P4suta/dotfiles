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
        let result = Command::new("rustc")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/gh.rs"))
            .arg("-o")
            .arg(
                directory
                    .path()
                    .join(format!("gh{}", std::env::consts::EXE_SUFFIX)),
            )
            .status()
            .unwrap();
        assert!(result.success());
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
            .env("GH_FIXTURE_LOG", &self.log)
            .env("GH_FIXTURE_USER", r#"{"login":"P4suta","id":42543015}"#)
            .env("GH_FIXTURE_REPOSITORY", serde_json::json!({
                "full_name":repo,"owner":{"login":repo.split('/').next().unwrap(),"id":1},"fork":false
            }).to_string())
            .args([operation, "--repo", repo]);
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

    fn log(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.log)?)
    }
}

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
        assert_eq!(fixture.log()?.matches("invocation\n").count(), 3);
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
