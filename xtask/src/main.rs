//! `cargo xtask`, the repository's development tasks behind the just recipes.

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use platform::freshness::Built;
use skill_ops::{export_checks, skill_source_paths, skills, split_frontmatter, validate_tree};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use xtask::change_rules::{Gate, Gates};
use xtask::check_build_dir;
use xtask::tool::Tool;

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    CheckSecrets,
    CheckAdapters,
    OpsCheck,
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
    Skills,
    /// Apply the native profile, rerun every script on the provisioned host, and apply again, on a disposable CI host.
    Rehearse {
        #[arg(long)]
        disposable_host: bool,
    },
    /// Run the gates a change selects, or every gate without a scope.
    Check {
        #[command(flatten)]
        scope: xtask::change::Scope,
    },
    /// Print `gates=<names>`: the gates the change set selects, or every gate without a scope.
    Classify {
        #[command(flatten)]
        scope: xtask::change::Scope,
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
    /// Prove the production contracts.
    /// A scope without the Rust gate skips them.
    Proofs {
        #[command(flatten)]
        scope: xtask::change::Scope,
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
        xtask::adr::generate(root, true)?;
        xtask::quality::editorconfig(root)?;
        xtask::quality::comment_scopes(root)?;
        xtask::instruction_audit::check(root)?;
        dotfiles::line_endings::check(root)?;
        let unknown = xtask::quality::unknown_recipes(root)?;
        anyhow::ensure!(
            unknown.is_empty(),
            "references to recipes the justfile does not define: {unknown:#?}"
        );
        xtask::quality::secrets(root)?;
        prose::repository(root, prose::Scope::Documents, &prose::Bundle::source(root)?)?;
    }
    if gates.contains(Gate::Skills) {
        skill_ops::check_catalog(root)?;
        println!(
            "Validated {} shared skills and aliases",
            validate_tree(root)?
        );
        xtask::quality::adapters(root)?;
    }
    if gates.contains(Gate::Rust) {
        xtask::profiles::data_schema(root, true)?;
        xtask::quality::workflows(root)?;
        xtask::quality::dependencies(root)?;
        check_workspace(root)?;
        check_package(
            root,
            Path::new("home/dot_agents/skills/github-repository/scripts/repo-settings/Cargo.toml"),
        )?;
    }
    if gates.contains(Gate::Profiles) {
        xtask::profiles::source_data(root)?;
        xtask::profiles::targets_page(root, true)?;
        xtask::quality::chezmoi_version(root)?;
        let templates = xtask::quality::template_findings(root)?;
        anyhow::ensure!(templates.is_empty(), "{}", templates.join("\n"));
        xtask::profiles::check_profiles(root, None)?;
        xtask::profile_snapshot::check(root)?;
        xtask::ignore_audit::check(root)?;
        xtask::input_audit::check(root)?;
    }
    println!("Checked the gates: {}", gates.names());
    Ok(())
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();
    cli.root = xtask::canonical(&cli.root)?;
    match cli.command {
        Action::CheckSecrets => xtask::quality::secrets(&cli.root)?,
        Action::CheckAdapters => xtask::quality::adapters(&cli.root)?,
        Action::OpsCheck => {
            ensure!(
                cfg!(target_os = "linux"),
                "Ansible checks require the Linux toolchain"
            );
            xtask::quality::ops(&cli.root)?;
        }
        Action::Profiles { output } => {
            xtask::quality::chezmoi_version(&cli.root)?;
            xtask::profiles::check_profiles(&cli.root, output.as_deref())?;
            xtask::profile_snapshot::check(&cli.root)?;
            xtask::ignore_audit::check(&cli.root)?;
            xtask::input_audit::check(&cli.root)?;
        }
        Action::Gen { check } => {
            xtask::adr::generate(&cli.root, check)?;
            xtask::profiles::data_schema(&cli.root, check)?;
            xtask::profiles::targets_page(&cli.root, check)?;
        }
        Action::ProfileSnapshots { write } => {
            if write {
                xtask::profile_snapshot::write(&cli.root)?;
            }
            xtask::profile_snapshot::check(&cli.root)?;
            xtask::input_audit::check(&cli.root)?;
        }
        Action::PublicAudit { denylist } => {
            xtask::profiles::audit_private(&cli.root, &denylist)?;
        }
        Action::InstallPrWorkflow => install_pr_workflow(&cli.root)?,
        Action::InstallProse => install_prose(&cli.root)?,
        Action::Proofs { scope } => {
            if scope.resolve(&cli.root)?.contains(Gate::Rust) {
                xtask::skill_proofs::verify(&cli.root)?;
            } else {
                println!("Skipped the proofs: the change selects no Rust gate");
            }
        }
        Action::Classify { scope } => xtask::change::run(&cli.root, &scope)?,
        Action::Skills => {
            skill_ops::check_catalog(&cli.root)?;
            println!(
                "Validated {} shared skills, aliases, and current maintenance dispositions",
                validate_tree(&cli.root)?
            );
        }
        Action::Rehearse { disposable_host } => {
            xtask::rehearsal::run(&cli.root, disposable_host)?;
        }
        Action::Check { scope } => check(&cli.root, scope.resolve(&cli.root)?)?,
        Action::Decide { skill } => {
            println!("{}", skill_ops::decide(&cli.root, &skill)?);
        }
        Action::Next { offline, execute } => {
            xtask::next_action::run(&cli.root, offline, execute)?;
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

fn user_directory() -> Result<PathBuf> {
    platform::home::home().context("user directory is unavailable")
}

/// Builds one binary of `package` in release mode and returns where it landed.
fn release_binary(root: &Path, package: &str, name: &str) -> Result<PathBuf> {
    cargo(
        root,
        Path::new("Cargo.toml"),
        &[
            "build",
            "--locked",
            "--release",
            "--package",
            package,
            "--bin",
            name,
        ],
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
    let binary = release_binary(root, "pr-workflow", "pr-workflow")?;
    let user_directory = user_directory()?;
    pr_workflow::deploy(root, &binary, &user_directory, |paths| {
        apply_sources(root, "dirs,files,symlinks", paths)
            .context("scoped PR skill installation failed")
    })?;
    platform::freshness::record_installation(&user_directory, root, Built::PrWorkflow)
}

fn install_prose(root: &Path) -> Result<()> {
    let binary = release_binary(root, "prose", "prose")?;
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
    prose::install(root, &binary, &user_directory)
}

fn install_review_guard(root: &Path) -> Result<()> {
    let binary = release_binary(root, "coderabbit-guard", "coderabbit")?;
    apply_sources(
        root,
        "dirs,files",
        [root.join(coderabbit_guard::SOURCE_POLICY)],
    )
    .context("review guard policy installation failed")?;
    let user_directory = user_directory()?;
    coderabbit_guard::install(&binary, &user_directory)?;
    platform::freshness::record_installation(&user_directory, root, Built::Coderabbit)?;
    println!("Installed CodeRabbit guard and cr alias with persistent local limits");
    Ok(())
}

fn install_skill_ops(root: &Path) -> Result<()> {
    skill_ops::check_catalog(root)?;
    let binary = release_binary(root, "skill-ops", "skill-ops")?;
    let user_directory = user_directory()?;
    skill_ops::skill_install::deploy(root, &binary, &user_directory, |path| {
        apply_sources(root, "dirs,files", [path])
            .with_context(|| format!("installation failed for {}", path.display()))
    })?;
    platform::freshness::record_installation(&user_directory, root, Built::SkillOps)
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
        xtask::report(&error);
        std::process::exit(1);
    }
}
