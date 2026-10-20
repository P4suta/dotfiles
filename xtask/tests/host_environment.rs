use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Path, PathBuf};
use xtask::{profile_snapshot::Combination, profiles};

#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

fn git(directory: &Path, global: &Path, arguments: &[&str]) -> Result<String> {
    let output = fixture_git::command("git", global)
        .current_dir(directory)
        .args(arguments)
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn tools(scope: &Path) -> Result<PathBuf> {
    let tools = scope.join("tools");
    fs::create_dir_all(&tools)?;
    let helper = tools.join(format!("git-remote-https{}", std::env::consts::EXE_SUFFIX));
    let status = platform::process::command("rustc")
        .args(["--edition", "2024"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/native_fetch.rs"
        ))
        .arg("-o")
        .arg(&helper)
        .status()?;
    ensure!(status.success(), "compile the native transport fixture");
    fs::copy(
        &helper,
        tools.join(format!("ssh{}", std::env::consts::EXE_SUFFIX)),
    )?;
    Ok(tools)
}

#[test]
fn applied_git_profiles_fetch_over_https_and_keep_all_pushes_on_ssh() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let scope = tempfile::tempdir()?;
    let initial = scope.path().join("initial.gitconfig");
    fs::write(
        &initial,
        "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
    )?;
    let source = scope.path().join("source");
    fs::create_dir_all(&source)?;
    git(&source, &initial, &["init", "--quiet"])?;
    git(
        &source,
        &initial,
        &["commit", "--quiet", "--allow-empty", "-m", "fixture"],
    )?;
    let head = git(&source, &initial, &["rev-parse", "HEAD"])?;
    let tools = tools(scope.path())?;
    for combination in Combination::all() {
        check_profile(
            &root,
            scope.path(),
            &source,
            &initial,
            &head,
            &tools,
            combination,
        )?;
    }
    Ok(())
}

fn check_profile(
    root: &Path,
    scope: &Path,
    source: &Path,
    initial: &Path,
    head: &str,
    tools: &Path,
    combination: Combination,
) -> Result<()> {
    let fixture = scope.join(combination.name());
    let home = fixture.join("home");
    fs::create_dir_all(&home)?;
    let mut answers = profiles::Answers::fixture(
        combination.profile,
        home.join("projects").to_string_lossy().into_owned(),
    );
    answers.role = combination.role.name();
    if combination.signing {
        answers.public_key = "ssh-ed25519 AAAA fixture".into();
    }
    let config = fixture.join("chezmoi.toml");
    fs::write(&config, profiles::configuration(root, &answers)?)?;
    let rendered = profiles::chezmoi(root, &fixture, &config, &home)
        .args(["execute-template", "--file"])
        .arg(root.join("home/dot_config/git/config.tmpl"))
        .output()?;
    ensure!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let github = profiles::chezmoi(root, &fixture, &config, &home)
        .args(["execute-template", "--file"])
        .arg(root.join("home/dot_config/gh/config.yml.tmpl"))
        .output()?;
    ensure!(
        github.status.success(),
        "render the GitHub CLI configuration"
    );
    assert!(String::from_utf8(github.stdout)?.contains("git_protocol: https\n"));
    let scratch = dotfiles::host_checks::Scratch::new(&config)?;
    let command = scratch.command("git");
    let applied = PathBuf::from(
        command
            .get_envs()
            .find_map(|(name, value)| (name == "GIT_CONFIG_GLOBAL").then_some(value).flatten())
            .context("scratch Git configuration")?,
    );
    fs::create_dir_all(
        applied
            .parent()
            .context("scratch Git configuration directory")?,
    )?;
    fs::write(&applied, &rendered.stdout)?;
    let repository = fixture.join("repository");
    fs::create_dir_all(&repository)?;
    git(&repository, initial, &["init", "--quiet", "--bare"])?;
    assert_urls(&repository, &applied)?;
    let log = fixture.join("transport");
    let run = |url: &str| -> Result<std::process::Output> {
        let mut path = vec![tools.to_path_buf()];
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut command = scratch.fetch_command(&repository, url);
        command
            .env("GIT_CONFIG_GLOBAL", &applied)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_EXEC_PATH", tools)
            .env("PATH", std::env::join_paths(path)?)
            .env("SKILL_OPS_STATE", fixture.join("state"))
            .env("HOST_FETCH_LOG", &log)
            .env("HOST_FETCH_HEAD", head)
            .env("HOST_FETCH_OBJECTS", source.join(".git/objects"));
        Ok(command.output()?)
    };
    let clean = rendered.stdout;
    let broken = String::from_utf8(clean.clone())?.replace(
        "pushInsteadOf = https://github.com/",
        "insteadOf = https://github.com/",
    );
    fs::write(&applied, broken)?;
    assert!(
        !run("https://github.com/third-party/public.git")?
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(&log)?, "ssh without an agent");
    fs::write(&applied, clean)?;
    for url in [
        "https://github.com/third-party/public.git",
        "https://github.com/P4suta/OComment",
        "https://github.com/P4suta/OComment.git",
        "https://github.com/P4suta/private-fixture.git",
        "https://github.com/P4suta/public-fixture.git",
    ] {
        let output = run(url)?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(&log)?, "https without credentials");
    }
    assert_eq!(
        git(&repository, initial, &["rev-parse", "FETCH_HEAD"])?,
        head
    );
    Ok(())
}

fn assert_urls(repository: &Path, applied: &Path) -> Result<()> {
    for (url, fetch, push) in [
        (
            "https://github.com/P4suta/OComment",
            "https://github.com/P4suta/OComment",
            "git@github.com:P4suta/OComment",
        ),
        (
            "https://github.com/P4suta/OComment.git",
            "https://github.com/P4suta/OComment.git",
            "git@github.com:P4suta/OComment.git",
        ),
        (
            "https://github.com/P4suta/private-fixture.git",
            "https://github.com/P4suta/private-fixture.git",
            "git@github.com:P4suta/private-fixture.git",
        ),
        (
            "https://github.com/P4suta/public-fixture.git",
            "https://github.com/P4suta/public-fixture.git",
            "git@github.com:P4suta/public-fixture.git",
        ),
        (
            "https://github.com/third-party/public.git",
            "https://github.com/third-party/public.git",
            "git@github.com:third-party/public.git",
        ),
    ] {
        git(repository, applied, &["config", "remote.fixture.url", url])?;
        assert_eq!(
            git(repository, applied, &["remote", "get-url", "fixture"])?,
            fetch
        );
        assert_eq!(
            git(
                repository,
                applied,
                &["remote", "get-url", "--push", "fixture"]
            )?,
            push
        );
    }
    Ok(())
}
