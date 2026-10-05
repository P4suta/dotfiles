mod apply;
mod env;
mod lint;
mod proc;
mod render;
mod setup;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "dotctl", version, about = "Command layer for these dotfiles")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Lint {
        kind: lint::Kind,
        files: Vec<PathBuf>,
    },
    Preflight,
    Setup {
        #[command(subcommand)]
        what: SetupKind,
    },
    Apply {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SetupKind {
    Tools {
        #[arg(long)]
        source: PathBuf,
        #[arg(long, value_delimiter = ',')]
        scoop_apps: Vec<String>,
    },
    Scoop {
        #[arg(long)]
        installer_commit: String,
        #[arg(long)]
        installer_sha256: String,
        #[arg(long, value_delimiter = ',')]
        buckets: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        apps: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        winget_duplicates: Vec<String>,
    },
    Winget {
        #[arg(long, value_delimiter = ',')]
        apps: Vec<String>,
    },
    Keyboard {
        #[arg(long)]
        keyboard_delay: u32,
        #[arg(long)]
        keyboard_speed: u32,
    },
    Shell,
    PythonStub,
    StorageScout,
    Sshd {
        #[arg(long, default_value_t = 22)]
        port: u16,
        #[arg(long, default_value = "pwsh")]
        default_shell: String,
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        password_auth: bool,
        #[arg(long, default_value = "Private")]
        firewall_profile: String,
        #[arg(long, default_value = "")]
        authorized_key: String,
        #[arg(long)]
        elevated: bool,
    },
    Ocomment {
        #[arg(long)]
        repo_url: String,
        #[arg(long)]
        destination: String,
    },
    Herdr {
        #[arg(long)]
        manifest_url: String,
        #[arg(long)]
        channel: String,
        #[arg(long, default_value = "")]
        expected_build_id: String,
        #[arg(long, default_value_t = 3)]
        retain: usize,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Lint { kind, files } => lint::run(kind, &files),
        Command::Preflight => preflight(),
        Command::Apply { yes, force } => apply::run(yes, force),
        Command::Setup { what } => match what {
            SetupKind::Tools { source, scoop_apps } => setup::tools::run(&source, &scoop_apps),
            SetupKind::Scoop {
                installer_commit,
                installer_sha256,
                buckets,
                apps,
                winget_duplicates,
            } => setup::scoop::run(&setup::scoop::Options {
                installer_commit,
                installer_sha256,
                buckets,
                apps,
                winget_duplicates,
            }),
            SetupKind::Winget { apps } => setup::winget::run(&apps),
            SetupKind::Keyboard {
                keyboard_delay,
                keyboard_speed,
            } => setup::keyboard::run(&setup::keyboard::Options {
                keyboard_delay,
                keyboard_speed,
            }),
            SetupKind::Shell => setup::shell::run(),
            SetupKind::PythonStub => setup::python_stub::run(),
            SetupKind::StorageScout => setup::storage_scout::run(),
            SetupKind::Sshd {
                port,
                default_shell,
                password_auth,
                firewall_profile,
                authorized_key,
                elevated,
            } => setup::sshd::run(&setup::sshd::Options {
                port,
                default_shell,
                password_auth,
                firewall_profile,
                authorized_key,
                elevated,
            }),
            SetupKind::Ocomment {
                repo_url,
                destination,
            } => setup::ocomment::run(&setup::ocomment::Options {
                repo_url,
                destination,
            }),
            SetupKind::Herdr {
                manifest_url,
                channel,
                expected_build_id,
                retain,
            } => setup::herdr::run(&setup::herdr::Options {
                manifest_url,
                channel,
                expected_build_id,
                retain,
            }),
        },
    };

    match result {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(err) => {
            eprintln!("dotctl: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn preflight() -> anyhow::Result<i32> {
    let mut cmd = env::command("lefthook");
    cmd.args(["run", "preflight", "--all-files"]);
    proc::status(&mut cmd)
}
