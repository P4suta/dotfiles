use anyhow::{Context, Result, ensure};
use platform::tool::Tool;
use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::process::Command;
use std::process::Output;

#[path = "../../platform/tests/support/fixture_process.rs"]
mod fixture_process;

const RUNTIME: &str = env!("CARGO_BIN_EXE_dotfiles");
const POLICY: &str = include_str!("../../../home/dot_config/dotfiles/bounded-tools.toml");
const IMAGE_ENV: &str = "DOTFILES_BOUNDED_TEST_IMAGE";

#[cfg(unix)]
fn executable(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path.parent().context("fixture directory")?)?;
    fs::write(path, bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(unix)]
fn fixture_command(program: impl AsRef<std::ffi::OsStr>, home: &Path) -> Command {
    let mut command = fixture_process::command(program, home);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let rustup = std::env::var_os("RUSTUP_HOME")
        .or_else(|| platform::home::home().map(|home| home.join(".rustup").into_os_string()));
    command
        .env_clear()
        .env("PATH", path)
        .current_dir(home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("MISE_DATA_DIR", home.join("mise"))
        .env("CARGO_HOME", home.join(".cargo"))
        .env(
            fixture_process::INCIDENT_STATE,
            fixture_process::directory(home).join("state"),
        )
        .env_remove("GITHUB_ACTIONS")
        .env_remove("__MISE_DIFF");
    if let Some(rustup) = rustup {
        command.env("RUSTUP_HOME", rustup);
    }
    command
}

#[cfg(unix)]
#[test]
fn direct_and_project_task_tools_refuse_but_ci_dispatches_the_original() -> Result<()> {
    if !dotfiles::bounded::requires_container() {
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let home = scope.path();
    let bin = home.join("mise/installs/cargo-kani-verifier/0.68.0/bin");
    let tool = bin.join("cargo-kani");
    executable(&tool, b"#!/bin/sh\nprintf '%s\\n' native-fixture-only\n")?;
    let installed = fixture_command(RUNTIME, home)
        .args(["bounded", "install"])
        .env("RUSTUP_HOME", home.join(".rustup"))
        .output()?;
    ensure!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let mut path = vec![bin];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(path)?;
    fs::write(home.join("empty.toml"), b"")?;
    fs::write(
        home.join("mise.toml"),
        format!(
            "[env]\n_.path = [{}]\n[tasks.verify]\nrun = 'cargo kani'\n",
            toml::Value::String(tool.parent().context("fixture bin")?.display().to_string())
        ),
    )?;
    let selected = fixture_command(Tool::Mise.program(), home)
        .args(["exec", "--", "/usr/bin/which", "cargo-kani"])
        .env("PATH", &path)
        .env("MISE_GLOBAL_CONFIG_FILE", home.join("empty.toml"))
        .env("MISE_SYSTEM_CONFIG_FILE", home.join("empty.toml"))
        .env("MISE_TRUSTED_CONFIG_PATHS", home)
        .output()?;
    ensure!(
        selected.status.success()
            && platform::paths::canonical(Path::new(
                String::from_utf8_lossy(&selected.stdout).trim()
            ))? == platform::paths::canonical(&tool)?,
        "mise must select the protected fixture before any test can invoke a verifier: {} {}",
        String::from_utf8_lossy(&selected.stdout),
        String::from_utf8_lossy(&selected.stderr)
    );
    for mut command in [
        fixture_command(&tool, home),
        fixture_command(Tool::Cargo.program(), home),
        fixture_command(Tool::Mise.program(), home),
    ] {
        if command.get_program() == "cargo" {
            command.arg("kani");
        }
        if command.get_program() == "mise" {
            command
                .args(["run", "verify"])
                .env("MISE_GLOBAL_CONFIG_FILE", home.join("empty.toml"))
                .env("MISE_SYSTEM_CONFIG_FILE", home.join("empty.toml"))
                .env("MISE_TRUSTED_CONFIG_PATHS", home);
        }
        let output = command.env("PATH", &path).output()?;
        ensure!(!output.status.success(), "native fixture ran");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        ensure!(diagnostic.contains("dotfiles bounded run"), "{diagnostic}");
        ensure!(
            !String::from_utf8_lossy(&output.stdout).contains("native-fixture-only"),
            "guard ran the native tool"
        );
    }
    let output = fixture_command(&tool, home)
        .env("GITHUB_ACTIONS", "true")
        .output()?;
    ensure!(output.status.success(), "CI fixture refused");
    ensure!(
        String::from_utf8_lossy(&output.stdout).contains("native-fixture-only"),
        "CI did not dispatch its original"
    );
    Ok(())
}

#[derive(Debug)]
struct KeptContainer(String);

impl Drop for KeptContainer {
    fn drop(&mut self) {
        let result = Tool::Docker
            .command()
            .args(["rm", "--force", &self.0])
            .output()
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    Err(std::io::Error::other(String::from_utf8_lossy(
                        &output.stderr,
                    )))
                }
            });
        platform::cleanup::report(result, "remove the bounded integration container");
    }
}

fn bounded(
    image: &str,
    root: &Path,
    policy: Option<&Path>,
    arguments: &[&str],
) -> Result<(KeptContainer, Output)> {
    let mut command = fixture_process::command(RUNTIME, root);
    command
        .args(["bounded", "run", "--image", image, "--keep", "--project"])
        .arg(root);
    if let Some(policy) = policy {
        command.arg("--policy").arg(policy);
    }
    let output = command.arg("--").args(arguments).output()?;
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    let id = diagnostic
        .lines()
        .find_map(|line| {
            line.strip_prefix("Bounded container ")
                .and_then(|line| line.split_once(':'))
                .map(|(id, _)| id.to_owned())
        })
        .with_context(|| format!("no bounded container: {diagnostic}"))?;
    Ok((KeptContainer(id), output))
}

#[test]
#[ignore = "requires Docker and DOTFILES_BOUNDED_TEST_IMAGE with rustc, a C linker, and GNU timeout"]
fn docker_inspect_confirms_limits_and_oom_and_timeout_are_inconclusive() -> Result<()> {
    let image = std::env::var(IMAGE_ENV).context(IMAGE_ENV)?;
    let root = tempfile::tempdir()?;
    fs::write(
        root.path().join("memory.rs"),
        include_bytes!("fixtures/bounded-memory.rs"),
    )?;
    let (_build, compiled) = bounded(
        &image,
        root.path(),
        None,
        &["rustc", "/work/memory.rs", "-o", "/work/allocate"],
    )?;
    ensure!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let policy = root.path().join("bounded.toml");
    fs::write(
        &policy,
        POLICY
            .replace("memory_mib = 2048", "memory_mib = 64")
            .replace("seconds = 900", "seconds = 2"),
    )?;
    let (memory, output) = bounded(&image, root.path(), Some(&policy), &["/work/allocate"])?;
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    ensure!(
        !output.status.success() && diagnostic.contains("exceeded memory limit 67108864 bytes"),
        "{diagnostic}"
    );
    let inspect = Tool::Docker
        .command()
        .args(["inspect", &memory.0])
        .output()?;
    let inspected: Vec<serde_json::Value> = serde_json::from_slice(&inspect.stdout)?;
    let container = inspected.first().context("Docker inspection")?;
    let host = &container["HostConfig"];
    ensure!(
        host["Memory"] == 67_108_864 && host["MemorySwap"] == 67_108_864,
        "memory limits differ"
    );
    ensure!(
        host["NanoCpus"] == 2_000_000_000_u64 && host["PidsLimit"] == 256,
        "CPU or PID limit differs"
    );
    ensure!(
        container["State"]["OOMKilled"] == true,
        "the kernel did not identify the memory kill"
    );
    ensure!(
        container["Config"]["Entrypoint"][0] == "/usr/bin/timeout"
            && container["Config"]["Cmd"][1] == "2s",
        "deadline differs"
    );
    let (_timeout, output) = bounded(
        &image,
        root.path(),
        Some(&policy),
        &[
            "bash",
            "-c",
            "printf 'proof-diagnostic-before-stop\\n'; exec sleep 20",
        ],
    )?;
    ensure!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains("time limit 2s"),
        "timeout supplied passing evidence"
    );
    ensure!(
        String::from_utf8_lossy(&output.stdout).contains("proof-diagnostic-before-stop"),
        "limit stop discarded the container diagnostic"
    );
    Ok(())
}
