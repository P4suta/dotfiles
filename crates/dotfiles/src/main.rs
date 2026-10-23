//! `dotfiles`, the installed runtime that every Git hook, setup launcher, and agent hook runs.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dotfiles::codex_hooks;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run unbounded verification only inside a container with policy limits.
    Bounded {
        #[command(subcommand)]
        action: dotfiles::bounded::Action,
    },
    /// Install mise tools with no mise shims on PATH.
    MiseInstall {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    /// Read mise's requested GitHub host from an installed CLI without executing its mise shim.
    GithubToken,
    Repository {
        #[command(subcommand)]
        request: RepositoryRequest,
    },
    Agent {
        agent: dotfiles::secrets::Agent,
        #[arg(long, env = "DOTFILES_CONFIG")]
        config: PathBuf,
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
    Doctor {
        #[arg(long)]
        strict: bool,
    },
    Touchid {
        #[arg(long)]
        live: bool,
        #[arg(long, hide = true)]
        elevated: bool,
    },
    Terminal {
        emulator: dotfiles::terminal::Emulator,
        session: dotfiles::terminal::Session,
        #[arg(long)]
        child: bool,
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
    #[cfg(unix)]
    /// Inspect Herdr and retire an explicitly selected instance.
    Herdr {
        #[command(subcommand)]
        action: HerdrAction,
    },
    /// Claude Code `PreToolUse` hook for Bash and PowerShell: refuses recursive disk scans and file writes through inline interpreters, heredocs, and redirects.
    ClaudeBash {
        /// Run an exact modern replacement through the client's input rewrite instead of refusing the legacy command.
        #[arg(long)]
        rewrite: bool,
    },
    /// `PreToolUse` and `PostToolUse` hook for shell commands: records each command's duration and reports the measured duration.
    /// For Claude Code it also sets the command's timeout from its measured durations.
    ClaudeTiming {
        /// The client whose hook protocol the output follows.
        #[arg(long, value_enum, default_value_t)]
        client: dotfiles::claude_timing::Client,
    },
    /// Refuse supported implementation edits without a checked issue context.
    IssueGuard,
    /// Lint files written by an agent, reading its hook payload from standard input.
    LintHook,
    /// Execute Codex hooks with local evidence or inspect their recent results.
    CodexHooks {
        #[command(subcommand)]
        request: codex_hooks::Request,
    },
    /// Fixed Japanese desktop notifications, independent of experimental skill maintenance.
    Notifications {
        #[command(subcommand)]
        action: dotfiles::notifications::Action,
    },
    /// The agent hooks of the verification gate: the session start report of a repository's gaps, and the stop decision over the files a turn changed.
    Assure {
        point: dotfiles::assure::AgentPoint,
        /// Check Stop reply prose in this owner-selected language.
        #[arg(long = "reply-language")]
        language: Option<dotfiles::assure::ReplyLanguage>,
    },
    /// Refuses work on a host whose process creation stalls, as it does when the Defender scan engine stops responding.
    Health {
        /// Prints a refusal to standard output, which a session start hook adds to the agent's context, and exits successfully.
        #[arg(long)]
        session: bool,
    },
    Hook {
        hook: dotfiles::hooks::Hook,
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
    #[command(hide = true)]
    PostCommitWorker { producer: String },
    LineEndings {
        #[command(subcommand)]
        request: dotfiles::line_endings::Request,
    },
    Git {
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
    GitAudit {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        fix_stale_hooks: bool,
    },
    Setup {
        step: dotfiles::setup::Step,
        #[arg(long, env = "DOTFILES_CONFIG")]
        config: PathBuf,
        #[arg(long)]
        live: bool,
    },
    Refresh {
        #[arg(long, env = "DOTFILES_CONFIG")]
        config: PathBuf,
        #[arg(long)]
        live: bool,
    },
    WslSign {
        #[arg(last = true, required = true)]
        arguments: Vec<OsString>,
    },
    RunSecrets {
        #[arg(long)]
        project: String,
        #[arg(long)]
        config: String,
        #[arg(long)]
        names: String,
        #[arg(last = true, required = true)]
        command: Vec<OsString>,
    },
    RequireSecrets {
        #[arg(long)]
        names: String,
        #[arg(last = true, required = true)]
        command: Vec<OsString>,
    },
    Profile {
        action: dotfiles::profiles::NativeAction,
        #[arg(long, env = "DOTFILES_CONFIG")]
        config: PathBuf,
        #[arg(long, env = "DOTFILES_DESTINATION")]
        destination: PathBuf,
        #[arg(long)]
        live: bool,
        /// A state directory apart from chezmoi's default database beside the configuration.
        #[arg(long)]
        state: Option<PathBuf>,
        #[arg(long)]
        backup: Option<PathBuf>,
    },
    /// Keep one host per OS family reachable, run the gate on each, and require their notes before push.
    Hosts {
        #[command(subcommand)]
        action: dotfiles::hosts::Action,
    },
    /// Plan, fetch, and verify pinned materials.
    Artifacts {
        #[command(subcommand)]
        action: dotfiles::artifacts::Action,
    },
    /// The Windows setup command line, which the elevated sshd setup reruns.
    #[command(hide = true)]
    Windows {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
}

#[derive(Subcommand)]
enum RepositoryRequest {
    Generate {
        #[arg(long)]
        remote: Option<String>,
    },
    Check {
        #[arg(long)]
        remote: Option<String>,
    },
}

#[cfg(unix)]
#[derive(Clone, Copy, Subcommand)]
enum HerdrAction {
    Health {
        #[arg(long)]
        session: bool,
    },
    Cleanup {
        #[arg(long)]
        pid: u32,
    },
}

fn run(mut cli: Cli) -> Result<()> {
    cli.root = dotfiles::canonical(&cli.root)?;
    let exit = dotfiles::secrets::exit;
    match cli.command {
        Commands::Bounded { action } => dotfiles::bounded::execute(action)?,
        Commands::MiseInstall { arguments } => {
            let home = platform::home::home().context("native home is unavailable")?;
            exit(
                platform::runtime::Real::new(&home)?
                    .mise_install(&arguments)?
                    .status()?,
            );
        }
        Commands::GithubToken => github_token()?,
        Commands::Repository { request } => repository(&cli.root, request)?,
        Commands::Agent {
            agent,
            config,
            arguments,
        } => exit(dotfiles::secrets::agent(
            &cli.root, &config, agent, &arguments,
        )?),
        Commands::Doctor { strict } => dotfiles::terminal::doctor(strict)?,
        Commands::Touchid { live, elevated } => dotfiles::terminal::touchid(live, elevated)?,
        Commands::Terminal {
            emulator,
            session,
            child,
            arguments,
        } => exit(dotfiles::terminal::run(
            emulator, session, child, &arguments,
        )?),
        #[cfg(unix)]
        Commands::Herdr { action } => herdr(action)?,
        Commands::ClaudeBash { rewrite } => dotfiles::claude_hook::run(rewrite)?,
        Commands::ClaudeTiming { client } => dotfiles::claude_timing::run(client)?,
        Commands::IssueGuard => std::process::exit(dotfiles::issue_guard::hook()?),
        Commands::LintHook => std::process::exit(dotfiles::lint_hook::run()?),
        Commands::CodexHooks { request } => std::process::exit(codex_hooks::run(request)?),
        Commands::Notifications { action } => dotfiles::notifications::run(action)?,
        Commands::Assure { point, language } => assure(point, language)?,
        Commands::Health { session } => health(session)?,
        Commands::Hook { hook, arguments } => dotfiles::hooks::run(hook, &arguments)?,
        Commands::PostCommitWorker { producer } => {
            std::process::exit(dotfiles::hooks::post_commit_worker(&producer));
        }
        Commands::LineEndings { request } => dotfiles::line_endings::run(request)?,
        Commands::Git { arguments } => exit(dotfiles::hooks::git(&arguments)?),
        Commands::GitAudit {
            directory,
            fix_stale_hooks,
        } => dotfiles::hooks::audit(&directory, fix_stale_hooks)?,
        Commands::Setup { step, config, live } => {
            dotfiles::setup::run(&cli.root, &config, step, live)?;
        }
        Commands::Refresh { config, live } => {
            for step in [
                dotfiles::setup::Step::Integrations,
                dotfiles::setup::Step::Desktop,
            ] {
                dotfiles::setup::run(&cli.root, &config, step, live)?;
            }
        }
        Commands::WslSign { arguments } => exit(dotfiles::wsl::run(&arguments)?),
        Commands::RunSecrets {
            project,
            config,
            names,
            command,
        } => exit(dotfiles::secrets::run(&project, &config, &names, &command)?),
        Commands::RequireSecrets { names, command } => {
            exit(dotfiles::secrets::consume(&names, &command)?);
        }
        Commands::Profile {
            action,
            config,
            destination,
            live,
            state,
            backup,
        } => dotfiles::profiles::operate(
            &cli.root,
            action,
            &config,
            &destination,
            live,
            state.as_deref(),
            backup.as_deref(),
        )?,
        Commands::Hosts { action } => dotfiles::hosts::run(action)?,
        Commands::Artifacts { action } => dotfiles::artifacts::run(&cli.root, action)?,
        Commands::Windows { arguments } => windows(arguments)?,
    }
    Ok(())
}

fn repository(root: &Path, request: RepositoryRequest) -> Result<()> {
    match request {
        RepositoryRequest::Generate { remote } => {
            dotfiles::repository::write(root)?;
            if let Some(remote) = remote {
                dotfiles::repository::write_description(root, &remote)?;
            }
        }
        RepositoryRequest::Check { remote } => {
            dotfiles::repository::check(root, remote.as_deref())?;
        }
    }
    Ok(())
}

fn github_token() -> Result<()> {
    let host =
        std::env::var_os("MISE_CREDENTIAL_HOST").context("mise credential host unavailable")?;
    let home = std::env::var_os(platform::github::CREDENTIAL_HOME).map(PathBuf::from);
    dotfiles::secrets::exit(platform::github::command_in_home(&host, home.as_deref())?.status()?)
}

/// Runs the Windows setup command line and exits with its status.
#[cfg(windows)]
fn windows(arguments: Vec<OsString>) -> Result<()> {
    std::process::exit(dotfiles::windows::cli::execute(
        std::iter::once(OsString::from("dotfiles windows")).chain(arguments),
    ))
}

#[cfg(not(windows))]
fn windows(_: Vec<OsString>) -> Result<()> {
    anyhow::bail!("the Windows setup runs only on Windows")
}

#[cfg(unix)]
fn herdr(action: HerdrAction) -> Result<()> {
    let home = platform::home::home().context("native home is unavailable")?;
    match action {
        HerdrAction::Health { session: true } => dotfiles::herdr::session_start(&home),
        HerdrAction::Health { session: false } => anyhow::ensure!(
            dotfiles::herdr::health(&home)?,
            "Herdr health reported findings"
        ),
        HerdrAction::Cleanup { pid } => dotfiles::herdr::cleanup(&home, pid)?,
    }
    Ok(())
}

/// The verification gate's agent hooks, whose session start first reports the agent hooks that failed since the last one.
fn assure(
    point: dotfiles::assure::AgentPoint,
    reply_language: Option<dotfiles::assure::ReplyLanguage>,
) -> Result<()> {
    if point == dotfiles::assure::AgentPoint::Start {
        dotfiles::hook_failures::session_start();
        #[cfg(unix)]
        if let Some(home) = platform::home::home() {
            dotfiles::herdr::session_start(&home);
        }
    }
    dotfiles::assure::hook(point, reply_language.map(Into::into))
}

/// The health gate, which at session start prints its refusal instead of failing.
fn health(session: bool) -> Result<()> {
    let home = platform::home::home().context("user directory is unavailable")?;
    let result = dotfiles::health::gate(
        Path::new(&home),
        if session { "session-start" } else { "health" },
    );
    match result {
        Err(error) if session => {
            print!(
                "{}",
                error.downcast::<dotfiles::refusal::Refusal>()?.render()
            );
            Ok(())
        }
        other => other,
    }
}

/// The agent hook this command line runs, whose failures the next session start reports.
fn agent_hook() -> Option<&'static str> {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let session = arguments.iter().any(|argument| argument == "--session");
    match arguments.first()?.to_str()? {
        "claude-bash" => Some("claude-bash"),
        "claude-timing" => Some("claude-timing"),
        "issue-guard" => Some("issue-guard"),
        "lint-hook" => Some("lint-hook"),
        "notifications" => Some("notifications"),
        "assure" => Some("assure"),
        "health" if session => Some("health"),
        _ => None,
    }
}

fn main() {
    match dotfiles::bounded::dispatch() {
        Ok(Some(status)) => dotfiles::secrets::exit(status),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
    }
    // `dotfiles doctor` reads the installed runtime's build identity to tell whether it matches the applied source.
    match dotfiles::freshness::print_build_identity() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            dotfiles::report(&error);
            std::process::exit(1);
        }
    }
    let hook = agent_hook();
    if let Some(hook) = hook {
        dotfiles::hook_failures::record_panics(hook);
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if let Some(hook) = hook
                && error.use_stderr()
            {
                dotfiles::hook_failures::record(hook, &error.to_string());
            }
            error.exit();
        }
    };
    if let Err(error) = run(cli) {
        if let Some(hook) = hook {
            dotfiles::hook_failures::record(hook, &format!("{error:#}"));
        }
        dotfiles::report(&error);
        std::process::exit(1);
    }
}
