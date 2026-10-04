//! Child process helpers.
//!
//! Output is moved as bytes, never as text decoded by an intermediate shell.
//! The ja-JP console code page is CP932, and the PowerShell pipeline this crate replaces had to force UTF-8 on decode, encode and file defaults to stop validators receiving mangled input.
//! Writing bytes straight through removes the class of bug rather than configuring around it.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Runs a command, letting its output reach the terminal untouched.
///
/// Returns the exit code, or 1 for a signal death, so a caller can aggregate exit codes the way the shell wrappers did.
pub fn status(cmd: &mut Command) -> Result<i32> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {}", describe(cmd)))?;
    Ok(status.code().unwrap_or(1))
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

    /// stdout as text, with invalid sequences replaced rather than rejected:
    /// a validator that emits CP932 in an error message should still be readable, and should not turn into a dotctl crash.
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

/// Runs a command that is expected to succeed, returning its stdout as text.
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

/// UTF-8, no BOM: a BOM changes what taplo, git config and PowerShell see on the first line.
///
/// The handle is closed before the path is handed out, and only the path is kept: Windows refuses a second opener while we hold the file, which shows up as PSScriptAnalyzer reporting that the file is in use by another process.
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
