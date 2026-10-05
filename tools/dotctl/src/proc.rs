//! Child process helpers.
//!
//! Moves output as bytes, never as text that a shell decoded.
//! The ja-JP console uses code page 932, and decoding through a shell mangles validator input.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// A child's exit code, or 1 for a signal death.
/// `?` alone reports only a failure to start, so the lints reject discarding this value.
#[must_use = "check the exit code with `require` or return it; `?` alone does not"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit(i32);

impl Exit {
    pub fn code(self) -> i32 {
        self.0
    }

    pub fn success(self) -> bool {
        self.0 == 0
    }
}

/// Runs a command with its output passed straight to the terminal and returns its exit code.
pub fn status(cmd: &mut Command) -> Result<Exit> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {}", describe(cmd)))?;
    Ok(Exit(status.code().unwrap_or(1)))
}

/// Runs a command with its output passed straight to the terminal and fails unless it exits with 0.
pub fn run(cmd: &mut Command) -> Result<()> {
    let exit = status(cmd)?;
    if !exit.success() {
        bail!("{} exited with {}", describe(cmd), exit.code());
    }
    Ok(())
}

/// Output of a captured run.
#[derive(Debug)]
pub struct Captured {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Captured {
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    /// stdout as text, with invalid sequences replaced.
    /// A validator that prints code page 932 in an error stays readable and never crashes dotctl.
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    /// Writes captured stdout and stderr to their own streams, unchanged.
    pub fn echo(&self) {
        let _ = std::io::stdout().write_all(&self.stdout);
        let _ = std::io::stderr().write_all(&self.stderr);
        let _ = std::io::stdout().flush();
    }
}

/// Runs a command and captures both streams.
pub fn capture(cmd: &mut Command) -> Result<Captured> {
    let output = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to run {}", describe(cmd)))?;
    Ok(Captured {
        code: output.status.code().unwrap_or(1),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

/// Runs a command that must succeed and returns its stdout as text.
pub fn capture_ok(cmd: &mut Command) -> Result<String> {
    let described = describe(cmd);
    let out = capture(cmd)?;
    if !out.ok() {
        bail!(
            "{described} exited with {}\n{}",
            out.code,
            out.stderr_text().trim_end()
        );
    }
    Ok(out.stdout_text())
}

fn describe(cmd: &Command) -> String {
    let mut parts = vec![cmd.get_program().to_string_lossy().into_owned()];
    parts.extend(cmd.get_args().map(|a| a.to_string_lossy().into_owned()));
    parts.join(" ")
}

/// UTF-8 without a byte order mark, which would change what taplo, git config, and PowerShell read on the first line.
///
/// Closes the handle and keeps only the path, because Windows refuses a second opener while the handle stays open.
/// Dropping the returned value deletes the file.
pub fn write_temp(content: &str, suffix: &str) -> Result<tempfile::TempPath> {
    let mut file = tempfile::Builder::new()
        .prefix("dotctl-")
        .suffix(suffix)
        .tempfile()
        .context("creating a temp file for the rendered content")?;
    file.write_all(content.as_bytes())?;
    file.flush()?;
    Ok(file.into_temp_path())
}

#[cfg(test)]
mod tests {
    use super::{run, status};
    use std::process::Command;

    fn exiting(code: i32) -> Command {
        if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/C", &format!("exit {code}")]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", &format!("exit {code}")]);
            command
        }
    }

    #[test]
    fn a_failing_child_fails_run_and_reports_its_code() {
        assert!(run(&mut exiting(0)).is_ok());
        let error = run(&mut exiting(3)).unwrap_err().to_string();
        assert!(error.contains("exited with 3"), "{error}");
        assert_eq!(status(&mut exiting(3)).unwrap().code(), 3);
    }
}
