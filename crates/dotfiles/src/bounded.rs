//! Resource policy and the one lifecycle for bounded verification containers.

use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use platform::tool::Tool;
use rules::bounded_rules::{self, Completion, Limits};
use serde::Deserialize;
use std::ffi::OsString;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Output};

mod guard;
pub use guard::requires_container;
pub use guard::{dispatch, install};

pub const POLICY: &str = ".config/dotfiles/bounded-tools.toml";
const DEFAULT_POLICY: &str = include_str!("../../../home/dot_config/dotfiles/bounded-tools.toml");
const POLICY_VERSION: u8 = 1;
const WORKDIR: &str = "/work";
const TIMEOUT: &str = "/usr/bin/timeout";
const LOG_BYTES: &str = "1m";
const LOG_LINES: &str = "4096";
const DIGEST_PREFIX: &str = "sha256:";
const DIGEST_LENGTH: usize = 64;
#[cfg(unix)]
const WAIT_STATUS_SHIFT: u32 = 8;
const TIMEOUT_EXIT: i32 = 124;
const KILLED_EXIT: i32 = 137;
const MAX_EXIT_CODE: i32 = 255;
const LIMIT_OPTIONS: [&str; 4] = ["--memory", "--memory-swap", "--cpus", "--pids-limit"];

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Run a project command in an immutable Linux image under the resource policy.
    Run {
        #[arg(long)]
        image: String,
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        policy: Option<PathBuf>,
        /// Keep this run's stopped container for inspection.
        #[arg(long)]
        keep: bool,
        #[arg(last = true, required = true)]
        command: Vec<OsString>,
    },
    /// Protect existing managed tools and tools newly installed by mise.
    Install,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    tools: Vec<String>,
    limits: RawLimits,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLimits {
    memory_mib: u64,
    swap_mib: u64,
    cpus: u16,
    pids: u32,
    seconds: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    pub tools: Vec<String>,
    pub limits: Limits,
}

impl Policy {
    /// Validate policy data before it can construct a container or select executables.
    ///
    /// # Errors
    ///
    /// Refuses unknown fields, versions, empty or unsafe tool names, duplicates, zero limits, and overflow.
    pub fn parse(text: &str) -> Result<Self> {
        let document: Document = toml::from_str(text).context("parse bounded tool policy")?;
        ensure!(
            document.version == POLICY_VERSION,
            "unsupported bounded policy version"
        );
        ensure!(!document.tools.is_empty(), "bounded policy lists no tools");
        let mut unique = std::collections::BTreeSet::new();
        for tool in &document.tools {
            ensure!(
                !tool.is_empty()
                    && tool
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
                "invalid bounded tool name {tool:?}"
            );
            ensure!(unique.insert(tool), "duplicate bounded tool {tool}");
        }
        let raw = document.limits;
        let limits = bounded_rules::limits(
            raw.memory_mib,
            raw.swap_mib,
            raw.cpus,
            raw.pids,
            raw.seconds,
        )
        .context("bounded limits must be positive and fit Docker's finite resource ranges")?;
        Ok(Self {
            tools: document.tools,
            limits,
        })
    }

    /// Read the owner's installed policy, using the compiled source policy before first installation.
    ///
    /// # Errors
    ///
    /// Returns policy read or validation failures rather than falling back from an invalid policy.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        if let Some(path) = path {
            return Self::parse(&fs::read_to_string(path)?);
        }
        let installed = platform::home::home().map(|home| home.join(POLICY));
        match installed.as_deref().map(fs::read_to_string) {
            Some(Ok(text)) => Self::parse(&text),
            Some(Err(error)) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            Some(Err(_)) | None => Self::parse(DEFAULT_POLICY),
        }
    }

    #[must_use]
    pub fn resource_arguments(&self) -> Vec<OsString> {
        let limits = self.limits;
        [
            "--memory".to_owned(),
            limits.memory.to_string(),
            "--memory-swap".to_owned(),
            limits.memory_swap.to_string(),
            "--cpus".to_owned(),
            limits.cpus.to_string(),
            "--pids-limit".to_owned(),
            limits.pids.to_string(),
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }
}

/// The source policy's resource flags for inspectable project container plans.
///
/// # Panics
///
/// Panics if the compiled source policy is invalid, which the policy tests reject.
#[must_use]
pub fn resource_arguments() -> Vec<OsString> {
    Policy::parse(DEFAULT_POLICY)
        .expect("the checked source policy is valid")
        .resource_arguments()
}

pub(crate) fn pinned(image: &str) -> bool {
    let digest = image
        .strip_prefix(DIGEST_PREFIX)
        .or_else(|| image.rsplit_once("@sha256:").map(|(_, digest)| digest));
    digest.is_some_and(|digest| {
        digest.len() == DIGEST_LENGTH && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn create_arguments(policy: &Policy, arguments: &[OsString], image: &str) -> Result<Vec<OsString>> {
    ensure!(
        pinned(image),
        "bounded runs require an immutable image ID or name@sha256:digest"
    );
    ensure!(
        arguments.first().is_some_and(|argument| argument == "run"),
        "expected Docker run arguments"
    );
    let image_index = arguments
        .iter()
        .position(|argument| argument == image)
        .context("container plan omits its image")?;
    ensure!(
        image_index + 1 < arguments.len(),
        "bounded run requires an explicit command"
    );
    let mut result = vec![OsString::from("create")];
    let mut options = arguments[1..image_index].iter();
    while let Some(option) = options.next() {
        if option == "--rm" {
            continue;
        }
        if LIMIT_OPTIONS.iter().any(|limit| option == limit) {
            options.next().context("resource option lacks a value")?;
            continue;
        }
        ensure!(
            !LIMIT_OPTIONS
                .iter()
                .any(|limit| option.to_string_lossy().starts_with(&format!("{limit}="))),
            "resource options must use separate values"
        );
        ensure!(
            option != "--entrypoint"
                && option != "--privileged"
                && !option.to_string_lossy().starts_with("--entrypoint="),
            "bounded runner owns the container entrypoint and resource boundary"
        );
        result.push(option.clone());
    }
    result.extend(policy.resource_arguments());
    result.extend(
        [
            "--log-driver",
            "local",
            "--log-opt",
            &format!("max-size={LOG_BYTES}"),
            "--log-opt",
            "max-file=1",
            "--log-opt",
            "compress=false",
            "--entrypoint",
            TIMEOUT,
            image,
            "--kill-after=1s",
            &format!("{}s", policy.limits.seconds),
        ]
        .map(OsString::from),
    );
    result.extend_from_slice(&arguments[image_index + 1..]);
    Ok(result)
}

#[derive(Debug)]
struct Container {
    id: String,
    keep: bool,
}

impl Drop for Container {
    fn drop(&mut self) {
        if !self.keep {
            platform::cleanup::report(
                docker(&["rm", "--force", &self.id]).map(|_| ()),
                format_args!("remove bounded container {}", self.id),
            );
        }
    }
}

fn docker(arguments: &[&str]) -> Result<Output> {
    let output = Tool::Docker
        .command()
        .args(arguments)
        .output()
        .context("start Docker for bounded verification")?;
    ensure!(
        output.status.success(),
        "Docker {} failed: {}",
        arguments.first().unwrap_or(&"command"),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct State {
    #[serde(rename = "OOMKilled")]
    oom_killed: bool,
    exit_code: i32,
    error: String,
}

fn exit_status(code: i32) -> Result<ExitStatus> {
    ensure!(
        (0..=MAX_EXIT_CODE).contains(&code),
        "Docker returned an invalid container exit code {code}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        Ok(ExitStatus::from_raw(code << WAIT_STATUS_SHIFT))
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        Ok(ExitStatus::from_raw(code.cast_unsigned()))
    }
}

/// Execute an existing project container plan under the same policy as the public runner.
///
/// # Errors
///
/// Refuses mutable images and invalid plans, reports Docker failures, and leaves limit stops inconclusive.
pub fn run(arguments: &[OsString], image: &str) -> Result<Output> {
    run_with_policy(&Policy::load(None)?, arguments, image)
}

/// Execute a project plan under an explicit validated policy without changing the installed policy.
///
/// # Errors
///
/// Refuses invalid plans and mutable images, and reports resource stops as inconclusive.
pub fn run_with_policy(policy: &Policy, arguments: &[OsString], image: &str) -> Result<Output> {
    run_with(policy, arguments, image, false)
}

/// Run a proof campaign with its inventory-derived safety cap and the installed resource limits.
///
/// # Errors
///
/// Refuses an empty, invalid, or overflowing inventory and propagates container failures.
pub fn run_campaign(
    arguments: &[OsString],
    image: &str,
    harnesses: u32,
    parallelism: u16,
    overhead: u32,
) -> Result<Output> {
    let mut policy = Policy::load(None)?;
    policy.limits.seconds =
        rules::proof_budget_rules::campaign_seconds(harnesses, parallelism, overhead)
            .context("proof campaign requires a nonempty finite inventory and parallelism")?;
    run_with(&policy, arguments, image, false)
}

/// Recognize a project Docker run and route its immutable image through the bounded lifecycle.
///
/// # Errors
///
/// Refuses malformed plans or returns the bounded execution failure.
pub fn command_output(command: &std::process::Command) -> Result<Option<Output>> {
    if Path::new(command.get_program()).file_stem() != Some(std::ffi::OsStr::new("docker")) {
        return Ok(None);
    }
    let arguments: Vec<OsString> = command.get_args().map(std::ffi::OsStr::to_owned).collect();
    if arguments.first().is_none_or(|argument| argument != "run") {
        return Ok(None);
    }
    let mut options = arguments.iter().skip(1);
    while let Some(argument) = options.next() {
        let text = argument.to_str().context("Docker option is not UTF-8")?;
        if !text.starts_with('-') {
            return run(&arguments, text).map(Some);
        }
        if !text.contains('=')
            && !matches!(text, "--rm" | "--init" | "--read-only" | "--privileged")
        {
            options.next().context("Docker option lacks its value")?;
        }
    }
    anyhow::bail!("Docker run omits an immutable image")
}

fn run_with(policy: &Policy, arguments: &[OsString], image: &str, keep: bool) -> Result<Output> {
    let create = Tool::Docker
        .command()
        .args(create_arguments(policy, arguments, image)?)
        .output()?;
    ensure!(
        create.status.success(),
        "create bounded container: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let id = String::from_utf8(create.stdout)?.trim().to_owned();
    ensure!(
        id.len() == DIGEST_LENGTH && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Docker returned an invalid container identity"
    );
    let container = Container { id, keep };
    eprintln!(
        "Bounded container {}: memory={} bytes, memory+swap={} bytes, cpus={}, pids={}, time={}s",
        container.id,
        policy.limits.memory,
        policy.limits.memory_swap,
        policy.limits.cpus,
        policy.limits.pids,
        policy.limits.seconds
    );
    docker(&["start", &container.id])?;
    docker(&["wait", &container.id])?;
    let inspected = docker(&["inspect", "--format", "{{json .State}}", &container.id])?;
    let state: State = serde_json::from_slice(&inspected.stdout)?;
    let mut output = docker(&["logs", "--tail", LOG_LINES, &container.id])?;
    output.status = exit_status(state.exit_code)?;
    let timed_out = matches!(state.exit_code, TIMEOUT_EXIT | KILLED_EXIT) && !state.oom_killed;
    match bounded_rules::completion(state.oom_killed, timed_out, output.status.success()) {
        Completion::MemoryLimit => anyhow::bail!(
            "inconclusive: bounded container {} exceeded memory limit {} bytes (memory+swap {} bytes)",
            container.id,
            policy.limits.memory,
            policy.limits.memory_swap
        ),
        Completion::TimeLimit => anyhow::bail!(
            "inconclusive: bounded container {} stopped at the time limit {}s or was killed (exit {}); no verification evidence",
            container.id,
            policy.limits.seconds,
            state.exit_code
        ),
        Completion::Passed | Completion::Failed => {
            ensure!(
                state.error.is_empty(),
                "bounded container failed: {}",
                state.error
            );
            Ok(output)
        }
    }
}

/// Execute the public bounded runner or install its native guards.
///
/// # Errors
///
/// Returns policy, installation, container, or project command failures.
pub fn execute(action: Action) -> Result<()> {
    match action {
        Action::Install => install(),
        Action::Run {
            image,
            project,
            policy,
            keep,
            command,
        } => {
            let project = crate::canonical(&project)?;
            ensure!(project.is_dir(), "bounded project is not a directory");
            let mut mount = OsString::from("type=bind,\"source=");
            mount.push(&project);
            mount.push(format!("\",target={WORKDIR}"));
            let mut arguments = vec![
                "run".into(),
                "--pull=never".into(),
                "--mount".into(),
                mount,
                "--workdir".into(),
                WORKDIR.into(),
                image.clone().into(),
            ];
            arguments.extend(command);
            let output = run_with(&Policy::load(policy.as_deref())?, &arguments, &image, keep)?;
            std::io::stdout().write_all(&output.stdout)?;
            std::io::stderr().write_all(&output.stderr)?;
            ensure!(
                output.status.success(),
                "bounded project command failed: {}",
                output.status
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn only_a_valid_zero_exit_supplies_passing_evidence() -> Result<()> {
        assert!(exit_status(0)?.success());
        for code in 1..=MAX_EXIT_CODE {
            assert!(!exit_status(code)?.success());
        }
        for code in [i32::MIN, -1, MAX_EXIT_CODE + 1, i32::MAX] {
            assert!(exit_status(code).is_err());
        }
        Ok(())
    }

    #[test]
    fn policy_rejects_invalid_resource_and_tool_data() {
        for (from, to) in [
            ("memory_mib = 2048", "memory_mib = 0"),
            ("swap_mib = 0", "swap_mib = 18446744073709551615"),
            ("version = 1", "version = 2"),
            ("\"kani\"", "\"../kani\""),
            ("\"kani\"", "\"cargo-kani\""),
        ] {
            assert!(
                Policy::parse(&DEFAULT_POLICY.replace(from, to)).is_err(),
                "{to}"
            );
        }
    }

    #[test]
    fn every_run_has_finite_limits_and_an_in_container_deadline() -> Result<()> {
        let policy = Policy::parse(DEFAULT_POLICY)?;
        let arguments =
            ["run", "--rm", "--memory", "999g", IMAGE, "cargo", "kani"].map(OsString::from);
        let result = create_arguments(&policy, &arguments, IMAGE)?;
        for pair in policy.resource_arguments().as_chunks::<2>().0 {
            assert!(result.windows(2).any(|actual| actual == pair));
        }
        assert_eq!(
            result
                .iter()
                .filter(|argument| *argument == "--memory")
                .count(),
            1
        );
        assert!(
            !result
                .iter()
                .any(|argument| argument == "--rm" || argument == "999g")
        );
        assert!(
            result
                .windows(2)
                .any(|pair| pair[0] == "--entrypoint" && pair[1] == TIMEOUT)
        );
        assert!(
            result
                .windows(3)
                .any(|part| part == [OsString::from("900s"), "cargo".into(), "kani".into()])
        );
        assert!(
            create_arguments(
                &policy,
                &["run".into(), "latest".into(), "kani".into()],
                "latest"
            )
            .is_err()
        );
        Ok(())
    }
}
