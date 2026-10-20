//! Codex hook execution evidence that preserves child streams and decisions.

use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::num::NonZeroUsize;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const LOG: &str = ".local/state/dotfiles/codex-hooks.jsonl";
const MAX_LOG_BYTES: u64 = 1 << 20;
const RETAIN_RECORDS: usize = 128;
const DIAGNOSTIC_BYTES: usize = 4096;
pub const SHELL_CONTEXT: &str = "DOTFILES_CODEX_HOOK_SHELL";
pub const POWERSHELL: &str = "powershell";

#[derive(Debug, Deserialize)]
struct HookIdentity {
    session_id: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Request {
    /// Execute one hook and record its start, child status, and standard error.
    Run {
        #[arg(long)]
        name: String,
        #[arg(last = true, required = true)]
        command: Vec<OsString>,
    },
    /// Print recent execution records.
    /// Missing evidence returns an error.
    Logs {
        #[arg(long, default_value = "20")]
        limit: NonZeroUsize,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
enum Phase<'a> {
    Started,
    Exited {
        code: Option<i32>,
        stderr: &'a str,
        stderr_truncated: bool,
        input_error: Option<String>,
    },
    SpawnFailed {
        error: String,
    },
}

#[derive(Debug, Serialize)]
struct Record<'a> {
    id: &'a str,
    timestamp_ms: u128,
    name: &'a str,
    thread_id: Option<&'a str>,
    #[serde(flatten)]
    phase: Phase<'a>,
}

fn append(path: &Path, record: &Record<'_>) -> Result<()> {
    std::fs::create_dir_all(path.parent().context("hook log directory")?)?;
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    file.lock()?;
    if file.metadata()?.len() >= MAX_LOG_BYTES {
        let mut previous = String::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_string(&mut previous)?;
        let mut retained: Vec<_> = previous.lines().rev().take(RETAIN_RECORDS).collect();
        retained.reverse();
        let mut text = retained.join("\n");
        text.push('\n');
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(text.as_bytes())?;
    }
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    file.seek(SeekFrom::End(0))?;
    file.write_all(&line)?;
    Ok(())
}

fn record(path: &Path, id: &str, name: &str, thread_id: Option<&str>, phase: Phase<'_>) {
    let result = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("hook log clock")
        .and_then(|time| {
            append(
                path,
                &Record {
                    id,
                    timestamp_ms: time.as_millis(),
                    name,
                    thread_id,
                    phase,
                },
            )
        });
    if let Err(error) = result {
        eprintln!("dotfiles: hook execution evidence unavailable: {error:#}");
    }
}

fn capture(
    path: &Path,
    name: &str,
    command: &mut Command,
    input: Vec<u8>,
    thread_id: Option<&str>,
) -> Result<Output> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let id = format!("{}-{}", now.as_nanos(), std::process::id());
    record(path, &id, name, thread_id, Phase::Started);
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            crate::hook_failures::record(name, &error.to_string());
            record(
                path,
                &id,
                name,
                thread_id,
                Phase::SpawnFailed {
                    error: error.to_string(),
                },
            );
            return Err(error).context("start hook child");
        }
    };
    let mut stdin = child.stdin.take().context("hook child standard input")?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().context("wait for hook child")?;
    let input_error = writer
        .join()
        .map_err(|_| anyhow::anyhow!("hook input writer panicked"))?
        .err()
        .map(|error| error.to_string());
    let diagnostic = output
        .stderr
        .get(..DIAGNOSTIC_BYTES)
        .unwrap_or(&output.stderr);
    // A child that a signal ended can't record its own failure, and a `dotfiles` child records every other one.
    if output.status.code().is_none() {
        crate::hook_failures::record(name, &format!("the hook ended with {}", output.status));
    }
    record(
        path,
        &id,
        name,
        thread_id,
        Phase::Exited {
            code: output.status.code(),
            stderr: &String::from_utf8_lossy(diagnostic),
            stderr_truncated: output.stderr.len() > DIAGNOSTIC_BYTES,
            input_error,
        },
    );
    Ok(output)
}

fn logs(path: &Path, limit: NonZeroUsize) -> Result<()> {
    let file = File::open(path)
        .with_context(|| format!("no hook execution evidence at {}", path.display()))?;
    file.lock_shared()?;
    let mut latest = std::collections::VecDeque::new();
    for line in BufReader::new(file).lines() {
        latest.push_back(line?);
        if latest.len() > limit.get() {
            latest.pop_front();
        }
    }
    ensure!(!latest.is_empty(), "no hook executions observed");
    for line in latest {
        println!("{line}");
    }
    Ok(())
}

/// Execute or inspect locally recorded hook runs.
///
/// # Errors
///
/// Returns an error for unavailable execution evidence, invalid process arguments, or failed process and stream operations.
pub fn run(request: Request) -> Result<i32> {
    let home = platform::home::home().context("native home unavailable")?;
    let path = home.join(LOG);
    match request {
        Request::Run { name, command } => {
            let (program, arguments) = command.split_first().context("hook command missing")?;
            let mut child = platform::process::command(program);
            child.args(arguments).env_remove(SHELL_CONTEXT);
            if cfg!(windows) {
                child.env(SHELL_CONTEXT, POWERSHELL);
            }
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input)?;
            let thread_id = serde_json::from_slice::<HookIdentity>(&input)
                .ok()
                .and_then(|identity| identity.session_id)
                .or_else(|| std::env::var("CODEX_THREAD_ID").ok());
            let output = capture(&path, &name, &mut child, input, thread_id.as_deref())?;
            std::io::stdout().write_all(&output.stdout)?;
            std::io::stderr().write_all(&output.stderr)?;
            Ok(output.status.code().unwrap_or(1))
        }
        Request::Logs { limit } => {
            logs(&path, limit)?;
            Ok(0)
        }
    }
}
