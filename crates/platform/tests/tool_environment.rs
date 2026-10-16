use anyhow::{Context, Result, ensure};
use platform::tool::{GIT_LOCAL_ENVIRONMENT, Tool};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

#[test]
fn docker_buildx_is_an_exact_project_tool() {
    assert!(Tool::ALL.iter().any(|tool| {
        tool.program() == "docker-cli-plugin-docker-buildx"
            && matches!(
                tool.provision(),
                platform::tool::Provision::Pinned("aqua:docker/buildx")
            )
    }));
}

#[cfg(windows)]
#[test]
fn native_windows_tar_ignores_path_shadowing_for_drive_letter_archives() -> Result<()> {
    use platform::runtime::{Real, Runner, checked};

    const PAYLOAD: &str = "archive payload";
    let scope = tempfile::tempdir()?;
    let root = platform::paths::canonical(scope.path())?;
    let shadow = root.join("shadow");
    let extract = root.join("extract");
    fs::create_dir(&shadow)?;
    fs::create_dir(&extract)?;
    fs::copy(std::env::current_exe()?, shadow.join("tar.exe"))?;
    fs::write(root.join("payload.txt"), PAYLOAD)?;
    let native =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").context("native Windows root")?)
            .join("System32/tar.exe");
    let archive = root.join("fixture.zip");
    let created = platform::process::command(&native)
        .args(["-c", "-a", "-f"])
        .arg(&archive)
        .arg("-C")
        .arg(&root)
        .arg("payload.txt")
        .output()?;
    ensure!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let mut runner = Real::new(&root)?;
    runner.path = shadow.into_os_string();
    assert_eq!(runner.command(Tool::TarExe).get_program(), native);
    assert!(runner.available(Tool::TarExe));
    let listing = checked(
        &mut runner,
        Tool::TarExe,
        &["-tf".into(), archive.clone().into()],
    )?;
    assert_eq!(String::from_utf8(listing)?.trim(), "payload.txt");
    checked(
        &mut runner,
        Tool::TarExe,
        &[
            "-xf".into(),
            archive.into(),
            "-C".into(),
            extract.clone().into(),
        ],
    )?;
    assert_eq!(fs::read_to_string(extract.join("payload.txt"))?, PAYLOAD);
    Ok(())
}

#[cfg(windows)]
#[test]
fn powershell_cache_preserves_the_native_windows_environment() -> Result<()> {
    let cache = platform::process::PowerShellCache::new()?;
    let mut command = Tool::Pwsh.command();
    command.env("LOCALAPPDATA", "native-local");
    command.env("XDG_CACHE_HOME", "native-xdg");
    let before: Vec<_> = command
        .get_envs()
        .map(|(name, value)| (name.to_owned(), value.map(OsStr::to_owned)))
        .collect();
    cache.apply(&mut command);
    let after: Vec<_> = command
        .get_envs()
        .map(|(name, value)| (name.to_owned(), value.map(OsStr::to_owned)))
        .collect();
    assert_eq!(after, before);
    Ok(())
}

#[cfg(unix)]
#[test]
fn powershell_cache_cleanup_preserves_failed_runs_and_launch_errors() -> Result<()> {
    let cache = platform::process::PowerShellCache::new()?;
    let mut command = Tool::Pwsh.command();
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::Write($env:XDG_CACHE_HOME); [Console]::Error.Write('retained diagnostic'); exit 7",
    ]);
    let output = cache.apply(&mut command).output()?;
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stderr, b"retained diagnostic");
    let path = std::path::PathBuf::from(std::str::from_utf8(&output.stdout)?);
    assert!(path.is_dir());
    drop(cache);
    assert!(!path.exists());

    let scope = tempfile::tempdir()?;
    let cache = platform::process::PowerShellCache::new()?;
    let mut missing = platform::process::command(scope.path().join("missing-pwsh"));
    cache.apply(&mut missing);
    let path = missing
        .get_envs()
        .find(|(name, _)| name == &OsStr::new("XDG_CACHE_HOME"))
        .and_then(|(_, value)| value)
        .context("the cache was applied")?
        .to_owned();
    assert_eq!(
        missing.output().unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    drop(cache);
    assert!(!Path::new(&path).exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn simultaneous_powershell_children_own_their_caches_and_preserve_shared_state() -> Result<()> {
    const CLIENTS: usize = 4;
    let scope = tempfile::tempdir()?;
    let shared = scope.path().join("shared-cache");
    fs::create_dir_all(&shared)?;
    let sentinel = shared.join("sentinel");
    fs::write(&sentinel, b"preserved shared state")?;
    let barrier = std::sync::Barrier::new(CLIENTS);
    let paths = std::thread::scope(|threads| -> Result<Vec<std::path::PathBuf>> {
        let children: Vec<_> = (0..CLIENTS)
            .map(|client| {
                let shared = &shared;
                let barrier = &barrier;
                threads.spawn(move || -> Result<std::path::PathBuf> {
                    barrier.wait();
                    let cache = platform::process::PowerShellCache::new()?;
                    let mut command = Tool::Pwsh.command();
                    command
                        .args([
                            "-NoLogo",
                            "-NoProfile",
                            "-NonInteractive",
                            "-Command",
                            "$ErrorActionPreference = 'Stop'; [IO.File]::WriteAllText((Join-Path $env:XDG_CACHE_HOME 'owner'), $env:DOTFILES_CACHE_OWNER); [Console]::Write($env:XDG_CACHE_HOME)",
                        ])
                        .env("XDG_CACHE_HOME", shared)
                        .env("DOTFILES_CACHE_OWNER", client.to_string());
                    let output = cache.apply(&mut command).output()?;
                    ensure!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let path = std::path::PathBuf::from(std::str::from_utf8(&output.stdout)?);
                    assert_ne!(&path, shared);
                    assert_eq!(fs::read_to_string(path.join("owner"))?, client.to_string());
                    drop(cache);
                    assert!(!path.exists());
                    Ok(path)
                })
            })
            .collect();
        children
            .into_iter()
            .map(|child| child.join().expect("cache child panicked"))
            .collect()
    })?;
    assert_eq!(paths.iter().collect::<BTreeSet<_>>().len(), CLIENTS);
    assert_eq!(fs::read(sentinel)?, b"preserved shared state");
    assert_eq!(fs::read_dir(&shared)?.count(), 1);
    Ok(())
}

#[path = "../../dotguard/tests/support/fixture_git.rs"]
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
const DIRECT_CHILD: &str = "DOTFILES_DIRECT_GIT_CHILD";

#[test]
fn directly_constructed_children_ignore_the_invoking_repository() {
    if std::env::var_os(DIRECT_CHILD).is_some() {
        let scope = tempfile::tempdir().unwrap();
        let global = scope.path().join("gitconfig");
        fs::write(&global, "").unwrap();
        let output = fixture_git::constructor("git", &global)
            .current_dir(scope.path())
            .args(["init", "--quiet"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(scope.path().join(".git").is_dir());
        return;
    }
    let scope = tempfile::tempdir().unwrap();
    let output = platform::process::command(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "directly_constructed_children_ignore_the_invoking_repository",
            "--nocapture",
        ])
        .env(DIRECT_CHILD, "1")
        .env("GIT_DIR", scope.path().join("hook.git"))
        .env("GIT_INDEX_FILE", scope.path().join("hook-index"))
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

/// Stages CRLF text in a fixture repository whose configuration declares no conversion.
/// It runs only inside the next test.
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
    let output = platform::tool::external(std::env::current_exe().unwrap())
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
    let mut found = Vec::new();
    for line in source.lines() {
        let mut bypassed = !tool_allowed && line.contains(concat!("Tool::", "Git"));
        for construction in [
            concat!("Command::", "new("),
            concat!("process::", "command("),
        ] {
            let mut rest = line;
            while let Some(start) = rest.find(construction) {
                rest = &rest[start + construction.len()..];
                let program = rest.split(')').next().unwrap_or_default().trim();
                let program = program.trim_start_matches('&');
                let field = program.rsplit_once('.').map(|(_, field)| field);
                bypassed |= program == "\"git\"" || program == "git" || field == Some("git");
            }
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
        concat!("platform::process::", "command(\"git\")"),
        concat!("Tool::", "Git.command()"),
    ] {
        assert_eq!(bypasses(line, false), [line], "{line}");
    }
    assert_eq!(
        bypasses(concat!("Tool::", "Git.command()"), true),
        Vec::<String>::new()
    );
    for line in [
        concat!("Command::", "new(\"rustc\")"),
        concat!("Command::", "new(program)"),
        concat!("Command::", "new(&self.github)"),
        "fixture_git::command(\"git\", &global)",
    ] {
        assert!(bypasses(line, false).is_empty(), "{line}");
    }
}

fn check_fixture_git(root: &Path) -> Result<()> {
    let mut suites = vec![root.join("xtask/tests")];
    for entry in fs::read_dir(root.join("crates"))? {
        let tests = entry?.path().join("tests");
        if tests.is_dir() {
            suites.push(tests);
        }
    }
    let mut found = Vec::new();
    for suite in suites {
        for entry in fs::read_dir(suite)? {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            // This suite tests the tool registry's Git command itself.
            let tool_allowed = path.file_name() == Some(OsStr::new("tool_environment.rs"));
            for line in bypasses(&fs::read_to_string(&path)?, tool_allowed) {
                found.push(format!("{}: {line}", path.display()));
            }
        }
    }
    ensure!(
        found.is_empty(),
        "start these through fixture_git::command:\n{}",
        found.join("\n")
    );
    Ok(())
}

const FIXTURE_GUARD_CHILD: &str = "DOTFILES_FIXTURE_GIT_GUARD_CHILD";
const INCIDENT_EVIDENCE_ROOT: &str = "SKILL_OPS_INCIDENT_EVIDENCE_ROOT";
const RAW_FIXTURE_GIT: &str = "platform::tool::Tool::Git";

fn report_fixture_git_violation() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    fs::create_dir_all(root.join("xtask/tests"))?;
    fs::create_dir(root.join("crates"))?;
    let source = root.join("xtask/tests/checks.rs");
    fs::write(
        &source,
        format!(
            "fn original_fixture() {{ let output = {RAW_FIXTURE_GIT}\n.command(); }}\nfn canonical_fixture() {{ let output = {RAW_FIXTURE_GIT}\n.command(); }}\n"
        ),
    )?;
    let error = check_fixture_git(root).expect_err("both fixture Git calls bypass isolation");
    let diagnostic = format!("{error:#}");
    assert!(diagnostic.contains("start these through fixture_git::command"));
    assert_eq!(diagnostic.matches(RAW_FIXTURE_GIT).count(), 2);
    platform::report(&error);
    fs::write(
        source,
        "fn fixture(global: &std::path::Path) {\nlet first = fixture_git::command(\"git\", global);\nlet second = fixture_git::command(\"git\", global);\n}\n",
    )?;
    check_fixture_git(root)
}

fn check_fixture_git_incident(scope: &Path) -> Result<()> {
    let records =
        fs::read_dir(scope.join("state/incidents"))?.collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(records.len(), 1);
    let record: serde_json::Value = serde_json::from_slice(&fs::read(records[0].path())?)?;
    assert_eq!(record["schema"], 1);
    assert_eq!(record["rule"], "tool.failure");
    assert_eq!(
        record["source"],
        rules::incident_rules::Source::MalformedInput.key()
    );
    let evidence = record["evidence"].as_array().context("incident evidence")?;
    assert!(
        evidence
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(
                |item| item.contains("start these through fixture_git::command")
                    && item.replace('\\', "/").contains("xtask/tests/checks.rs")
                    && item.matches(RAW_FIXTURE_GIT).count() == 2
            )
    );
    Ok(())
}

/// Every test process that runs Git on a fixture starts from `crates/dotguard/tests/support/fixture_git.rs`.
#[test]
fn fixture_git_processes_start_from_the_shared_helper() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .context("platform has a repository ancestor")?;
    check_fixture_git(root)?;
    if std::env::var(FIXTURE_GUARD_CHILD).is_ok_and(|role| role == "1") {
        return report_fixture_git_violation();
    }
    let scope = match std::env::var_os(INCIDENT_EVIDENCE_ROOT) {
        Some(directory) => tempfile::tempdir_in(directory)?,
        None => tempfile::tempdir()?,
    };
    let home = scope.path().join("home");
    let opt_in = home.join(platform::incidents::EXPERIMENTAL);
    fs::create_dir_all(opt_in.parent().context("fixture consent has a parent")?)?;
    fs::write(opt_in, rules::incident_rules::EXPERIMENTAL_OPT_IN)?;
    let output = fixture_git::fixture_process::command(std::env::current_exe()?, scope.path())
        .args([
            "--exact",
            "fixture_git_processes_start_from_the_shared_helper",
            "--nocapture",
        ])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env(FIXTURE_GUARD_CHILD, "1")
        .env("DOTFILES_EXPECTED_FAILURE", "")
        .output()?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    ensure!(
        output.status.success(),
        "owned fixture Git counterexample failed"
    );
    check_fixture_git_incident(scope.path())?;
    if std::env::var_os(INCIDENT_EVIDENCE_ROOT).is_some() {
        let _ = scope.keep();
    }
    Ok(())
}

#[test]
fn tool_processes_never_inherit_a_hooks_repository() {
    for command in [
        Tool::Cargo.command(),
        platform::process::command("cargo"),
        platform::tool::external("cargo"),
        platform::runtime::Real::new(Path::new("fixture-home"))
            .unwrap()
            .command_installed(Path::new("fixture-tool")),
    ] {
        let removed: BTreeSet<&OsStr> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name)
            .collect();
        for name in GIT_LOCAL_ENVIRONMENT {
            assert!(removed.contains(OsStr::new(name)), "{name} is inherited");
        }
    }
}

#[test]
fn hook_gates_keep_the_commit_in_progress() {
    const REPOSITORY_CHILD: &str = "DOTFILES_REPOSITORY_GIT_CHILD";
    if let Some(repository) = std::env::var_os(REPOSITORY_CHILD) {
        let output = Tool::Git
            .hook_command()
            .args(["rev-parse", "--absolute-git-dir"])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            platform::paths::canonical(Path::new(String::from_utf8(output.stdout).unwrap().trim()))
                .unwrap(),
            platform::paths::canonical(Path::new(&repository)).unwrap()
        );
        return;
    }
    let scope = tempfile::tempdir().unwrap();
    let global = scope.path().join("gitconfig");
    fs::write(&global, "").unwrap();
    let initialized = fixture_git::command("git", &global)
        .current_dir(scope.path())
        .args(["init", "--quiet"])
        .status()
        .unwrap();
    assert!(initialized.success());
    let git = scope.path().join(".git");
    let output = platform::process::command(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "hook_gates_keep_the_commit_in_progress",
            "--nocapture",
        ])
        .env(REPOSITORY_CHILD, &git)
        .env("GIT_DIR", &git)
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
