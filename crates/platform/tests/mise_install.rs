use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use platform::runtime::Runner;

const COMPLETION_LIMIT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const ACTIVATION_FIXTURE: &str = "DOTFILES_INSTALL_ACTIVATION_FIXTURE";

fn executable(directory: &Path, name: &str) -> PathBuf {
    directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

fn complete(child: &mut Child, state: &Path) -> Result<(bool, ExitStatus)> {
    let deadline = Instant::now() + COMPLETION_LIMIT;
    let completed = loop {
        if child.try_wait()?.is_some() {
            break true;
        }
        if state.join("reentered").exists() || Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    if state.join("reentered").exists() {
        ensure!(
            child.try_wait()?.is_none(),
            "the shim must wait for the install lock"
        );
    }
    fs::write(state.join("release"), "release fixture waiters")?;
    Ok((completed, child.wait()?))
}

#[test]
fn a_backend_reaches_a_real_tool_while_mise_install_holds_its_lock() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let bin = scope.path().join("bin");
    let data = scope.path().join("mise");
    let shims = data.join("shims");
    let native = data.join("installs/native/bin");
    for directory in [&bin, &shims, &native] {
        fs::create_dir_all(directory)?;
    }
    let mise = executable(&bin, "mise");
    let compiled = platform::process::command("rustc")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mise_install.rs"))
        .args(["--edition", "2024", "-o"])
        .arg(&mise)
        .status()?;
    ensure!(compiled.success(), "compile the backend lock fixture");
    fs::copy(&mise, executable(&bin, "backend"))?;
    for name in ["uv", "erlc"] {
        for directory in [&shims, &native] {
            fs::copy(&mise, executable(directory, name))?;
        }
        for (scenario, expected) in [("legacy", false), ("candidate", true), ("runner", true)] {
            let state = scope.path().join(format!("{name}-{scenario}"));
            fs::create_dir(&state)?;
            let log = state.join("output.log");
            let output = fs::File::create(&log)?;
            let mut command = platform::process::command(&mise);
            command
                .env("PATH", std::env::join_paths([&shims, &native, &bin])?)
                .env("MISE_DATA_DIR", &data)
                .env("BACKEND_FIXTURE_MISE", &mise)
                .env("BACKEND_FIXTURE_NATIVE", executable(&native, name))
                .env("BACKEND_FIXTURE_TOOL", name)
                .env("BACKEND_FIXTURE_STATE", &state)
                .args(["install", "-y"])
                .stdin(Stdio::null())
                .stdout(output.try_clone()?)
                .stderr(output);
            let (completed, successful) = if scenario == "runner" {
                let mut runner = platform::runtime::Real::new(scope.path())?;
                runner.environment = command
                    .get_envs()
                    .filter_map(|(key, value)| {
                        value.map(|value| (key.to_owned(), value.to_owned()))
                    })
                    .collect();
                runner.directory = Some(scope.path().to_path_buf());
                std::thread::scope(|threads| -> Result<(bool, bool)> {
                    let running = threads.spawn(move || {
                        runner.run(platform::tool::Tool::Mise, &["install".into(), "-y".into()])
                    });
                    let deadline = Instant::now() + COMPLETION_LIMIT;
                    while !running.is_finished()
                        && !state.join("reentered").exists()
                        && Instant::now() < deadline
                    {
                        std::thread::sleep(POLL_INTERVAL);
                    }
                    let completed = running.is_finished();
                    fs::write(state.join("release"), "release fixture waiters")?;
                    let reply = running.join().expect("the runner fixture does not panic")?;
                    fs::write(&log, reply.bytes)?;
                    Ok((completed, reply.success))
                })?
            } else {
                let mut child = if expected {
                    platform::process::mise_install(command)?.spawn()?
                } else {
                    command.spawn()?
                };
                let (completed, status) = complete(&mut child, &state)?;
                (completed, status.success())
            };
            ensure!(
                state.join("held").exists(),
                "the installer never held its lock"
            );
            ensure!(
                completed == expected && state.join("reentered").exists() != expected,
                "{name} {scenario}: unexpected install-lock reentry: {}",
                fs::read_to_string(&log)?
            );
            ensure!(successful, "{}", fs::read_to_string(&log)?);
            assert_eq!(fs::read_to_string(state.join("native"))?, name);
            assert_eq!(
                fs::read_to_string(&log)?.replace("\r\n", "\n"),
                format!("fixture-native-{name}\n")
            );
        }
    }
    Ok(())
}

#[test]
fn installation_paths_refuse_standard_and_configured_mise_shims() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let data = scope.path().join("custom-store");
    let configured = scope.path().join("custom-shims");
    let standard = scope.path().join("mise/shims");
    let scoop = scope.path().join("scoop/shims");
    let native = scope.path().join("mise/installs/uv/bin");
    let directories = [&standard, &data.join("shims"), &configured, &scoop, &native];
    let mut command = platform::process::command("mise");
    command
        .env("PATH", std::env::join_paths(directories)?)
        .env("MISE_DATA_DIR", &data)
        .env("MISE_SHIMS_DIR", &configured)
        .args(["install", "-y"]);
    let install = platform::process::mise_install(command)?;
    let path = install
        .get_envs()
        .find_map(|(key, value)| (key == OsStr::new("PATH")).then_some(value).flatten())
        .context("the installation must have an explicit PATH")?;
    let actual: Vec<_> = std::env::split_paths(path).collect();
    assert_eq!(actual, [scoop, native]);
    Ok(())
}

#[test]
fn shim_aliases_are_excluded_after_filesystem_resolution() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let shims = scope.path().join("mise").join("shims");
    let alias = scope.path().join("alias");
    let native = scope.path().join("native");
    fs::create_dir_all(&shims)?;
    fs::create_dir(&native)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&shims, &alias)?;
    #[cfg(windows)]
    {
        let status = platform::process::command(std::env::var_os("COMSPEC").context("COMSPEC")?)
            .args(["/c", "mklink", "/J"])
            .arg(&alias)
            .arg(&shims)
            .stdout(Stdio::null())
            .status()?;
        ensure!(status.success(), "create the fixture directory junction");
    }
    let mut command = platform::process::command("mise");
    command
        .current_dir(scope.path())
        .env("PATH", std::env::join_paths([Path::new("alias"), &native])?)
        .args(["install", "-y"]);
    let install = platform::process::mise_install(command)?;
    let path = install
        .get_envs()
        .find_map(|(key, value)| (key == "PATH").then_some(value).flatten())
        .context("PATH")?;
    assert_eq!(std::env::split_paths(path).collect::<Vec<_>>(), [native]);
    Ok(())
}

#[test]
fn a_runner_cannot_restore_shims_with_its_environment_overrides() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let shims = scope.path().join("mise/shims");
    let native = scope.path().join("native");
    let mut runner = platform::runtime::Real::new(scope.path())?;
    runner
        .environment
        .push(("PATH".into(), std::env::join_paths([&shims, &native])?));
    let install = runner.mise_install(&["-y".into()])?;
    let path = install
        .get_envs()
        .find_map(|(key, value)| (key == "PATH").then_some(value).flatten())
        .context("PATH")?;
    assert_eq!(std::env::split_paths(path).collect::<Vec<_>>(), [native]);
    Ok(())
}

#[test]
fn an_explicitly_removed_path_is_not_inherited_by_the_installer() -> Result<()> {
    let mut command = platform::process::command("mise");
    command.env_remove("PATH").args(["install", "-y"]);
    let install = platform::process::mise_install(command)?;
    assert!(
        install
            .get_envs()
            .any(|(key, value)| key == "PATH" && value == Some(OsStr::new("")))
    );
    Ok(())
}

#[test]
fn installation_paths_remove_duplicates_without_changing_lookup_order() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let first = scope.path().join("first");
    let second = scope.path().join("second");
    let directories: Vec<_> = (0..1_000).flat_map(|_| [&first, &second]).collect();
    let mut command = platform::process::command("mise");
    command.env("PATH", std::env::join_paths(directories)?);
    let install = platform::process::mise_install(command)?;
    let path = install
        .get_envs()
        .find_map(|(key, value)| (key == "PATH").then_some(value).flatten())
        .context("PATH")?;
    assert_eq!(
        std::env::split_paths(path).collect::<Vec<_>>(),
        [first, second]
    );
    Ok(())
}

#[test]
fn a_finalized_install_preserves_tools_added_by_the_parent_mise_activation() -> Result<()> {
    if let Some(scope) = std::env::var_os(ACTIVATION_FIXTURE) {
        return activated_install(Path::new(&scope));
    }
    let scope = tempfile::tempdir()?;
    let root = platform::paths::canonical(scope.path())?;
    let native = root.join("native");
    fs::create_dir(&native)?;
    let project = root.join("parent");
    fs::create_dir(&project)?;
    let cargo = executable(&native, "cargo");
    let compiled = platform::process::command("rustc")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mise_install.rs"))
        .args(["--edition", "2024", "-o"])
        .arg(&cargo)
        .status()?;
    ensure!(compiled.success(), "compile the activation fixture");
    let home = platform::home::home().context("native home")?;
    let mise = platform::code_environment::Installations::discover(&home)?.mise;
    let version = platform::process::command(&mise)
        .arg("--version")
        .output()?;
    ensure!(
        version.status.success(),
        "read the regression's mise version"
    );
    println!(
        "Activation regression: mise {}",
        std::str::from_utf8(&version.stdout)?.trim()
    );
    fs::write(root.join("mise-path"), mise.to_string_lossy().as_bytes())?;
    fs::write(root.join("empty.toml"), "")?;
    fs::write(
        project.join("mise.toml"),
        format!("[env]\n_.path = [{}]\n", serde_json::to_string(&native)?),
    )?;
    let mut parent = platform::process::command(&mise);
    parent.env_clear();
    for name in ["PATH", "SystemRoot", "WINDIR", "COMSPEC", "PATHEXT"] {
        if let Some(value) = std::env::var_os(name) {
            parent.env(name, value);
        }
    }
    for (name, relative) in [
        ("HOME", "home"),
        ("USERPROFILE", "home"),
        ("TEMP", "tmp"),
        ("TMP", "tmp"),
        ("TMPDIR", "tmp"),
        ("MISE_DATA_DIR", "data"),
        ("MISE_CACHE_DIR", "cache"),
        ("MISE_STATE_DIR", "state"),
        ("MISE_CONFIG_DIR", "config"),
        ("MISE_SYSTEM_CONFIG_DIR", "system-config"),
        ("MISE_SYSTEM_DATA_DIR", "system-data"),
        ("MISE_GLOBAL_CONFIG_FILE", "empty.toml"),
        ("MISE_SYSTEM_CONFIG_FILE", "empty.toml"),
        ("SKILL_OPS_STATE", "incidents"),
    ] {
        parent.env(name, root.join(relative));
    }
    for relative in ["home", "tmp", "config"] {
        fs::create_dir(root.join(relative))?;
    }
    let output = parent
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_TRUSTED_CONFIG_PATHS", &root)
        .env("MISE_CEILING_PATHS", &root)
        .env("MISE_OVERRIDE_TOOL_VERSIONS_FILENAMES", "none")
        .env(ACTIVATION_FIXTURE, &root)
        .current_dir(&project)
        .args(["exec", "--"])
        .arg(std::env::current_exe()?)
        .args([
            "--exact",
            "a_finalized_install_preserves_tools_added_by_the_parent_mise_activation",
            "--nocapture",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "activation fixture failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn activated_install(root: &Path) -> Result<()> {
    ensure!(
        std::env::var_os("__MISE_DIFF").is_some(),
        "parent activation metadata"
    );
    let native = root.join("native");
    let shims = root.join("mise/shims");
    fs::create_dir_all(&shims)?;
    let inherited = std::env::var_os("PATH").context("parent activation PATH")?;
    let remaining: Vec<_> = std::env::split_paths(&inherited)
        .filter(|directory| directory != &native)
        .collect();
    let unique = std::env::join_paths(std::iter::once(&native).chain(&remaining))?;
    let repeated = std::env::join_paths([&native, &shims, &native].into_iter().chain(&remaining))?;
    let mise = fs::read_to_string(root.join("mise-path"))?;
    let query = |path: &OsStr| {
        let mut command = platform::process::command(&mise);
        command
            .current_dir(root.join("home"))
            .env("MISE_CONFIG_FILE", root.join("empty.toml"))
            .env("PATH", path)
            .env("DOTFILES_INSTALL_ENV", "finalized")
            .args(["env", "--json"]);
        command
    };
    let legacy = query(&unique).output()?;
    ensure!(legacy.status.success(), "legacy environment query");
    let legacy: serde_json::Value = serde_json::from_slice(&legacy.stdout)?;
    let legacy_path = legacy["PATH"].as_str().context("legacy PATH")?;
    assert!(!std::env::split_paths(OsStr::new(legacy_path)).any(|path| path == native));

    let mut command = query(&repeated);
    command.env_remove("DOTFILES_INSTALL_REMOVED");
    let mut installed = platform::process::mise_install(command)?;
    assert!(installed.get_envs().any(
        |(key, value)| key == "DOTFILES_INSTALL_ENV" && value == Some(OsStr::new("finalized"))
    ));
    assert!(
        installed
            .get_envs()
            .any(|(key, value)| key == "DOTFILES_INSTALL_REMOVED" && value.is_none())
    );
    let output = installed.output()?;
    ensure!(output.status.success(), "finalized environment query");
    let environment: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let path = environment["PATH"].as_str().context("finalized PATH")?;
    let directories: Vec<_> = std::env::split_paths(OsStr::new(path)).collect();
    assert!(
        directories.contains(&native),
        "parent activation removed the finalized Cargo directory"
    );
    assert!(!directories.contains(&shims));
    let state = root.join("cargo-state");
    fs::create_dir(&state)?;
    let cargo = platform::process::command("cargo")
        .env("PATH", path)
        .env("BACKEND_FIXTURE_TOOL", "cargo")
        .env("BACKEND_FIXTURE_STATE", state)
        .arg("--version")
        .output()?;
    ensure!(
        cargo.status.success(),
        "the finalized installer must reach Cargo: {}",
        String::from_utf8_lossy(&cargo.stderr)
    );
    assert_eq!(cargo.stdout, b"fixture-native-cargo\n");
    Ok(())
}

#[cfg(windows)]
#[test]
fn an_installer_refuses_a_path_cmd_would_discard_before_starting() -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use platform::process_rules::CMD_PATH_LIMIT;
    let scope = tempfile::tempdir()?;
    let mut directories = Vec::new();
    let mut path = std::ffi::OsString::new();
    while path.encode_wide().count() <= CMD_PATH_LIMIT {
        directories.push(
            scope
                .path()
                .join(format!("{}-{}", directories.len(), "🦀".repeat(64))),
        );
        path = std::env::join_paths(&directories)?;
    }
    assert!(path.to_string_lossy().chars().count() < CMD_PATH_LIMIT);
    let mut command = platform::process::command("mise");
    command.env("PATH", &path);
    let error = platform::process::mise_install(command).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        error
            .to_string()
            .contains(&path.encode_wide().count().to_string())
    );
    assert!(error.to_string().contains(&CMD_PATH_LIMIT.to_string()));
    Ok(())
}

#[cfg(windows)]
#[test]
fn native_windows_shim_names_are_case_insensitive() -> Result<()> {
    let mut command = platform::process::command("mise");
    let direct = Path::new("C:/fixture/native");
    command.env(
        "Path",
        std::env::join_paths([Path::new("C:/fixture/MISE/SHIMS"), direct])?,
    );
    let install = platform::process::mise_install(command)?;
    let path = install
        .get_envs()
        .find_map(|(key, value)| {
            key.as_encoded_bytes()
                .eq_ignore_ascii_case(b"PATH")
                .then_some(value)
                .flatten()
        })
        .context("PATH")?;
    assert_eq!(std::env::split_paths(path).collect::<Vec<_>>(), [direct]);
    Ok(())
}
