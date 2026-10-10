//! `cargo xtask`, the repository's development tasks behind the just recipes.

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use platform::freshness::Built;
use skill_ops::{export_checks, skill_source_paths, skills, split_frontmatter, validate_tree};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use xtask::change_rules::{Gate, Gates};
use xtask::check_build_dir;
use xtask::environment;
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
    AnsibleSyntaxCheck,
    Profiles {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Checks every profile in the pinned Linux container.
    ProfilesLinux,
    /// Checks instruction, heap, and binary ceilings on the pinned Linux host.
    Benchmarks,
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
    /// Records every profile's terminal cell grids.
    ThemeGrids {
        #[arg(long)]
        write: bool,
    },
    /// Renders the fzf configuration on stdin as a JSON terminal cell grid.
    FzfGrid,
    /// Compares two saved cell grids in the pinned terminal recorder image without gating.
    ThemePixels(xtask::pixels::Request),
    /// Counts each shell's startup programs in an owned fixture or an applied home.
    StartupPrograms {
        #[arg(long, conflicts_with = "home", required_unless_present = "home")]
        profile: Option<String>,
        #[arg(long)]
        home: Option<PathBuf>,
    },
    PublicAudit {
        #[arg(long)]
        denylist: PathBuf,
    },
    Skills,
    /// Lists every computed tool, version selector, and profile, then checks skill coverage.
    Inventory,
    /// Apply the native profile, rerun every script on the provisioned host, and apply again, on a disposable CI host.
    Rehearse {
        #[arg(long)]
        disposable_host: bool,
    },
    /// Run the gates a change selects, or every gate without a scope.
    /// With `--staged`, run the lint stages and the prose checks over the staged files instead.
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
    /// Install the persisted prose checker and its pinned linters, and remove retired reply checks.
    InstallProse,
    /// Refuse a lint baseline that admits more than it did at `--base`, or rewrite every baseline from the tracked files with `--write`.
    Baselines {
        #[arg(long, conflicts_with = "write")]
        base: Option<String>,
        #[arg(long)]
        write: bool,
    },
    /// Prove the production contracts.
    /// A scope that selects no proofs gate skips them.
    Proofs {
        #[command(flatten)]
        scope: xtask::change::Scope,
    },
    /// Write the reachable state graph of every Quint model that records one, as `TLC` dumps it in the pinned image.
    Graphs,
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

/// The pre-commit check validates lint, document prose, and comments from the index snapshot.
/// The pre-push check runs the gates the whole change selects.
fn check_staged(root: &Path) -> Result<()> {
    let staged = xtask::change::paths(root, &xtask::change::Source::Staged)?;
    let growth = xtask::lint::baseline_growth(root, "HEAD")?;
    let snapshot = xtask::staged_snapshot::export(root)?;
    xtask::lint::check_content(snapshot.path(), &staged, false, growth)?;
    prose::repository(
        snapshot.path(),
        prose::Scope::Documents,
        &prose::Bundle::source(snapshot.path())?,
    )?;
    prose::repository(
        snapshot.path(),
        prose::Scope::Comments,
        &prose::Bundle::source(snapshot.path())?,
    )
}

/// Runs the checks of each selected gate.
/// A change that selects fewer gates skips only the checks it can't affect.
fn check(root: &Path, gates: Gates, reference: &str) -> Result<()> {
    if gates.contains(Gate::Prose) {
        xtask::lint::check(root, &xtask::lint::tracked(root)?, true, reference)?;
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
        dotfiles::repository::check(root, None)?;
    }
    if gates.contains(Gate::Skills) {
        skill_ops::check_catalog(root)?;
        xtask::inventory::check(root)?;
        println!(
            "Validated {} shared skills and aliases",
            validate_tree(root)?
        );
        xtask::quality::adapters(root)?;
    }
    if gates.contains(Gate::Rust) {
        xtask::profiles::data_schema(root, true)?;
        xtask::obligations::check(root, &xtask::obligations::github_issue_closed)?;
        xtask::quality::workflows(root)?;
        xtask::quality::dependencies(root)?;
        check_workspace(root)?;
        check_package(
            root,
            Path::new("home/dot_agents/skills/github-repository/scripts/repo-settings/Cargo.toml"),
        )?;
    }
    // The catalog deploys with the profiles, and the core it checks builds with the Rust packages.
    if gates.contains(Gate::Rust) || gates.contains(Gate::Profiles) {
        xtask::structure::check(root)?;
    }
    if gates.contains(Gate::Profiles) {
        xtask::profiles::source_data(root)?;
        xtask::launchers::generate(root, true)?;
        xtask::profiles::targets_page(root, true)?;
        xtask::quality::chezmoi_version(root)?;
        let templates = xtask::quality::template_findings(root)?;
        anyhow::ensure!(templates.is_empty(), "{}", templates.join("\n"));
        xtask::profiles::check_profiles(root, None)?;
        xtask::profile_snapshot::check(root)?;
        xtask::ignore_audit::check(root)?;
        xtask::input_audit::check(root)?;
    }
    println!("Checked the gates: {gates}");
    Ok(())
}

fn generate(root: &Path, check: bool) -> Result<()> {
    xtask::adr::generate(root, check)?;
    xtask::profiles::data_schema(root, check)?;
    xtask::obligations::schemas(root, check)?;
    xtask::lean::vectors(root, check)?;
    xtask::manifest_model::instance(root, check)?;
    xtask::launchers::generate(root, check)?;
    xtask::profiles::targets_page(root, check)
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();
    cli.root = xtask::canonical(&cli.root)?;
    match cli.command {
        Action::CheckSecrets => xtask::quality::secrets(&cli.root)?,
        Action::CheckAdapters => xtask::quality::adapters(&cli.root)?,
        Action::AnsibleSyntaxCheck => {
            ensure!(
                cfg!(target_os = "linux"),
                "Ansible checks require the Linux toolchain"
            );
            xtask::quality::ops(&cli.root)?;
        }
        Action::Profiles { output } => profiles(&cli.root, output.as_deref())?,
        Action::ProfilesLinux => xtask::linux_profiles::check(&cli.root)?,
        Action::Benchmarks => benchmarks(&cli.root)?,
        Action::Gen { check } => generate(&cli.root, check)?,
        Action::ThemeGrids { write } => xtask::profiles::write_theme_grids(&cli.root, write)?,
        Action::FzfGrid => fzf_grid(&cli.root)?,
        Action::ThemePixels(request) => xtask::pixels::advisory(&request),
        Action::StartupPrograms { profile, home } => {
            xtask::profiles::print_startup(&cli.root, profile.as_deref(), home.as_deref())?;
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
            if scope.resolve(&cli.root)?.contains(Gate::Proofs) {
                xtask::skill_proofs::verify(&cli.root)?;
            } else {
                println!(
                    "Skipped the proofs: the change touches no rule module, proof inventory, or tooling"
                );
            }
        }
        Action::Graphs => xtask::state_graph::write(&cli.root)?,
        Action::Classify { scope } => xtask::change::run(&cli.root, &scope)?,
        Action::Skills => {
            skill_ops::check_catalog(&cli.root)?;
            xtask::inventory::check(&cli.root)?;
            println!(
                "Validated {} shared skills, aliases, and current maintenance dispositions",
                validate_tree(&cli.root)?
            );
        }
        Action::Inventory => inventory(&cli.root)?,
        Action::Rehearse { disposable_host } => {
            xtask::rehearsal::run(&cli.root, disposable_host)?;
        }
        Action::Check { scope } if scope.staged() => check_staged(&cli.root)?,
        Action::Check { scope } => check(&cli.root, scope.resolve(&cli.root)?, scope.reference())?,
        Action::Baselines { base, write } => {
            if write {
                xtask::lint::write_baselines(&cli.root)?;
            } else {
                let refusals =
                    xtask::lint::baseline_growth(&cli.root, base.as_deref().unwrap_or("HEAD"))?;
                ensure!(refusals.is_empty(), "{}", refusals.join("\n"));
                println!("Every lint baseline only shrank");
            }
        }
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

/// Prints installed tools, their versions, and their profiles, and checks their skill coverage.
fn inventory(root: &Path) -> Result<()> {
    for (name, installed) in xtask::inventory::inventory(root, &xtask::inventory::policy(root)?)? {
        println!(
            "{name}\t{}\t{}",
            installed.versions.into_iter().collect::<Vec<_>>().join("|"),
            installed.profiles.into_iter().collect::<Vec<_>>().join(",")
        );
    }
    xtask::inventory::check(root)?;
    Ok(())
}

/// Applies the targets of the given source paths and their parent directories.
fn apply_sources<I, S>(root: &Path, include: &str, sources: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let status = Tool::chezmoi_in(root)
        .args([
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
        .args(["apply", "--include", "dirs,files", "--parent-dirs"])
        .arg(user_directory.join(".config/prose"))
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

/// The frontmatter keys a portable skill carries: its name and its description.
const PORTABLE_KEYS: usize = 2;

fn format_metadata(root: &Path) -> Result<()> {
    for (path, skill) in skills(root)? {
        let text = fs::read_to_string(&path)?;
        let (header, body) = split_frontmatter(&text)?;
        let documents = yaml_rust2::YamlLoader::load_from_str(header)?;
        ensure!(
            documents[0].as_hash().context("mapping required")?.len() == PORTABLE_KEYS,
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

fn fzf_grid(root: &Path) -> Result<()> {
    let mut config = String::new();
    std::io::stdin().read_to_string(&mut config)?;
    println!(
        "{}",
        serde_json::to_string(&xtask::theme::fzf_grid(root, &config)?)?
    );
    Ok(())
}

fn profiles(root: &Path, output: Option<&Path>) -> Result<()> {
    xtask::quality::chezmoi_version(root)?;
    xtask::profiles::check_profiles(root, output)?;
    xtask::profile_snapshot::check(root)?;
    xtask::ignore_audit::check(root)?;
    xtask::input_audit::check(root)
}

fn main() {
    if let Err(error) = environment::check().and_then(|()| run()) {
        xtask::report(&error);
        std::process::exit(1);
    }
}

fn benchmarks(root: &Path) -> Result<()> {
    if cfg!(target_os = "linux") {
        xtask::benchmarks::verify(root)
    } else {
        xtask::benchmarks::verify_in_container(root)
    }
}
