use anyhow::{Context, Result, bail, ensure};
use std::fs::File;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::Path;

pub use crate::review_rules::{
    DAILY_LIMIT, DAY_MS, HOUR_MS, HOURLY_LIMIT, PROBE_DAILY_LIMIT, PROBE_GAP_MS,
};
use crate::review_rules::{
    cooldown_complete, included_allowance, rolling_allowance, service_access,
};

#[derive(Debug)]
pub struct LedgerLock(File);

impl Deref for LedgerLock {
    type Target = File;

    fn deref(&self) -> &File {
        &self.0
    }
}

impl DerefMut for LedgerLock {
    fn deref_mut(&mut self) -> &mut File {
        &mut self.0
    }
}

impl Drop for LedgerLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Debug, PartialEq)]
pub enum Invocation {
    ReadOnly,
    Review,
    Forbidden,
}

fn first_command(args: &[String]) -> Option<&str> {
    let mut args = args.iter().peekable();
    while let Some(arg) = args.next() {
        if !arg.starts_with('-') {
            return Some(arg);
        }
        let (flag, inline_value) = arg
            .split_once('=')
            .map_or((arg.as_str(), false), |(flag, _)| (flag, true));
        match flag {
            "-c" | "--config" => {
                while args.peek().is_some_and(|value| !value.starts_with('-')) {
                    args.next();
                }
            }
            "--deep" if !inline_value => {
                if args.peek().is_some_and(|value| !value.starts_with('-')) {
                    args.next();
                }
            }
            "--dir" | "--base" | "--base-commit" | "--source-branch" | "--region"
                if !inline_value =>
            {
                args.next();
            }
            _ => {}
        }
    }
    None
}

pub fn classify(args: &[String]) -> Invocation {
    if args.iter().any(|arg| {
        arg == "--use-credits"
            || arg.starts_with("--use-credits=")
            || arg == "--api-key"
            || arg.starts_with("--api-key=")
            || arg == "--remote"
            || arg.starts_with("--remote=")
    }) || matches!(first_command(args), Some("code" | "handoff" | "update"))
    {
        return Invocation::Forbidden;
    }
    if usage_query(args)
        || matches!(args, [arg] if matches!(arg.as_str(), "--version" | "-V" | "--help" | "-h"))
        || args.first().is_some_and(|arg| {
            matches!(
                arg.as_str(),
                "auth" | "doctor" | "stats" | "usage" | "pullrequest" | "skills"
            )
        })
        || matches!(args, [command, subcommand, ..] if command == "review" && subcommand == "findings")
        || matches!(args, [command, flag] if command == "review" && matches!(flag.as_str(), "--show-prompts" | "--usage" | "--help" | "-h"))
    {
        Invocation::ReadOnly
    } else {
        Invocation::Review
    }
}

pub fn check_budget(records: &[u64], now: u64) -> Result<()> {
    ensure!(
        records.iter().all(|at| *at <= now),
        "clock moved backwards; review refused"
    );
    let hourly = records.iter().filter(|at| now - **at < HOUR_MS).count();
    let daily = records.iter().filter(|at| now - **at < DAY_MS).count();
    ensure!(
        rolling_allowance(hourly, daily),
        "local CodeRabbit limit reached ({HOURLY_LIMIT}/rolling hour, {DAILY_LIMIT}/rolling day); wait for history to leave the window and recheck usage without bypassing the guard"
    );
    Ok(())
}

pub fn check_usage(text: &str, organization: &str) -> Result<()> {
    let events: Vec<serde_json::Value> = text
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()
        .context("invalid CodeRabbit usage response")?;
    let mut completed = events
        .iter()
        .filter(|event| event["phase"] == "usage" && event["status"] == "complete");
    let usage = completed
        .next()
        .context("included CodeRabbit allowance is unavailable; review refused")?;
    ensure!(
        completed.next().is_none(),
        "ambiguous usage response; review refused"
    );
    ensure!(
        usage["type"] == "status" && usage["organization"] == organization,
        "CodeRabbit organization does not match the guarded account"
    );
    let included = &usage["includedReviews"];
    let limit = included["limit"]
        .as_u64()
        .context("missing included review limit")?;
    let remaining = included["remaining"]
        .as_u64()
        .context("missing included review allowance")?;
    ensure!(
        included["state"] == "available"
            && included["rollingWindowMs"] == HOUR_MS
            && included_allowance(limit, remaining),
        "insufficient verified included allowance; consumption is limited to floor(capacity * 0.9 * 0.8)"
    );
    let billing = &usage["billingPeriod"];
    ensure!(
        billing["state"] == "available"
            && billing["usageBillingStatus"] == "inactive"
            && billing["scope"] == "user"
            && billing["user"] == organization,
        "paid review usage must be inactive; review refused"
    );
    Ok(())
}

pub fn review_directory(args: &[String]) -> Result<Option<std::path::PathBuf>> {
    let mut directory = None;
    for (index, arg) in args.iter().enumerate() {
        let value = if arg == "--dir" {
            Some(
                args.get(index + 1)
                    .context("--dir requires a directory")?
                    .as_str(),
            )
        } else {
            arg.strip_prefix("--dir=")
        };
        if let Some(value) = value {
            ensure!(
                directory.is_none() && !value.is_empty() && !value.starts_with('-'),
                "review directory must be unambiguous"
            );
            directory = Some(value.into());
        }
    }
    Ok(directory)
}

pub fn records(text: &str) -> Result<Vec<u64>> {
    ensure!(
        text.is_empty() || text.ends_with('\n'),
        "truncated review ledger; refusing to reset allowance"
    );
    let records: Vec<u64> = text
        .lines()
        .map(|line| line.parse())
        .collect::<std::result::Result<_, _>>()
        .context("corrupt review ledger; refusing to reset allowance")?;
    ensure!(
        records.windows(2).all(|pair| pair[0] <= pair[1]),
        "unsorted review ledger; review refused"
    );
    Ok(records)
}

pub fn lock_ledger(path: &Path) -> Result<(LedgerLock, Vec<u64>)> {
    let metadata = std::fs::symlink_metadata(path).context(
        "review ledger is missing; inspect the guard installation without resetting history",
    )?;
    ensure!(
        metadata.is_file() && !metadata.is_symlink(),
        "review ledger must be a regular file"
    );
    ensure!(
        metadata.len() <= 1_048_576,
        "review ledger exceeds its size bound; operator maintenance is required"
    );
    let file = File::options().read(true).append(true).open(path)?;
    file.try_lock().map_err(|error| {
        anyhow::anyhow!(
            "another CodeRabbit invocation holds the review ledger, or locking failed: {error}"
        )
    })?;
    let mut file = LedgerLock(file);
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok((file, records(&text)?))
}

pub fn record(file: &mut File, now: u64) -> Result<()> {
    writeln!(file, "{now}")?;
    file.sync_all()
        .context("persist CodeRabbit review reservation before launch")
}

pub fn reserve(path: &Path, now: u64) -> Result<LedgerLock> {
    let (mut file, records) = lock_ledger(path)?;
    check_budget(&records, now)?;
    record(&mut file, now)?;
    Ok(file)
}

pub fn execute_probe<T>(ledger: &Path, now: u64, probe: impl FnOnce() -> Result<T>) -> Result<T> {
    let (mut file, history) = lock_ledger(ledger)?;
    ensure!(
        history.iter().all(|at| *at <= now),
        "clock moved backwards; usage query refused"
    );
    ensure!(
        history
            .last()
            .is_none_or(|at| cooldown_complete(*at, now, PROBE_GAP_MS)),
        "usage query cooldown has not elapsed; do not poll or retry"
    );
    ensure!(
        rolling_allowance(
            history.iter().filter(|at| now - **at < HOUR_MS).count(),
            history.iter().filter(|at| now - **at < DAY_MS).count(),
        ),
        "usage-query budget exhausted ({HOURLY_LIMIT}/rolling hour, {PROBE_DAILY_LIMIT}/rolling day); do not retry"
    );
    record(&mut file, now)?;
    probe()
}

pub fn usage_query(args: &[String]) -> bool {
    first_command(args) == Some("usage")
        || (matches!(first_command(args), None | Some("review"))
            && args.iter().any(|arg| arg == "--usage"))
}

pub fn execute_reserved(
    ledger: &Path,
    probes: &Path,
    now: u64,
    organization: &str,
    usage: impl FnOnce() -> Result<String>,
    review: impl FnOnce() -> Result<i32>,
) -> Result<i32> {
    let _reservation = reserve(ledger, now)?;
    check_usage(&execute_probe(probes, now, usage)?, organization)?;
    review()
}

fn initialize_ledger(user_directory: &Path, marker: &str, name: &str) -> Result<LedgerLock> {
    let state = user_directory.join(".local/state/coderabbit-guard");
    std::fs::create_dir_all(&state)?;
    let initialized = state.join(marker);
    let ledger = state.join(name);
    if !initialized.try_exists()? && !ledger.try_exists()? {
        File::options()
            .write(true)
            .create_new(true)
            .open(&ledger)?
            .sync_all()?;
    }
    let (locked, _history) = lock_ledger(&ledger)?;
    if !initialized.try_exists()? {
        locked.sync_all()?;
        #[cfg(unix)]
        let state_directory = File::open(&state)?;
        #[cfg(unix)]
        state_directory
            .sync_all()
            .context("persist review ledger directory before initialization")?;
        #[cfg(unix)]
        for directory in [
            user_directory.join(".local/state"),
            user_directory.join(".local"),
            user_directory.to_path_buf(),
            user_directory.join(".."),
        ] {
            File::open(directory)?
                .sync_all()
                .context("persist review guard state directory ancestry")?;
        }
        File::options()
            .write(true)
            .create_new(true)
            .open(&initialized)?
            .sync_all()?;
        #[cfg(unix)]
        state_directory
            .sync_all()
            .context("persist initialized review guard directory")?;
    }
    Ok(locked)
}

pub fn install(binary: &Path, user_directory: &Path) -> Result<()> {
    use std::fs;
    ensure!(binary.is_file(), "built review guard is missing");
    let _reviews = initialize_ledger(user_directory, "initialized", "reviews.log")?;
    let _probes = initialize_ledger(user_directory, "usage-initialized", "usage.log")?;
    let directory = user_directory.join(".local/bin");
    fs::create_dir_all(&directory)?;
    for name in ["coderabbit", "cr"] {
        let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        let temporary = directory.join(format!(".{name}.{}.install", std::process::id()));
        File::options()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> Result<()> {
            fs::copy(binary, &temporary)?;
            File::options().write(true).open(&temporary)?.sync_all()?;
            fs::rename(&temporary, directory.join(name))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
    }
    #[cfg(unix)]
    for path in [directory, user_directory.join(".local")] {
        File::open(path)?
            .sync_all()
            .context("persist installed review guard wrappers")?;
    }
    Ok(())
}

pub fn validate_policy(text: &str) -> Result<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let object = value
        .as_object()
        .context("review guard policy must be an object")?;
    ensure!(object.len() == 3, "unknown review guard policy field");
    ensure!(
        value["review_os"] == "macos",
        "the managed review executor must be macOS"
    );
    let organization = value["organization"]
        .as_str()
        .context("guarded organization is required")?;
    let version = value["vendor_version"]
        .as_str()
        .context("vendor version is required")?;
    ensure!(
        !organization.is_empty()
            && version.split('.').count() == 3
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "invalid review guard policy"
    );
    Ok((organization.into(), version.into()))
}

pub fn now_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

pub fn run(args: Vec<String>, user_directory: &Path) -> Result<i32> {
    use std::process::Command;
    let (organization, version) = validate_policy(&std::fs::read_to_string(
        user_directory.join(".config/coderabbit-guard/policy.json"),
    )?)?;
    let ledger = user_directory.join(".local/state/coderabbit-guard/reviews.log");
    let paused = user_directory
        .join(".local/state/coderabbit-guard/paused")
        .try_exists()?;
    ensure!(
        service_access(paused, args == ["--guard-status"]),
        "CodeRabbit use is paused by the owner; explicit resume is required"
    );
    if args == ["--guard-status"] {
        let (_file, history) = lock_ledger(&ledger)?;
        let now = now_ms()?;
        ensure!(
            history.last().is_none_or(|at| *at <= now),
            "clock moved backwards"
        );
        println!(
            "CodeRabbit guard: {}/{} in rolling hour; {}/{} in rolling day; organization {organization}; paused {paused}",
            history.iter().filter(|at| now - **at < HOUR_MS).count(),
            HOURLY_LIMIT,
            history.iter().filter(|at| now - **at < DAY_MS).count(),
            DAILY_LIMIT
        );
        return Ok(0);
    }
    let kind = classify(&args);
    if kind == Invocation::Forbidden {
        bail!(
            "paid reviews, inline API keys, remote scopes, cloud coding, and self-update are disabled by the CodeRabbit guard"
        );
    }
    if kind == Invocation::Review || usage_query(&args) {
        ensure!(
            std::env::consts::OS == "macos",
            "managed service reviews and usage queries run only on the Mac; other hosts perform native local checks"
        );
    }
    let location = Command::new("mise")
        .args(["where", &format!("http:coderabbit@{version}")])
        .output()
        .context("locate the dotfiles-managed CodeRabbit runtime")?;
    ensure!(
        location.status.success(),
        "mise cannot locate the pinned CodeRabbit runtime"
    );
    let directory = std::path::PathBuf::from(std::str::from_utf8(&location.stdout)?.trim());
    ensure!(
        directory.is_absolute(),
        "mise returned a relative runtime path"
    );
    let vendor = directory.join(if cfg!(windows) {
        "coderabbit-vendor.exe"
    } else {
        "coderabbit-vendor"
    });
    ensure!(
        vendor.is_file(),
        "CodeRabbit vendor runtime must be renamed by mise before using the guard"
    );
    if kind == Invocation::ReadOnly {
        if usage_query(&args) {
            return execute_probe(
                &user_directory.join(".local/state/coderabbit-guard/usage.log"),
                now_ms()?,
                || {
                    Ok(Command::new(vendor)
                        .args(args)
                        .env("CI", "true")
                        .status()?
                        .code()
                        .unwrap_or(1))
                },
            );
        }
        return Ok(Command::new(vendor)
            .args(args)
            .status()?
            .code()
            .unwrap_or(1));
    }
    ensure!(
        std::env::var_os("CODERABBIT_API_KEY").is_none(),
        "inline authentication cannot bypass the guarded account"
    );
    let runtime = Command::new(&vendor).arg("--version").output()?;
    ensure!(
        runtime.status.success() && std::str::from_utf8(&runtime.stdout)?.trim() == version,
        "CodeRabbit runtime has drifted from its pinned version; review refused"
    );
    let directory = review_directory(&args)?;
    execute_reserved(
        &ledger,
        &user_directory.join(".local/state/coderabbit-guard/usage.log"),
        now_ms()?,
        &organization,
        || {
            let mut command = Command::new(&vendor);
            command.args(["usage", "--agent"]).env("CI", "true");
            if let Some(directory) = directory {
                command.current_dir(directory);
            }
            let usage = command.output()?;
            ensure!(
                usage.status.success(),
                "included review allowance could not be verified; no review started"
            );
            Ok(String::from_utf8(usage.stdout)?)
        },
        || {
            let status = Command::new(&vendor)
                .args(args)
                .env("CI", "true")
                .status()
                .context("launch reserved CodeRabbit invocation")?;
            Ok(status.code().unwrap_or(1))
        },
    )
}
