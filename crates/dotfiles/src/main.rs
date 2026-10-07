//! `dotfiles`, the installed runtime that every Git hook, setup launcher, and agent hook runs.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
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
    /// Claude Code `PreToolUse` hook for Bash and PowerShell: refuses recursive disk scans and file writes through inline interpreters, heredocs, and redirects.
    ClaudeBash,
    /// Claude Code `PreToolUse` and `PostToolUse` hook for Bash and PowerShell: sets each command's timeout from its measured durations.
    ClaudeTiming,
    /// The agent hooks of the verification gate: the session start report of a repository's gaps, and the stop decision over the files a turn changed.
    Assure { point: dotfiles::assure::AgentPoint },
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
    /// The Windows setup command line, which the elevated sshd setup reruns.
    #[command(hide = true)]
    Windows {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();
    cli.root = dotfiles::canonical(&cli.root)?;
    let exit = dotfiles::secrets::exit;
    match cli.command {
        Action::Agent {
            agent,
            config,
            arguments,
        } => exit(dotfiles::secrets::agent(
            &cli.root, &config, agent, &arguments,
        )?),
        Action::Doctor { strict } => dotfiles::terminal::doctor(strict)?,
        Action::Touchid { live, elevated } => dotfiles::terminal::touchid(live, elevated)?,
        Action::Terminal {
            emulator,
            session,
            child,
            arguments,
        } => exit(dotfiles::terminal::run(
            emulator, session, child, &arguments,
        )?),
        Action::ClaudeBash => dotfiles::claude_hook::run()?,
        Action::ClaudeTiming => dotfiles::claude_timing::run()?,
        Action::Assure { point } => dotfiles::assure::hook(point)?,
        Action::Health { session } => health(session)?,
        Action::Hook { hook, arguments } => dotfiles::hooks::run(hook, &arguments)?,
        Action::LineEndings { request } => dotfiles::line_endings::run(request)?,
        Action::Git { arguments } => exit(dotfiles::hooks::git(&arguments)?),
        Action::GitAudit {
            directory,
            fix_stale_hooks,
        } => dotfiles::hooks::audit(&directory, fix_stale_hooks)?,
        Action::Setup { step, config, live } => {
            dotfiles::setup::run(&cli.root, &config, step, live)?;
        }
        Action::Refresh { config, live } => {
            for step in [
                dotfiles::setup::Step::Integrations,
                dotfiles::setup::Step::Desktop,
            ] {
                dotfiles::setup::run(&cli.root, &config, step, live)?;
            }
        }
        Action::WslSign { arguments } => exit(dotfiles::wsl::run(&arguments)?),
        Action::RunSecrets {
            project,
            config,
            names,
            command,
        } => exit(dotfiles::secrets::run(&project, &config, &names, &command)?),
        Action::RequireSecrets { names, command } => {
            exit(dotfiles::secrets::consume(&names, &command)?);
        }
        Action::Profile {
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
        Action::Hosts { action } => dotfiles::hosts::run(action)?,
        Action::Windows { arguments } => windows(arguments)?,
    }
    Ok(())
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

fn main() {
    if let Err(error) = run() {
        dotfiles::report(&error);
        std::process::exit(1);
    }
}
