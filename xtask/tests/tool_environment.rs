use dotfiles_xtask::tool::{GIT_LOCAL_ENVIRONMENT, Tool};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

#[expect(
    clippy::disallowed_methods,
    reason = "the shared constructor for fixture Git processes"
)]
#[path = "../../guard/tests/support/fixture_git.rs"]
mod fixture_git;

#[test]
fn the_cleared_variables_are_exactly_gits_repository_local_variables() {
    let output = Tool::Git
        .command()
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .expect("run git");
    assert!(output.status.success());
    let listed: BTreeSet<String> = String::from_utf8(output.stdout)
        .expect("utf-8")
        .lines()
        .map(str::to_owned)
        .collect();
    let cleared: BTreeSet<String> = GIT_LOCAL_ENVIRONMENT.map(str::to_owned).into();
    assert_eq!(cleared, listed);
    assert!(
        listed.iter().all(|name| name.starts_with("GIT_")),
        "the fixture helper removes these by their prefix"
    );
}

const FIXTURE_CHILD: &str = "DOTFILES_FIXTURE_GIT_CHILD";

/// Stages CRLF text in a fixture repository whose configuration declares no conversion; it runs only inside the next test.
#[test]
fn fixture_git_child() {
    if std::env::var_os(FIXTURE_CHILD).is_none() {
        return;
    }
    let scope = tempfile::tempdir().unwrap();
    let global = scope.path().join("gitconfig");
    fs::write(&global, "").unwrap();
    let repository = scope.path().join("repository");
    fs::create_dir(&repository).unwrap();
    let git = |arguments: &[&str]| {
        let output = fixture_git::command("git", &global)
            .current_dir(&repository)
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    git(&["init", "--quiet"]);
    fs::write(repository.join("notes.txt"), b"a\r\nb\r\n").unwrap();
    git(&["add", "notes.txt"]);
    assert_eq!(git(&["cat-file", "blob", ":notes.txt"]), b"a\r\nb\r\n");
    assert_eq!(
        String::from_utf8(git(&["check-attr", "--all", "notes.txt"])).unwrap(),
        ""
    );
}

/// A commit hook's repository, configuration passed through the environment, and the user's Git attributes all stay out of a fixture.
#[test]
fn fixture_git_ignores_the_invoking_repository_and_user_configuration() {
    let scope = tempfile::tempdir().unwrap();
    let user = scope.path().join("config");
    fs::create_dir_all(user.join("git")).unwrap();
    fs::write(user.join("git/attributes"), "* text=auto eol=lf\n").unwrap();
    let output = dotfiles_xtask::tool::external(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_git_child", "--nocapture"])
        .env(FIXTURE_CHILD, "1")
        .env("GIT_DIR", scope.path().join("hook.git"))
        .env("GIT_INDEX_FILE", scope.path().join("hook-index"))
        .env("GIT_CONFIG_PARAMETERS", "'core.autocrlf'='true'")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.autocrlf")
        .env("GIT_CONFIG_VALUE_0", "true")
        .env("XDG_CONFIG_HOME", &user)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!scope.path().join("hook.git").exists());
}

/// Each line that starts a Git process, or the guard's Git wrapper, without the shared fixture helper.
fn bypasses(source: &str, tool_allowed: bool) -> Vec<String> {
    let construction = concat!("Command::", "new(");
    let mut found = Vec::new();
    for line in source.lines() {
        let mut rest = line;
        let mut bypassed = !tool_allowed && line.contains(concat!("Tool::", "Git"));
        while let Some(start) = rest.find(construction) {
            rest = &rest[start + construction.len()..];
            let program = rest.split(')').next().unwrap_or_default().trim();
            let program = program.trim_start_matches('&');
            bypassed |= program == "\"git\"" || program == "git" || program.ends_with(".git");
        }
        if bypassed {
            found.push(line.trim().to_owned());
        }
    }
    found
}

#[test]
fn the_bypass_check_finds_every_way_to_start_git() {
    for line in [
        concat!("Command::", "new(\"git\")"),
        concat!("std::process::Command::", "new( &self.git )"),
        concat!("Command::", "new(git)"),
        concat!("Tool::", "Git.command()"),
    ] {
        assert_eq!(bypasses(line, false), [line], "{line}");
    }
    assert!(bypasses(concat!("Tool::", "Git.command()"), true).is_empty());
    for line in [
        concat!("Command::", "new(\"rustc\")"),
        concat!("Command::", "new(program)"),
        concat!("Command::", "new(&self.github)"),
        "fixture_git::command(\"git\", &global)",
    ] {
        assert!(bypasses(line, false).is_empty(), "{line}");
    }
}

/// Every test process that runs Git against a fixture starts from `guard/tests/support/fixture_git.rs`.
#[test]
fn fixture_git_processes_start_from_the_shared_helper() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut found = Vec::new();
    for suite in ["xtask/tests", "guard/tests"] {
        for entry in fs::read_dir(root.join(suite)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            // This suite tests the tool registry's Git command itself.
            let tool_allowed = path.file_name() == Some(OsStr::new("tool_environment.rs"));
            for line in bypasses(&fs::read_to_string(&path).unwrap(), tool_allowed) {
                found.push(format!("{}: {line}", path.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "start these through fixture_git::command:\n{}",
        found.join("\n")
    );
}

#[test]
fn tool_processes_never_inherit_a_hooks_repository() {
    let command = Tool::Cargo.command();
    let removed: BTreeSet<&OsStr> = command
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name)
        .collect();
    for name in GIT_LOCAL_ENVIRONMENT {
        assert!(removed.contains(OsStr::new(name)), "{name} is inherited");
    }
}

#[test]
fn hook_gates_keep_the_commit_in_progress() {
    let command = Tool::Lefthook.hook_command();
    assert_eq!(command.get_envs().count(), 0);
}
