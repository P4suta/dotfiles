use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use dotfiles_xtask::tool::Tool;
use dotfiles_xtask::{
    check_build_dir, export_checks, skill_source_paths, skills, split_frontmatter, validate_tree,
};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
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
        agent: dotfiles_xtask::secrets::Agent,
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
        emulator: dotfiles_xtask::terminal::Emulator,
        session: dotfiles_xtask::terminal::Session,
        #[arg(long)]
        child: bool,
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
    Hook {
        hook: dotfiles_xtask::hooks::Hook,
        #[arg(last = true)]
        arguments: Vec<OsString>,
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
        step: dotfiles_xtask::setup::Step,
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
    CheckSecrets,
    CheckAdapters,
    OpsCheck,
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
    Profiles {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    PublicAudit {
        #[arg(long)]
        denylist: PathBuf,
    },
    Profile {
        action: dotfiles_xtask::profiles::NativeAction,
        #[arg(long, env = "DOTFILES_CONFIG")]
        config: PathBuf,
        #[arg(long, env = "DOTFILES_DESTINATION")]
        destination: PathBuf,
        #[arg(long)]
        live: bool,
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        backup: Option<PathBuf>,
    },
    Skills,
    /// Apply the native profile with its scripts, twice, on a disposable CI host.
    Rehearse {
        #[arg(long)]
        disposable_host: bool,
    },
    Check,
    Aliases,
    FormatMetadata,
    Snapshot {
        #[arg(long)]
        output: PathBuf,
    },
    SkillPaths,
    Install,
    InstallReviewGuard,
    InstallSkillOps,
    InstallPrWorkflow,
    PrWorkflow {
        #[command(subcommand)]
        command: dotfiles_xtask::pr_workflow::Action,
    },
    Proofs,
}

fn cargo(root: &Path, manifest: &Path, args: &[&str]) -> Result<()> {
    let configured = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
    let status = Tool::Cargo
        .command()
        .current_dir(root)
        .env(
            "CARGO_TARGET_DIR",
            check_build_dir(root, configured.as_deref()),
        )
        .args(args)
        .arg("--manifest-path")
        .arg(manifest)
        .status()
        .context("start cargo")?;
    ensure!(status.success(), "cargo {args:?} failed with {status}");
    Ok(())
}

fn check_package(root: &Path, manifest: &Path) -> Result<()> {
    cargo(root, manifest, &["fmt", "--check"])?;
    let configured = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
    let status = Tool::Cargo
        .command()
        .current_dir(root)
        .env(
            "CARGO_TARGET_DIR",
            check_build_dir(root, configured.as_deref()),
        )
        .args(["clippy", "--locked", "--all-targets", "--manifest-path"])
        .arg(manifest)
        .args(["--", "-D", "warnings"])
        .status()?;
    ensure!(status.success(), "clippy failed with {status}");
    cargo(root, manifest, &["test", "--locked", "--all-targets"])
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();
    cli.root = dotfiles_xtask::canonical(&cli.root)?;
    match cli.command {
        Action::Agent {
            agent,
            config,
            arguments,
        } => dotfiles_xtask::secrets::exit(dotfiles_xtask::secrets::agent(
            &cli.root, &config, agent, &arguments,
        )?),
        Action::Doctor { strict } => dotfiles_xtask::terminal::doctor(strict)?,
        Action::Touchid { live, elevated } => dotfiles_xtask::terminal::touchid(live, elevated)?,
        Action::Terminal {
            emulator,
            session,
            child,
            arguments,
        } => dotfiles_xtask::secrets::exit(dotfiles_xtask::terminal::run(
            emulator, session, child, &arguments,
        )?),
        Action::Hook { hook, arguments } => dotfiles_xtask::hooks::run(hook, &arguments)?,
        Action::Git { arguments } => {
            dotfiles_xtask::secrets::exit(dotfiles_xtask::hooks::git(&arguments)?)
        }
        Action::GitAudit {
            directory,
            fix_stale_hooks,
        } => dotfiles_xtask::hooks::audit(&directory, fix_stale_hooks)?,
        Action::Setup { step, config, live } => {
            dotfiles_xtask::setup::run(&cli.root, &config, step, live)?
        }
        Action::Refresh { config, live } => {
            dotfiles_xtask::setup::run(
                &cli.root,
                &config,
                dotfiles_xtask::setup::Step::Integrations,
                live,
            )?;
            dotfiles_xtask::setup::run(
                &cli.root,
                &config,
                dotfiles_xtask::setup::Step::Desktop,
                live,
            )?;
        }
        Action::CheckSecrets => dotfiles_xtask::quality::secrets(&cli.root)?,
        Action::CheckAdapters => dotfiles_xtask::quality::adapters(&cli.root)?,
        Action::OpsCheck => {
            ensure!(
                cfg!(target_os = "linux"),
                "Ansible checks require the Linux toolchain"
            );
            dotfiles_xtask::quality::ops(&cli.root)?;
        }
        Action::WslSign { arguments } => {
            dotfiles_xtask::secrets::exit(dotfiles_xtask::wsl::run(&arguments)?)
        }
        Action::RunSecrets {
            project,
            config,
            names,
            command,
        } => dotfiles_xtask::secrets::exit(dotfiles_xtask::secrets::run(
            &project, &config, &names, &command,
        )?),
        Action::RequireSecrets { names, command } => {
            dotfiles_xtask::secrets::exit(dotfiles_xtask::secrets::consume(&names, &command)?)
        }
        Action::Profiles { output } => {
            dotfiles_xtask::profiles::check_profiles(&cli.root, output.as_deref())?
        }
        Action::PublicAudit { denylist } => {
            dotfiles_xtask::profiles::audit_private(&cli.root, &denylist)?
        }
        Action::Profile {
            action,
            config,
            destination,
            live,
            state,
            backup,
        } => dotfiles_xtask::profiles::operate(
            &cli.root,
            action,
            &config,
            &destination,
            live,
            &state,
            backup.as_deref(),
        )?,
        Action::PrWorkflow { command } => dotfiles_xtask::pr_workflow::run(command)?,
        Action::InstallPrWorkflow => {
            cargo(
                &cli.root,
                Path::new("xtask/Cargo.toml"),
                &["build", "--locked", "--release", "--bin", "pr-workflow"],
            )?;
            let configured = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
            let binary = check_build_dir(&cli.root, configured.as_deref())
                .join("release")
                .join(format!("pr-workflow{}", std::env::consts::EXE_SUFFIX));
            let user_directory =
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .context("user directory is unavailable")?;
            dotfiles_xtask::pr_workflow::deploy(
                &cli.root,
                &binary,
                Path::new(&user_directory),
                |paths| {
                    let status = Tool::Chezmoi
                        .command()
                        .arg("--source")
                        .arg(&cli.root)
                        .args([
                            "--force",
                            "apply",
                            "--include",
                            "dirs,files,symlinks",
                            "--source-path",
                            "--parent-dirs",
                        ])
                        .args(paths)
                        .status()?;
                    ensure!(
                        status.success(),
                        "scoped PR skill installation failed with {status}"
                    );
                    Ok(())
                },
            )?;
        }
        Action::Proofs => dotfiles_xtask::skill_proofs::verify(&cli.root)?,
        Action::Skills => {
            dotfiles_xtask::skill_ops::check_catalog(&cli.root)?;
            println!(
                "Validated {} shared skills, aliases, and current maintenance dispositions",
                validate_tree(&cli.root)?
            );
        }
        Action::Rehearse { disposable_host } => {
            dotfiles_xtask::rehearsal::run(&cli.root, disposable_host)?
        }
        Action::Check => {
            dotfiles_xtask::skill_ops::check_catalog(&cli.root)?;
            dotfiles_xtask::quality::comment_scopes(&cli.root)?;
            println!(
                "Validated {} shared skills and aliases",
                validate_tree(&cli.root)?
            );
            check_package(&cli.root, Path::new("xtask/Cargo.toml"))?;
            check_package(
                &cli.root,
                Path::new("dot_agents/skills/github-repository/scripts/repo-settings/Cargo.toml"),
            )?;
            if cfg!(windows) {
                check_package(&cli.root, Path::new("tools/dotctl/Cargo.toml"))?;
            } else {
                check_package(&cli.root, Path::new("guard/Cargo.toml"))?;
            }
            dotfiles_xtask::quality::adapters(&cli.root)?;
            dotfiles_xtask::profiles::check_profiles(&cli.root, None)?;
            dotfiles_xtask::quality::secrets(&cli.root)?;
        }
        Action::Aliases => {
            let entries = skills(&cli.root)?;
            let directory = cli.root.join("dot_claude/skills");
            fs::create_dir_all(&directory)?;
            for (_, skill) in entries {
                let path = directory.join(format!("symlink_{}", skill.name));
                let expected = format!("../../.agents/skills/{}\n", skill.name);
                match fs::read_to_string(&path) {
                    Ok(current) => ensure!(
                        current == expected,
                        "refusing to overwrite {}",
                        path.display()
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        let mut file = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&path)?;
                        file.write_all(expected.as_bytes())?;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            println!(
                "Validated {} shared skills and aliases",
                validate_tree(&cli.root)?
            );
        }
        Action::FormatMetadata => {
            for (path, skill) in skills(&cli.root)? {
                let text = fs::read_to_string(&path)?;
                let (header, body) = split_frontmatter(&text)?;
                let documents = yaml_rust2::YamlLoader::load_from_str(header)?;
                ensure!(
                    documents[0].as_hash().context("mapping required")?.len() == 2,
                    "formatting must preserve optional metadata; format this skill manually"
                );
                let description = skill.description.replace(". ", ".\n  ");
                fs::write(
                    path,
                    format!(
                        "---\nname: {}\ndescription: >-\n  {description}\n---\n{body}",
                        skill.name
                    ),
                )?;
            }
        }
        Action::Snapshot { output } => export_checks(&cli.root, &output)?,
        Action::SkillPaths => {
            let mut output = std::io::stdout().lock();
            for path in skill_source_paths(&cli.root)? {
                output.write_all(path.to_str().context("non-UTF-8 skill source")?.as_bytes())?;
                output.write_all(&[0])?;
            }
        }
        Action::Install => {
            let paths = skill_source_paths(&cli.root)?;
            ensure!(!paths.is_empty(), "no skill files to install");
            let status = Tool::Chezmoi
                .command()
                .arg("--source")
                .arg(&cli.root)
                .args([
                    "--force",
                    "apply",
                    "--include",
                    "dirs,files,symlinks",
                    "--source-path",
                    "--parent-dirs",
                ])
                .args(paths)
                .status()?;
            ensure!(
                status.success(),
                "scoped skill installation failed with {status}"
            );
        }
        Action::InstallReviewGuard => {
            cargo(
                &cli.root,
                Path::new("xtask/Cargo.toml"),
                &["build", "--locked", "--release", "--bin", "coderabbit"],
            )?;
            let status = Tool::Chezmoi
                .command()
                .arg("--source")
                .arg(&cli.root)
                .args([
                    "--force",
                    "apply",
                    "--include",
                    "dirs,files",
                    "--source-path",
                    "--parent-dirs",
                ])
                .arg(
                    cli.root
                        .join("dot_config/coderabbit-guard/policy.json.tmpl"),
                )
                .status()?;
            ensure!(status.success(), "review guard policy installation failed");
            let configured = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
            let binary = check_build_dir(&cli.root, configured.as_deref())
                .join("release")
                .join(format!("coderabbit{}", std::env::consts::EXE_SUFFIX));
            let user_directory =
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .context("user directory is unavailable")?;
            dotfiles_xtask::review_guard::install(&binary, Path::new(&user_directory))?;
            println!("Installed CodeRabbit guard and cr alias with persistent local limits");
        }
        Action::InstallSkillOps => {
            dotfiles_xtask::skill_ops::check_catalog(&cli.root)?;
            cargo(
                &cli.root,
                Path::new("xtask/Cargo.toml"),
                &["build", "--locked", "--release", "--bin", "skill-ops"],
            )?;
            let configured = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
            let binary = check_build_dir(&cli.root, configured.as_deref())
                .join("release")
                .join(format!("skill-ops{}", std::env::consts::EXE_SUFFIX));
            let user_directory =
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .context("user directory is unavailable")?;
            dotfiles_xtask::skill_install::deploy(
                &cli.root,
                &binary,
                Path::new(&user_directory),
                |path| {
                    let status = Tool::Chezmoi
                        .command()
                        .arg("--source")
                        .arg(&cli.root)
                        .args([
                            "--force",
                            "apply",
                            "--include",
                            "dirs,files",
                            "--source-path",
                            "--parent-dirs",
                        ])
                        .arg(path)
                        .status()?;
                    ensure!(
                        status.success(),
                        "installation failed for {}",
                        path.display()
                    );
                    Ok(())
                },
            )?;
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
