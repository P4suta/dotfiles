use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const TEST: &str = "native_installation_retries_after_the_persistent_state_lock_releases";
const ROLE: &str = "SKILL_OPS_CHEZMOI_STATE_HOLDER";
const READY: &str = "chezmoi-holder-ready";
const RELEASE: &str = "chezmoi-holder-release";
const LIMIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);

pub fn block_if_requested() -> Result<bool> {
    let Some(scope) = std::env::var_os(ROLE) else {
        return Ok(false);
    };
    let scope = crate::fixture_process::directory(Path::new(&scope));
    fs::write(scope.join(READY), "ready")?;
    let started = Instant::now();
    while !scope.join(RELEASE).try_exists()? {
        ensure!(
            started.elapsed() < LIMIT,
            "the owned holder was not released"
        );
        std::thread::sleep(POLL);
    }
    Ok(true)
}

pub fn write_script(source: &Path) -> Result<()> {
    let binary = std::env::current_exe()?;
    let binary = binary
        .to_str()
        .context("the fixture executable has a UTF-8 path")?;
    let (name, script) = if cfg!(windows) {
        (
            "run_once_before_state_holder.ps1",
            format!(
                "& '{}' --exact '{TEST}' --nocapture\nexit $LASTEXITCODE\n",
                binary.replace('\'', "''")
            ),
        )
    } else {
        (
            "run_once_before_state_holder.sh",
            format!(
                "#!/bin/sh\nexec '{}' --exact '{TEST}' --nocapture\n",
                binary.replace('\'', "'\\''")
            ),
        )
    };
    fs::write(source.join(name), script)?;
    Ok(())
}

pub fn command(scope: &Path, source: &Path, user: &Path) -> Command {
    let mut command =
        crate::fixture_process::command(platform::tool::Tool::Chezmoi.program(), scope);
    command
        .arg("--source")
        .arg(source)
        .arg("--config")
        .arg(scope.join("chezmoi.toml"))
        .arg("--destination")
        .arg(user)
        .arg("--cache")
        .arg(scope.join("cache"))
        .arg("--persistent-state")
        .arg(scope.join("state.boltdb"))
        .arg("--no-tty")
        .env("HOME", user)
        .env("USERPROFILE", user)
        .env("DOTFILES_EXPECTED_FAILURE", "");
    command
}

pub struct Holder {
    child: Option<Child>,
    scope: PathBuf,
}

impl Holder {
    pub fn start(mut command: Command, scope: &Path) -> Result<Self> {
        let scope = crate::fixture_process::directory(scope);
        let child = command
            .env(ROLE, &scope)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut holder = Self {
            child: Some(child),
            scope,
        };
        let started = Instant::now();
        while !holder.scope.join(READY).try_exists()? {
            if holder
                .child
                .as_mut()
                .context("the holder has a child")?
                .try_wait()?
                .is_some()
            {
                let output = holder.release()?;
                anyhow::bail!(
                    "the holder exited before acquiring its state: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            ensure!(
                started.elapsed() < LIMIT,
                "the owned holder did not become ready"
            );
            std::thread::sleep(POLL);
        }
        Ok(holder)
    }

    pub fn release(&mut self) -> Result<Output> {
        fs::write(self.scope.join(RELEASE), "released")?;
        let started = Instant::now();
        let child = self.child.as_mut().context("the holder has a child")?;
        while child.try_wait()?.is_none() {
            if started.elapsed() >= LIMIT {
                child.kill()?;
                child.wait()?;
                anyhow::bail!("the released holder did not finish within its bound");
            }
            std::thread::sleep(POLL);
        }
        Ok(self
            .child
            .take()
            .context("the completed holder has a child")?
            .wait_with_output()?)
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        if self.child.is_some() {
            platform::cleanup::report(
                self.release(),
                "release and wait for the owned chezmoi holder",
            );
        }
    }
}
