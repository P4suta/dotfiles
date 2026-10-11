use anyhow::{Context, Result, ensure};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use xtask::profile_rules::Profile;
use xtask::{profile_snapshot::Combination, profiles};

const COMPLETION_LIMIT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn compile_fixture(executable: &Path) -> Result<()> {
    let compiled = platform::process::command("rustc")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gh_token.rs"))
        .args(["--edition", "2024", "-o"])
        .arg(executable)
        .status()?;
    ensure!(compiled.success(), "compile the GitHub credential fixture");
    Ok(())
}

fn runtime(scope: &Path) -> Result<std::path::PathBuf> {
    let directory = scope.join("home/.local/bin");
    fs::create_dir_all(&directory)?;
    Ok(directory.join(format!("dotfiles{}", std::env::consts::EXE_SUFFIX)))
}

fn isolated(command: &mut Command, scope: &Path, path: &OsStr) -> Result<()> {
    command.env_clear().env("PATH", path).current_dir(scope);
    for name in ["SystemRoot", "COMSPEC", "PATHEXT"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for name in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_CONFIG_HOME",
        "MISE_CONFIG_DIR",
        "MISE_DATA_DIR",
        "MISE_CACHE_DIR",
        "MISE_STATE_DIR",
        "GH_CONFIG_DIR",
    ] {
        let directory = scope.join(name);
        fs::create_dir_all(&directory)?;
        command.env(name, directory);
    }
    Ok(())
}

fn lock_result(child: &mut Child, state: &Path) -> Result<(bool, ExitStatus)> {
    let reentered = state.join("reentered");
    let deadline = Instant::now() + COMPLETION_LIMIT;
    let completed = loop {
        if child.try_wait()?.is_some() {
            break true;
        }
        if reentered.exists() || Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    if reentered.exists() {
        assert!(
            child.try_wait()?.is_none(),
            "the reentrant installer must still be waiting"
        );
    }
    fs::write(state.join("release"), "release fixture waiters")?;
    Ok((completed, child.wait()?))
}

fn rendered(root: &Path, combination: Combination, scope: &Path) -> Result<toml::Table> {
    let home = scope.join("home");
    fs::create_dir_all(&home)?;
    let mut answers = profiles::Answers::fixture(combination.profile, "/fixture/projects".into());
    answers.role = combination.role.name();
    if combination.signing {
        answers.public_key = "ssh-ed25519 AAAA fixture".into();
    }
    let config = scope.join("chezmoi.toml");
    fs::write(&config, profiles::configuration(root, &answers)?)?;
    let output = profiles::chezmoi(root, scope, &config, &home)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .arg("cat")
        .arg(home.join(".config/mise/config.toml"))
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(toml::from_str(std::str::from_utf8(&output.stdout)?)?)
}

#[test]
fn every_profile_requests_the_github_host_from_the_cli_login() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("checkout")?;
    for combination in Combination::all() {
        let scope = tempfile::tempdir()?;
        let config = rendered(root, combination, scope.path())?;
        let (quote, name) = if combination.profile == Profile::Windows {
            ('"', "dotfiles.exe")
        } else {
            ('\'', "dotfiles")
        };
        let binary = scope.path().join("home/.local/bin").join(name);
        let expected =
            format!("{quote}{}{quote} github-token", binary.display()).replace('\\', "/");
        assert_eq!(
            config
                .get("settings")
                .and_then(|settings| settings.get("github"))
                .and_then(|github| github.get("credential_command"))
                .and_then(toml::Value::as_str)
                .map(|command| command.replace('\\', "/")),
            Some(expected),
            "{}",
            combination.name()
        );
    }
    Ok(())
}

#[test]
fn mise_uses_a_keyring_login_and_falls_back_when_the_cli_has_no_login() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("checkout")?;
    let scope = tempfile::tempdir()?;
    let bin = scope.path().join("bin");
    fs::create_dir(&bin)?;
    let gh = bin.join(format!("gh{}", std::env::consts::EXE_SUFFIX));
    compile_fixture(&gh)?;
    fs::copy(&gh, runtime(scope.path())?)?;
    let combination = Combination::all()
        .into_iter()
        .find(|combination| combination.profile == dotfiles::profiles::native_profile().unwrap())
        .context("native profile")?;
    let config = rendered(root, combination, scope.path())?;
    let global = scope.path().join("global.toml");
    fs::write(
        &global,
        toml::to_string(&config["settings"]["github"])
            .map(|github| format!("[settings.github]\n{github}"))?,
    )?;
    let empty = scope.path().join("empty.toml");
    fs::write(&empty, "")?;
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").context("PATH")?,
    ));
    let path = std::env::join_paths(paths)?;
    for (login, expected) in [("yes", "fixture-keyring-token\n"), ("no", "")] {
        for host in ["github.com", "github.fixture.invalid"] {
            let mut command = platform::process::command("mise");
            isolated(&mut command, scope.path(), &path)?;
            let output = command
                .env("MISE_GLOBAL_CONFIG_FILE", &global)
                .env("MISE_SYSTEM_CONFIG_FILE", &empty)
                .env("GH_FIXTURE_LOG", scope.path().join("gh.log"))
                .env("GH_FIXTURE_LOGIN", login)
                .args(["token", "github", "--raw", host])
                .output()?;
            assert_eq!(
                output.status.success(),
                login == "yes",
                "{login} {host}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                std::str::from_utf8(&output.stdout)?.replace("\r\n", "\n"),
                expected
            );
            assert_eq!(
                fs::read_to_string(scope.path().join("gh.log"))?,
                format!("auth token --hostname {host}")
            );
        }
    }
    Ok(())
}

#[test]
fn a_credential_lookup_finishes_while_mise_install_holds_the_lock() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("checkout")?;
    let scope = tempfile::tempdir()?;
    let bin = scope.path().join("bin");
    let shims = scope.path().join("data/shims");
    let installed = scope
        .path()
        .join("data/installs/gh/2.87.3/gh_2.87.3_macOS_arm64/bin");
    for directory in [&bin, &shims, &installed] {
        fs::create_dir_all(directory)?;
    }
    let executable = |directory: &Path, name: &str| {
        directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    };
    let mise = executable(&bin, "mise");
    compile_fixture(&mise)?;
    for (directory, name) in [(&shims, "gh"), (&installed, "gh")] {
        fs::copy(&mise, executable(directory, name))?;
    }
    fs::copy(&mise, runtime(scope.path())?)?;
    let inherited = std::env::var_os("PATH").context("PATH")?;
    let real_mise = std::env::split_paths(&inherited)
        .map(|directory| executable(&directory, "mise"))
        .find(|candidate| candidate.is_file())
        .context("real mise")?;
    let combination = Combination::all()
        .into_iter()
        .find(|combination| combination.profile == dotfiles::profiles::native_profile().unwrap())
        .context("native profile")?;
    let config = rendered(root, combination, scope.path())?;
    let global = scope.path().join("global.toml");
    let empty = scope.path().join("empty.toml");
    fs::write(&empty, "")?;
    let path = std::env::join_paths(
        [bin, shims]
            .into_iter()
            .chain(std::env::split_paths(&inherited)),
    )?;
    for (scenario, expected) in [("legacy", false), ("candidate", true)] {
        let state = scope.path().join(scenario);
        fs::create_dir(&state)?;
        let mut github = config["settings"]["github"].clone();
        if !expected {
            github["credential_command"] = toml::Value::String(
                if combination.profile == Profile::Windows {
                    "gh auth token --hostname %MISE_CREDENTIAL_HOST%"
                } else {
                    "gh auth token --hostname \"$MISE_CREDENTIAL_HOST\""
                }
                .into(),
            );
        }
        fs::write(
            &global,
            format!("[settings.github]\n{}", toml::to_string(&github)?),
        )?;
        let log = state.join("output.log");
        let output = fs::File::create(&log)?;
        let mut command = platform::process::command(&mise);
        isolated(&mut command, scope.path(), &path)?;
        let reentered = state.join("reentered");
        let mut child = command
            .env("MISE_GLOBAL_CONFIG_FILE", &global)
            .env("MISE_SYSTEM_CONFIG_FILE", &empty)
            .env("GH_FIXTURE_LOG", state.join("gh.log"))
            .env("GH_FIXTURE_LOGIN", "yes")
            .env("GH_FIXTURE_MISE", &mise)
            .env("GH_FIXTURE_REAL_MISE", &real_mise)
            .env("GH_FIXTURE_INSTALLED_GH", executable(&installed, "gh"))
            .env("GH_FIXTURE_LOCK", state.join("install.lock"))
            .env("GH_FIXTURE_HELD", state.join("held"))
            .env("GH_FIXTURE_RELEASE", state.join("release"))
            .env("GH_FIXTURE_REENTERED", &reentered)
            .args(["install", "-y"])
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output)
            .spawn()?;
        let (completed, status) = lock_result(&mut child, &state)?;
        ensure!(
            state.join("held").exists(),
            "the installer never held its lock"
        );
        ensure!(
            completed == expected && reentered.exists() != expected,
            "{scenario}: the credential command did not respect the install-lock boundary: {}",
            fs::read_to_string(&log)?
        );
        ensure!(status.success(), "{}", fs::read_to_string(&log)?);
        assert_eq!(
            fs::read_to_string(state.join("gh.log"))?,
            "auth token --hostname github.com"
        );
        assert_eq!(
            fs::read_to_string(log)?.replace("\r\n", "\n"),
            "fixture-keyring-token\n"
        );
    }
    Ok(())
}

#[test]
fn github_credentials_resolve_only_installed_cli_executables() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let shims = scope.path().join("data/shims");
    let native = scope.path().join("native");
    let versions = scope.path().join("data/installs/gh");
    let name = format!("gh{}", std::env::consts::EXE_SUFFIX);
    for directory in [&shims, &native] {
        fs::create_dir_all(directory)?;
        fs::write(directory.join(&name), "fixture executable")?;
    }
    let shim_path = std::env::join_paths([&shims])?;
    assert_eq!(
        platform::github::executable(&shim_path).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let path = std::env::join_paths([scope.path().join("missing"), shims.clone(), native.clone()])?;
    assert_eq!(
        platform::github::executable(&path)?,
        platform::paths::canonical(&native.join(&name))?
    );
    for version in ["2.87.3", "latest"] {
        let installed = versions.join(version).join("bin");
        fs::create_dir_all(&installed)?;
        fs::write(installed.join(&name), "installed executable")?;
        assert_eq!(
            platform::github::executable(&shim_path)?,
            platform::paths::canonical(&installed.join(&name))?
        );
    }
    Ok(())
}

#[test]
fn keyring_discovery_remains_available_without_shims_on_path() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let data = scope.path().join("mise");
    let installed = data.join("installs/gh/latest/gh_2.87.3_macOS_arm64/bin");
    let shims = data.join("shims");
    let bin = scope.path().join("bin");
    for directory in [&installed, &shims, &bin] {
        fs::create_dir_all(directory)?;
    }
    let gh = installed.join(format!("gh{}", std::env::consts::EXE_SUFFIX));
    compile_fixture(&gh)?;
    let runtime = runtime(scope.path())?;
    fs::copy(&gh, &runtime)?;
    fs::copy(
        &gh,
        shims.join(format!("gh{}", std::env::consts::EXE_SUFFIX)),
    )?;
    let path = std::env::join_paths([&bin])?;
    let mut command = platform::process::command(&runtime);
    isolated(&mut command, scope.path(), &path)?;
    let output = command
        .env("MISE_DATA_DIR", &data)
        .env("MISE_CREDENTIAL_HOST", "github.com")
        .env("MISE_CREDENTIAL_PROVIDER", "github")
        .env("GH_FIXTURE_LOG", scope.path().join("gh.log"))
        .env("GH_FIXTURE_LOGIN", "yes")
        .arg("github-token")
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)?.replace("\r\n", "\n"),
        "fixture-keyring-token\n"
    );
    assert_eq!(
        fs::read_to_string(scope.path().join("gh.log"))?,
        "auth token --hostname github.com"
    );
    Ok(())
}

#[test]
fn a_mise_alias_file_selects_its_cli_when_other_versions_remain() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let shims = scope.path().join("data/shims");
    let versions = scope.path().join("data/installs/gh");
    let name = format!("gh{}", std::env::consts::EXE_SUFFIX);
    fs::create_dir_all(&shims)?;
    for version in ["2.87.3", "2.9.0"] {
        let installed = versions.join(version).join("bin");
        fs::create_dir_all(&installed)?;
        fs::write(installed.join(&name), "installed executable")?;
    }
    fs::write(
        versions.join("latest"),
        format!(".{}2.87.3", std::path::MAIN_SEPARATOR),
    )?;
    assert_eq!(
        platform::github::executable(&std::env::join_paths([shims])?)?,
        platform::paths::canonical(&versions.join("2.87.3/bin").join(name))?
    );
    Ok(())
}
