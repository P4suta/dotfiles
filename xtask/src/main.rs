use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use dotfiles_xtask::change_rules::{Gate, Gates};
use dotfiles_xtask::freshness::Built;
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
    /// Claude Code `PreToolUse` hook for Bash and PowerShell: refuses recursive disk scans and file writes through inline interpreters, heredocs, and redirects.
    ClaudeBash,
    /// Claude Code `PreToolUse` and `PostToolUse` hook for Bash and PowerShell: sets each command's timeout from its measured durations.
    ClaudeTiming,
    /// Refuses work on a host whose process creation stalls, as it does when the Defender scan engine stops responding.
    Health {
        /// Prints a refusal to standard output, which a session start hook adds to the agent's context, and exits successfully.
        #[arg(long)]
        session: bool,
    },
    Hook {
        hook: dotfiles_xtask::hooks::Hook,
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
    LineEndings {
        #[command(subcommand)]
        request: dotfiles_xtask::line_endings::Request,
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
    /// Writes the generated files: the architecture decision index.
    Gen {
        /// Refuses a stale generated file instead of writing it.
        #[arg(long)]
        check: bool,
    },
    /// Compares every profile, signing, and role rendering with its committed manifest and lists the scripts that depend on the command line.
    ProfileSnapshots {
        /// Rewrites this host family's manifests from the current rendering instead.
        #[arg(long)]
        write: bool,
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
    /// Apply the native profile, rerun every script on the provisioned host, and apply again, on a disposable CI host.
    Rehearse {
        #[arg(long)]
        disposable_host: bool,
    },
    /// Run the gates a change selects, or every gate without a scope.
    Check {
        #[command(flatten)]
        scope: dotfiles_xtask::change::Scope,
    },
    /// Print `gates=<names>`: the gates the change set selects, or every gate without a scope.
    Classify {
        #[command(flatten)]
        scope: dotfiles_xtask::change::Scope,
    },
    /// Draft the tracked decision for a changed or new skill.
    Decide {
        skill: String,
    },
    /// Inspect the worktree, branch, PR, checks, pauses, and host, and print the single next step as JSON.
    Next {
        /// Use the last fetched state and skip GitHub.
        #[arg(long)]
        offline: bool,
        /// Run a local, reversible next step, then report the step after it.
        #[arg(long)]
        execute: bool,
    },
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
    /// Install the prose checker, its pinned linters, and the reply hook of each client.
    InstallProse,
    PrWorkflow {
        #[command(subcommand)]
        command: dotfiles_xtask::pr_workflow::Action,
    },
    /// Keep one host per OS family reachable, run the gate on each, and require their notes before push.
    Hosts {
        #[command(subcommand)]
        action: dotfiles_xtask::hosts::Action,
    },
    /// Prove the production contracts.
    /// A scope without the Rust gate skips them.
    Proofs {
        #[command(flatten)]
        scope: dotfiles_xtask::change::Scope,
    },
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
        .args(&args[..1])
        .arg("--manifest-path")
        .arg(manifest)
        .args(&args[1..])
        .status()
        .context("start cargo")?;
    ensure!(status.success(), "cargo {args:?} failed with {status}");
    Ok(())
}

/// Formats, lints, and tests every crate of the root workspace under its one lint policy.
fn check_workspace(root: &Path) -> Result<()> {
    let manifest = Path::new("Cargo.toml");
    cargo(root, manifest, &["fmt", "--all", "--check"])?;
    cargo(
        root,
        manifest,
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )
    .context("clippy failed")?;
    cargo(
        root,
        manifest,
        &["test", "--locked", "--workspace", "--all-targets"],
    )
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

/// Runs the checks of each selected gate.
/// A change that selects fewer gates skips only the checks it can't affect.
fn check(root: &Path, gates: Gates) -> Result<()> {
    if gates.contains(Gate::Prose) {
        dotfiles_xtask::adr::generate(root, true)?;
        dotfiles_xtask::quality::editorconfig(root)?;
        dotfiles_xtask::quality::comment_scopes(root)?;
        dotfiles_xtask::instruction_audit::check(root)?;
        dotfiles_xtask::line_endings::check(root)?;
        let unknown = dotfiles_xtask::quality::unknown_recipes(root)?;
        anyhow::ensure!(
            unknown.is_empty(),
            "references to recipes the justfile does not define: {unknown:#?}"
        );
        dotfiles_xtask::quality::secrets(root)?;
        dotfiles_xtask::prose::repository(
            root,
            dotfiles_xtask::prose::Scope::Documents,
            &dotfiles_xtask::prose::Bundle::source(root)?,
        )?;
    }
    if gates.contains(Gate::Skills) {
        dotfiles_xtask::skill_ops::check_catalog(root)?;
        println!(
            "Validated {} shared skills and aliases",
            validate_tree(root)?
        );
        dotfiles_xtask::quality::adapters(root)?;
    }
    if gates.contains(Gate::Rust) {
        check_workspace(root)?;
        check_package(
            root,
            Path::new("home/dot_agents/skills/github-repository/scripts/repo-settings/Cargo.toml"),
        )?;
    }
    if gates.contains(Gate::Profiles) {
        dotfiles_xtask::quality::chezmoi_version(root)?;
        let unreferenced = dotfiles_xtask::quality::unreferenced_leaves(root)?;
        anyhow::ensure!(
            unreferenced.is_empty(),
            "profile leaves that no template includes: {unreferenced:?}"
        );
        dotfiles_xtask::profiles::check_profiles(root, None)?;
        dotfiles_xtask::profile_snapshot::check(root)?;
        dotfiles_xtask::ignore_audit::check(root)?;
        dotfiles_xtask::input_audit::check(root)?;
    }
    println!("Checked the gates: {}", gates.names());
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "one arm per subcommand, which the exhaustive match keeps complete"
)]
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
        Action::ClaudeBash => dotfiles_xtask::claude_hook::run()?,
        Action::ClaudeTiming => dotfiles_xtask::claude_timing::run()?,
        Action::Health { session } => health(session)?,
        Action::Hook { hook, arguments } => dotfiles_xtask::hooks::run(hook, &arguments)?,
        Action::LineEndings { request } => dotfiles_xtask::line_endings::run(request)?,
        Action::Git { arguments } => {
            dotfiles_xtask::secrets::exit(dotfiles_xtask::hooks::git(&arguments)?)
        }
        Action::GitAudit {
            directory,
            fix_stale_hooks,
        } => dotfiles_xtask::hooks::audit(&directory, fix_stale_hooks)?,
        Action::Setup { step, config, live } => {
            dotfiles_xtask::setup::run(&cli.root, &config, step, live)?;
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
            dotfiles_xtask::quality::chezmoi_version(&cli.root)?;
            dotfiles_xtask::profiles::check_profiles(&cli.root, output.as_deref())?;
            dotfiles_xtask::profile_snapshot::check(&cli.root)?;
            dotfiles_xtask::ignore_audit::check(&cli.root)?;
            dotfiles_xtask::input_audit::check(&cli.root)?;
        }
        Action::Gen { check } => dotfiles_xtask::adr::generate(&cli.root, check)?,
        Action::ProfileSnapshots { write } => {
            if write {
                dotfiles_xtask::profile_snapshot::write(&cli.root)?;
            }
            dotfiles_xtask::profile_snapshot::check(&cli.root)?;
            dotfiles_xtask::input_audit::check(&cli.root)?;
        }
        Action::PublicAudit { denylist } => {
            dotfiles_xtask::profiles::audit_private(&cli.root, &denylist)?;
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
        Action::PrWorkflow { command } => {
            // The same words the standalone binary receives: everything after this subcommand.
            let arguments: Vec<String> = std::env::args()
                .skip(1)
                .skip_while(|argument| argument != "pr-workflow")
                .skip(1)
                .collect();
            dotfiles_xtask::pr_workflow::execute(command, &arguments)?;
        }
        Action::Hosts { action } => dotfiles_xtask::hosts::run(action)?,
        Action::InstallPrWorkflow => install_pr_workflow(&cli.root)?,
        Action::InstallProse => install_prose(&cli.root)?,
        Action::Proofs { scope } => {
            if scope.resolve(&cli.root)?.contains(Gate::Rust) {
                dotfiles_xtask::skill_proofs::verify(&cli.root)?;
            } else {
                println!("Skipped the proofs: the change selects no Rust gate");
            }
        }
        Action::Classify { scope } => dotfiles_xtask::change::run(&cli.root, &scope)?,
        Action::Skills => {
            dotfiles_xtask::skill_ops::check_catalog(&cli.root)?;
            println!(
                "Validated {} shared skills, aliases, and current maintenance dispositions",
                validate_tree(&cli.root)?
            );
        }
        Action::Rehearse { disposable_host } => {
            dotfiles_xtask::rehearsal::run(&cli.root, disposable_host)?;
        }
        Action::Check { scope } => check(&cli.root, scope.resolve(&cli.root)?)?,
        Action::Decide { skill } => {
            println!("{}", dotfiles_xtask::skill_ops::decide(&cli.root, &skill)?);
        }
        Action::Next { offline, execute } => {
            dotfiles_xtask::next_action::run(&cli.root, offline, execute)?;
        }
        Action::Aliases => aliases(&cli.root)?,
        Action::FormatMetadata => format_metadata(&cli.root)?,
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
            apply_sources(&cli.root, "dirs,files,symlinks", paths)
                .context("scoped skill installation failed")?;
        }
        Action::InstallReviewGuard => install_review_guard(&cli.root)?,
        Action::InstallSkillOps => install_skill_ops(&cli.root)?,
    }
    Ok(())
}

/// The health gate, which at session start prints its refusal instead of failing.
fn health(session: bool) -> Result<()> {
    let result = dotfiles_xtask::health::gate(
        &user_directory()?,
        if session { "session-start" } else { "health" },
    );
    match result {
        Err(error) if session => {
            print!(
                "{}",
                error
                    .downcast::<dotfiles_xtask::refusal::Refusal>()?
                    .render()
            );
            Ok(())
        }
        other => other,
    }
}

fn user_directory() -> Result<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .context("user directory is unavailable")
}

/// Builds one binary of this package in release mode and returns where it landed.
fn release_binary(root: &Path, name: &str) -> Result<PathBuf> {
    cargo(
        root,
        Path::new("xtask/Cargo.toml"),
        &["build", "--locked", "--release", "--bin", name],
    )?;
    let configured = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
    Ok(check_build_dir(root, configured.as_deref())
        .join("release")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
}

/// Applies the targets of the given source paths and their parent directories.
fn apply_sources<I, S>(root: &Path, include: &str, sources: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let status = Tool::chezmoi_in(root)
        .args([
            "--force",
            "apply",
            "--include",
            include,
            "--source-path",
            "--parent-dirs",
        ])
        .args(sources)
        .status()?;
    ensure!(status.success(), "chezmoi apply failed with {status}");
    Ok(())
}

fn install_pr_workflow(root: &Path) -> Result<()> {
    let binary = release_binary(root, "pr-workflow")?;
    let user_directory = user_directory()?;
    dotfiles_xtask::pr_workflow::deploy(root, &binary, &user_directory, |paths| {
        apply_sources(root, "dirs,files,symlinks", paths)
            .context("scoped PR skill installation failed")
    })?;
    dotfiles_xtask::freshness::record_installation(&user_directory, root, Built::PrWorkflow)
}

fn install_prose(root: &Path) -> Result<()> {
    let binary = release_binary(root, "prose")?;
    let user_directory = user_directory()?;
    // chezmoi skips the contents of a directory given by its source path, so the installer gives the target tree.
    let status = Tool::chezmoi_in(root)
        .arg("--destination")
        .arg(&user_directory)
        .args([
            "--force",
            "apply",
            "--include",
            "dirs,files",
            "--parent-dirs",
        ])
        .arg(user_directory.join(".config/prose"))
        .arg(user_directory.join(".config/opencode/plugins/prose.ts"))
        .status()?;
    ensure!(
        status.success(),
        "the prose configuration installation failed with {status}"
    );
    dotfiles_xtask::prose::install(root, &binary, &user_directory)
}

fn install_review_guard(root: &Path) -> Result<()> {
    let binary = release_binary(root, "coderabbit")?;
    apply_sources(
        root,
        "dirs,files",
        [root.join(dotfiles_xtask::review_guard::SOURCE_POLICY)],
    )
    .context("review guard policy installation failed")?;
    let user_directory = user_directory()?;
    dotfiles_xtask::review_guard::install(&binary, &user_directory)?;
    dotfiles_xtask::freshness::record_installation(&user_directory, root, Built::Coderabbit)?;
    println!("Installed CodeRabbit guard and cr alias with persistent local limits");
    Ok(())
}

fn install_skill_ops(root: &Path) -> Result<()> {
    dotfiles_xtask::skill_ops::check_catalog(root)?;
    let binary = release_binary(root, "skill-ops")?;
    let user_directory = user_directory()?;
    dotfiles_xtask::skill_install::deploy(root, &binary, &user_directory, |path| {
        apply_sources(root, "dirs,files", [path])
            .with_context(|| format!("installation failed for {}", path.display()))
    })?;
    dotfiles_xtask::freshness::record_installation(&user_directory, root, Built::SkillOps)
}

/// Writes the Claude Code symlink of every shared skill, refusing to overwrite a different file.
fn aliases(root: &Path) -> Result<()> {
    let entries = skills(root)?;
    let directory = root.join("home/dot_claude/skills");
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
        validate_tree(root)?
    );
    Ok(())
}

fn format_metadata(root: &Path) -> Result<()> {
    for (path, skill) in skills(root)? {
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
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        dotfiles_xtask::report(&error);
        std::process::exit(1);
    }
}
