//! The command line of `dotfiles windows`.

use super::setup;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "dotfiles windows",
    version,
    about = "The Windows machine setup of these dotfiles"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Setup {
        #[command(subcommand)]
        what: SetupKind,
    },
}

#[derive(Debug, Subcommand)]
enum SetupKind {
    Tools {
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

/// Parses `arguments`, whose first word names the program, runs the setup they name, and returns the process exit status.
#[must_use]
pub fn execute<I, T>(arguments: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(arguments);
    let result = match cli.command {
        Command::Setup { what } => match what {
            SetupKind::Tools { scoop_apps } => setup::tools::run(&scoop_apps),
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
            SetupKind::PythonStub => {
                setup::python_stub::run();
                Ok(0)
            }
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
            } => {
                setup::ocomment::run(&setup::ocomment::Options {
                    repo_url,
                    destination,
                });
                Ok(0)
            }
            SetupKind::Herdr {
                manifest_url,
                channel,
                expected_build_id,
                retain,
            } => {
                setup::herdr::run(&setup::herdr::Options {
                    manifest_url,
                    channel,
                    expected_build_id,
                    retain,
                });
                Ok(0)
            }
        },
    };

    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("dotfiles windows: {err:#}");
            1
        }
    }
}
