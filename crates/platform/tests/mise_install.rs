use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use platform::runtime::Runner;

const COMPLETION_LIMIT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

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
