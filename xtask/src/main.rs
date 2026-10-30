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
use xtask::checks::Checks;
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
    DopplerMigrate(xtask::doppler_migration::Request),
    DopplerOrganize(xtask::doppler_migration::organization::Request),
    CheckSecrets,
    CheckAdapters,
    /// Checks portable uv locks for every tool, regardless of its OS filter.
    ToolLocks,
    AnsibleSyntaxCheck,
    Profiles {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Checks every profile in the pinned Linux container.
    ProfilesLinux,
    /// Checks instruction, heap, and binary ceilings on the pinned Linux host.
    Benchmarks {
        /// Run the benchmarks in the pinned Linux container.
        #[arg(long)]
        container: bool,
    },
    #[command(hide = true)]
    BenchmarksReport,
    /// Observes a Mac guest profile without granting benchmark verification credit.
    BenchmarksObserve {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        metadata_only: bool,
    },
    #[command(hide = true)]
    BenchmarksObserveReport,
    #[command(hide = true)]
    BenchmarksCpuFeatures,
    /// Writes the generated files: the architecture decision index.
    Gen {
        /// Refuses a stale generated file instead of writing it.
        #[arg(long)]
        check: bool,
        /// Vendors the Japanese kanji repertoire from this verified `Unihan_IRGSources.txt` first.
        #[arg(long)]
        irg_sources: Option<PathBuf>,
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
    /// Checks the complete candidate, including the proof suite on macOS.
    PushCheck,
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
    /// Prints skill content revisions without running maintenance analysis.
    SkillRevisions,
    Install,
    InstallReviewGuard,
    InstallSkillOps,
    InstallPrWorkflow,
    /// Refresh the verified PR workflow executable without applying shared skills.
    InstallPrWorkflowBinary,
    /// Build the pinned answer proof image and record its immutable identity on this host.
    InstallAnswerProver,
    /// Verify the answer gate's registered Kani contracts, rejecting probes, and parser corpus.
    AnswerProofs {
        /// Use an explicit bounded policy for this owner-side run.
        #[arg(long)]
        policy: Option<PathBuf>,
    },
    /// Install the persisted prose checker and its pinned linters, and remove retired reply checks.
    InstallProse,
    /// Refuse a lint baseline that admits more than it did at `--base`, or rewrite every baseline from the tracked files with `--write`.
    Baselines {
        #[arg(long, conflicts_with = "write")]
        base: Option<String>,
        #[arg(long)]
        write: bool,
        /// Refresh the static proof counts without admitting reductions or uncovered subjects.
        #[arg(long, requires = "write")]
        proof_counts: bool,
    },
    /// Prove the production contracts.
    /// A scope that selects no proofs gate skips them.
    Proofs {
        #[command(flatten)]
        scope: xtask::change::Scope,
        /// Verify only these registered Kani rule modules and their rejecting probes.
        #[arg(long = "source")]
        sources: Vec<String>,
        /// Limit an owner-side bounded run to this explicit time window.
        /// A limit stop leaves verification incomplete.
        #[arg(long)]
        window_seconds: Option<std::num::NonZeroU32>,
    },
    /// Run the CI proof campaign, deferring mutation and its fixture to the full native campaign.
    ProofsCi {
        /// One required hosted matrix partition.
        /// Without this flag, run the complete campaign.
        #[arg(long)]
        phase: Option<xtask::proof_ci_phase::Phase>,
    },
    /// Prepare required verifier environments, inspect readiness without installation, or run semantic smoke controls.
    VerificationEnv(xtask::verification_env::Request),
    /// Assess the pinned mutation runner without replacing native mutation evidence.
    NjutestAssessment(xtask::njutest_assessment::Request),
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
fn check_workspace(root: &Path, checks: &mut Checks) {
    let manifest = Path::new("Cargo.toml");
    checks.run("rust.fmt", || {
        cargo(root, manifest, &["fmt", "--all", "--check"])
    });
    checks.run("rust.clippy", || {
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
    });
    checks.run("rust.guard", || {
        cargo(
            root,
            manifest,
            &["check", "--locked", "--package", "dotguard", "--bins"],
        )
    });
    checks.run("rust.tests", || {
        cargo(
            root,
            manifest,
            &["test", "--locked", "--workspace", "--all-targets"],
        )
    });
}

fn check_package(root: &Path, manifest: &Path, checks: &mut Checks) {
    checks.run("settings.fmt", || {
        cargo(root, manifest, &["fmt", "--check"])
    });
    checks.run("settings.clippy", || {
        cargo(
            root,
            manifest,
            &[
                "clippy",
                "--locked",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        )
    });
    checks.run("settings.tests", || {
        cargo(root, manifest, &["test", "--locked", "--all-targets"])
    });
}

/// The pre-commit check validates lint, document prose, and comments from the index snapshot.
/// The pre-push check runs the gates the whole change selects.
fn check_staged(root: &Path) -> Result<()> {
    pr_workflow::check_issue_context(root, None, true)?;
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

/// Where the issue context of a change gets checked.
#[derive(Clone, Copy, PartialEq)]
enum IssueContext {
    /// Owner-local state on this host.
    Local,
    /// The originating host, before the push that sent this captured candidate, because the context stays in that host's owner-local state.
    Origin,
}

/// Runs the checks of each selected gate.
/// A change that selects fewer gates skips only the checks it can't affect.
fn check(root: &Path, gates: Gates, reference: &str, context: IssueContext) -> Result<()> {
    let mut checks = Checks::default();
    checks.run("tools.locks", || xtask::tool_locks::check(root));
    if context == IssueContext::Local {
        checks.run("issue.context", || {
            pr_workflow::check_issue_context(
                root,
                (reference != "HEAD").then_some(reference),
                false,
            )
        });
    } else {
        println!(
            "Skipping issue.context: the originating host checked it before push, and its owner-local state stays there"
        );
    }
    if gates.contains(Gate::Prose) {
        check_prose(root, reference, &mut checks);
    }
    if gates.contains(Gate::Skills) {
        checks.run("skills.catalog", || skill_ops::check_catalog(root));
        checks.run("skills.inventory", || xtask::inventory::check(root));
        checks.run("skills.validation", || {
            println!(
                "Validated {} shared skills and aliases",
                validate_tree(root)?
            );
            Ok(())
        });
        checks.run("skills.adapters", || xtask::quality::adapters(root));
    }
    if gates.contains(Gate::Rust) {
        checks.run("rust.data-schema", || {
            xtask::profiles::data_schema(root, true)
        });
        checks.run("rust.obligations", || {
            xtask::obligations::check(root, &xtask::obligations::github_issue_closed)
        });
        checks.run("rust.workflows", || xtask::quality::workflows(root));
        checks.run("rust.lockfiles", || xtask::quality::locked_workspaces(root));
        checks.run("rust.dependencies", || xtask::quality::dependencies(root));
        check_workspace(root, &mut checks);
        check_package(
            root,
            Path::new("home/dot_agents/skills/github-repository/scripts/repo-settings/Cargo.toml"),
            &mut checks,
        );
    }
    // The catalog deploys with the profiles, and the core it checks builds with the Rust packages.
    if gates.contains(Gate::Rust) || gates.contains(Gate::Profiles) {
        checks.run("structure", || xtask::structure::check(root));
    }
    if gates.contains(Gate::Profiles) {
        check_profiles(root, &mut checks);
    }
    checks.finish()?;
    println!("Checked the gates: {gates}");
    Ok(())
}

fn check_prose(root: &Path, reference: &str, checks: &mut Checks) {
    checks.run("prose.lint", || {
        xtask::lint::check(root, &xtask::lint::source_files(root)?, true, reference)
    });
    checks.run("prose.adr", || xtask::adr::generate(root, true));
    checks.run("prose.editorconfig", || xtask::quality::editorconfig(root));
    checks.run("prose.comments", || xtask::quality::comment_scopes(root));
    checks.run("prose.comment-style", || {
        prose::repository(root, prose::Scope::Comments, &prose::Bundle::source(root)?)
    });
    checks.run("prose.instructions", || {
        xtask::instruction_audit::check(root)
    });
    checks.run("prose.line-endings", || dotfiles::line_endings::check(root));
    checks.run("prose.recipes", || {
        let unknown = xtask::quality::unknown_recipes(root)?;
        ensure!(
            unknown.is_empty(),
            "references to recipes the justfile does not define: {unknown:#?}"
        );
        Ok(())
    });
    checks.run("prose.secrets", || xtask::quality::secrets(root));
    checks.run("prose.documents", || {
        prose::repository(root, prose::Scope::Documents, &prose::Bundle::source(root)?)
    });
    checks.run("prose.repository", || {
        dotfiles::repository::check(root, None)
    });
}

fn check_profiles(root: &Path, checks: &mut Checks) {
    checks.run("profiles.data", || xtask::profiles::source_data(root));
    checks.run("profiles.launchers", || {
        xtask::launchers::generate(root, true)
    });
    checks.run("profiles.targets", || {
        xtask::profiles::targets_page(root, true)
    });
    checks.run("profiles.chezmoi", || xtask::quality::chezmoi_version(root));
    checks.run("profiles.templates", || {
        let templates = xtask::quality::template_findings(root)?;
        ensure!(templates.is_empty(), "{}", templates.join("\n"));
        Ok(())
    });
    checks.run("profiles.rendering", || {
        xtask::profiles::check_profiles(root, None)
    });
    checks.run("profiles.snapshots", || {
        xtask::profile_snapshot::check(root)
    });
    checks.run("profiles.ignore", || xtask::ignore_audit::check(root));
    checks.run("profiles.inputs", || xtask::input_audit::check(root));
}

fn generate(root: &Path, check: bool, irg_sources: Option<&Path>) -> Result<()> {
    if let Some(irg_sources) = irg_sources {
        xtask::kanji_repertoire::refresh(root, irg_sources)?;
    }
    xtask::adr::generate(root, check)?;
    xtask::profiles::data_schema(root, check)?;
    xtask::obligations::schemas(root, check)?;
    xtask::lean::vectors(root, check)?;
    xtask::kanji_repertoire::table(root, check)?;
    xtask::manifest_model::instance(root, check)?;
    xtask::launchers::generate(root, check)?;
    xtask::njutest_assessment::generate(root, check)?;
    xtask::profiles::targets_page(root, check)
}

fn observe_benchmarks(root: &Path, output: &Path, metadata_only: bool) -> Result<()> {
    if metadata_only {
        return observe_benchmarks_prepared(root, output, true);
    }
    xtask::quality::with_locked_workspaces(root, || {
        observe_benchmarks_prepared(root, output, false)
    })
}

fn observe_benchmarks_prepared(root: &Path, output: &Path, metadata_only: bool) -> Result<()> {
    xtask::phase_evidence::run(
        root,
        "non-verifying Mac benchmark observation",
        if metadata_only {
            0
        } else {
            xtask::benchmarks::FUNCTIONS.len()
        },
        || xtask::benchmarks::observe_mac(root, output, metadata_only),
    )
}

fn report_benchmark_observation(root: &Path) -> Result<()> {
    xtask::quality::with_locked_workspaces(root, || {
        xtask::benchmarks::write_observation_report(root)
    })
}

fn proofs(
    root: &Path,
    scope: &xtask::change::Scope,
    sources: &[String],
    window: Option<std::num::NonZeroU32>,
) -> Result<()> {
    if (!sources.is_empty() || scope.resolve(root)?.contains(Gate::Proofs))
        && let Some(window) = window
    {
        return xtask::skill_proofs::verify_window(root, sources, window);
    }
    if !sources.is_empty() {
        return xtask::skill_proofs::verify_sources(root, sources);
    }
    if scope.resolve(root)?.contains(Gate::Proofs) {
        xtask::skill_proofs::verify(root)
    } else {
        println!(
            "Skipped the proofs: the change touches no rule module, proof inventory, or tooling"
        );
        Ok(())
    }
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();
    cli.root = xtask::canonical(&cli.root)?;
    match cli.command {
        Action::DopplerMigrate(request) => xtask::doppler_migration::run(&request)?,
        Action::DopplerOrganize(request) => xtask::doppler_migration::organization::run(&request)?,
        Action::PushCheck => push_check(&cli.root)?,
        Action::CheckSecrets => xtask::quality::secrets(&cli.root)?,
        Action::ToolLocks => xtask::tool_locks::check(&cli.root)?,
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
        Action::Benchmarks { container } => benchmarks(&cli.root, container)?,
        Action::BenchmarksReport => xtask::quality::with_locked_workspaces(&cli.root, || {
            xtask::benchmarks::write_container_report(&cli.root)
        })?,
        Action::BenchmarksObserve {
            output,
            metadata_only,
        } => observe_benchmarks(&cli.root, &output, metadata_only)?,
        Action::BenchmarksObserveReport => report_benchmark_observation(&cli.root)?,
        Action::BenchmarksCpuFeatures => xtask::benchmark_profile::print_processor_features()?,
        Action::Gen { check, irg_sources } => generate(&cli.root, check, irg_sources.as_deref())?,
        Action::ThemeGrids { write } => xtask::profiles::write_theme_grids(&cli.root, write)?,
        Action::FzfGrid => fzf_grid(&cli.root)?,
        Action::ThemePixels(request) => xtask::pixels::advisory(&request),
        Action::StartupPrograms { profile, home } => {
            xtask::profiles::print_startup(&cli.root, profile.as_deref(), home.as_deref())?;
        }
        Action::ProfileSnapshots { write } => profile_snapshots(&cli.root, write)?,
        Action::PublicAudit { denylist } => xtask::profiles::audit_private(&cli.root, &denylist)?,
        Action::InstallPrWorkflow => install_pr_workflow(&cli.root)?,
        Action::InstallPrWorkflowBinary => install_pr_workflow_binary(&cli.root)?,
        Action::InstallAnswerProver => install_answer_prover(&cli.root)?,
        Action::AnswerProofs { policy } => {
            xtask::skill_proofs::verify_answer(&cli.root, policy.as_deref())?;
        }
        Action::InstallProse => install_prose(&cli.root)?,
        Action::Proofs {
            scope,
            sources,
            window_seconds,
        } => proofs(&cli.root, &scope, &sources, window_seconds)?,
        Action::ProofsCi { phase: Some(phase) } => xtask::proof_ci_phase::verify(&cli.root, phase)?,
        Action::ProofsCi { phase: None } => xtask::skill_proofs::verify_ci(&cli.root)?,
        Action::VerificationEnv(request) => xtask::verification_env::run(&cli.root, &request)?,
        Action::NjutestAssessment(request) => xtask::njutest_assessment::run(&cli.root, request)?,
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
        Action::Check { scope } => check(
            &cli.root,
            scope.resolve(&cli.root)?,
            scope.reference(),
            IssueContext::Local,
        )?,
        Action::Baselines {
            base,
            write,
            proof_counts,
        } => baselines(&cli.root, base.as_deref(), write, proof_counts)?,
        Action::Decide { skill } => {
            println!("{}", skill_ops::decide(&cli.root, &skill)?);
        }
        Action::Next { offline, execute } => {
            xtask::next_action::run(&cli.root, offline, execute)?;
        }
        Action::Aliases => aliases(&cli.root)?,
        Action::FormatMetadata => format_metadata(&cli.root)?,
        Action::Snapshot { output } => export_checks(&cli.root, &output)?,
        Action::SkillPaths => print_skill_paths(&cli.root)?,
        Action::SkillRevisions => print_skill_revisions(&cli.root)?,
        Action::Install => install_shared_skills(&cli.root)?,
        Action::InstallReviewGuard => install_review_guard(&cli.root)?,
        Action::InstallSkillOps => install_skill_ops(&cli.root)?,
    }
    Ok(())
}

fn print_skill_paths(root: &Path) -> Result<()> {
    let mut output = std::io::stdout().lock();
    for path in skill_source_paths(root)? {
        output.write_all(path.to_str().context("non-UTF-8 skill source")?.as_bytes())?;
        output.write_all(&[0])?;
    }
    Ok(())
}

fn baselines(root: &Path, base: Option<&str>, write: bool, proof_counts: bool) -> Result<()> {
    if proof_counts {
        xtask::obligations::write_proof_counts(root)?;
    } else if write {
        xtask::lint::write_baselines(root)?;
    } else {
        let refusals = xtask::lint::baseline_growth(root, base.unwrap_or("HEAD"))?;
        ensure!(refusals.is_empty(), "{}", refusals.join("\n"));
        println!("Every lint baseline only shrank");
    }
    Ok(())
}

fn print_skill_revisions(root: &Path) -> Result<()> {
    let revisions: std::collections::BTreeMap<_, _> =
        skill_ops::catalog(&root.join("home/dot_agents/skills"))?
            .skills
            .into_iter()
            .map(|(name, skill)| (name, skill.revision))
            .collect();
    println!("{}", serde_json::to_string_pretty(&revisions)?);
    Ok(())
}

fn push_check(root: &Path) -> Result<()> {
    use rules::hosts_rules::Family;
    let family = match std::env::consts::OS {
        "linux" => Family::Linux,
        "macos" => Family::Mac,
        "windows" => Family::Windows,
        _ => anyhow::bail!("unsupported native operating system"),
    };
    xtask::skill_proofs::verify_push(
        family,
        || check(root, Gates::ALL, "HEAD", IssueContext::Origin),
        || xtask::skill_proofs::verify(root),
        || benchmarks(root, true),
    )
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

const SKILL_TARGETS: [&str; 3] = [
    ".agents/skills",
    ".claude/skills",
    ".config/skill-ops/policy.json",
];

fn install_shared_skills(root: &Path) -> Result<()> {
    skill_ops::check_catalog(root)?;
    ensure!(
        !skill_source_paths(root)?.is_empty(),
        "no skill files to install"
    );
    let user_directory = user_directory()?;
    let status = Tool::chezmoi_in(root)
        .args(["apply", "--include", "dirs,files,symlinks", "--parent-dirs"])
        .args(SKILL_TARGETS.map(|target| user_directory.join(target)))
        .status()
        .context("scoped skill installation failed")?;
    ensure!(
        status.success(),
        "scoped skill installation failed with {status}"
    );
    skill_ops::skill_install::install_catalog(
        root,
        &user_directory,
        xtask::profiles::native_profile()?,
    )
}

fn install_pr_workflow(root: &Path) -> Result<()> {
    let binary = release_binary(root, "pr-workflow", "pr-workflow")?;
    platform::freshness::verify_artifact(root, &binary, Built::PrWorkflow)?;
    let user_directory = user_directory()?;
    pr_workflow::deploy(root, &binary, &user_directory, |paths| {
        apply_sources(root, "dirs,files,symlinks", paths)
            .context("scoped PR skill installation failed")
    })?;
    platform::freshness::record_installation(&user_directory, root, Built::PrWorkflow)
}

fn install_pr_workflow_binary(root: &Path) -> Result<()> {
    let binary = release_binary(root, "pr-workflow", "pr-workflow")?;
    pr_workflow::install_binary(root, &binary, &user_directory()?)
}

fn profile_snapshots(root: &Path, write: bool) -> Result<()> {
    if write {
        xtask::profile_snapshot::write(root)?;
    }
    xtask::profile_snapshot::check(root)?;
    xtask::input_audit::check(root)
}

fn install_answer_prover(root: &Path) -> Result<()> {
    let image = xtask::lean::image(root)?;
    xtask::lean::prepare_image(root, &image)?;
    let output = Tool::Docker
        .command()
        .args(["image", "inspect", "--format", "{{.Id}}", &image])
        .output()?;
    ensure!(
        output.status.success(),
        "inspect the pinned answer proof image"
    );
    let image = String::from_utf8(output.stdout)?.trim().to_owned();
    let path = user_directory()?.join(".config/dotfiles/answer-proof.json");
    std::fs::create_dir_all(
        path.parent()
            .context("answer prover configuration has no parent")?,
    )?;
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&dotfiles::answer::Prover {
            version: 1,
            image,
            lean_version: dotfiles::answer::lean_version()?.to_owned(),
        })?,
    )?;
    Ok(())
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
    platform::freshness::verify_artifact(root, &binary, Built::Coderabbit)?;
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
    let user_directory = user_directory()?;
    if !platform::incidents::enabled_at(&user_directory) {
        return skill_ops::skill_install::remove_native_hooks(&user_directory);
    }
    skill_ops::check_catalog(root)?;
    let binary = release_binary(root, "skill-ops", "skill-ops")?;
    platform::freshness::verify_artifact(root, &binary, Built::SkillOps)?;
    skill_ops::skill_install::deploy(
        root,
        &binary,
        &user_directory,
        xtask::profiles::native_profile()?,
        |path| {
            apply_sources(root, "dirs,files", [path])
                .with_context(|| format!("installation failed for {}", path.display()))
        },
    )?;
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

fn benchmarks(root: &Path, container: bool) -> Result<()> {
    xtask::quality::with_locked_workspaces(root, || benchmarks_prepared(root, container))
}

fn benchmarks_prepared(root: &Path, container: bool) -> Result<()> {
    xtask::phase_evidence::run(
        root,
        "fixed Linux benchmark budget",
        xtask::benchmarks::FUNCTIONS.len(),
        || {
            if container || !cfg!(target_os = "linux") {
                xtask::benchmarks::verify_in_container(root)
            } else {
                xtask::benchmarks::verify(root)
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_observation_is_closed_and_never_accepts_a_verification_flag() {
        assert!(matches!(
            Cli::try_parse_from([
                "xtask",
                "benchmarks-observe",
                "--metadata-only",
                "--output",
                "fresh.json"
            ])
            .unwrap()
            .command,
            Action::BenchmarksObserve {
                metadata_only: true,
                ..
            }
        ));
        assert!(Cli::try_parse_from(["xtask", "benchmarks-observe"]).is_err());
        for flag in ["--verify", "--accept", "--skip", "--reuse"] {
            assert!(
                Cli::try_parse_from([
                    "xtask",
                    "benchmarks-observe",
                    "--output",
                    "fresh.json",
                    flag
                ])
                .is_err()
            );
            assert!(Cli::try_parse_from(["xtask", "benchmarks-observe-report", flag]).is_err());
        }
    }

    #[test]
    fn benchmark_transport_is_a_distinct_fixed_container_entry() {
        assert!(matches!(
            Cli::try_parse_from(["xtask", "benchmarks-report"])
                .unwrap()
                .command,
            Action::BenchmarksReport
        ));
        for args in [
            vec!["xtask", "benchmarks-report", "--container"],
            vec!["xtask", "benchmarks-report", "--report", "old.json"],
            vec!["xtask", "benchmarks-report", "--skip"],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn proof_count_refresh_requires_an_explicit_write() {
        assert!(Cli::try_parse_from(["xtask", "baselines", "--write", "--proof-counts"]).is_ok());
        assert!(Cli::try_parse_from(["xtask", "baselines", "--proof-counts"]).is_err());
        assert!(
            Cli::try_parse_from([
                "xtask",
                "baselines",
                "--write",
                "--proof-counts",
                "--base",
                "HEAD"
            ])
            .is_err()
        );
    }

    #[test]
    fn assessment_entry_keeps_inspection_preparation_and_execution_explicit() {
        for args in [
            vec!["xtask", "njutest-assessment", "prepare"],
            vec!["xtask", "njutest-assessment", "plan"],
            vec![
                "xtask",
                "njutest-assessment",
                "inspect",
                "catalog",
                "catalog.json",
            ],
            vec![
                "xtask",
                "njutest-assessment",
                "run",
                "--temperature",
                "warm",
            ],
        ] {
            assert!(matches!(
                Cli::try_parse_from(args).unwrap().command,
                Action::NjutestAssessment(_)
            ));
        }
        for args in [
            vec!["xtask", "njutest-assessment", "adopt"],
            vec!["xtask", "njutest-assessment", "run"],
            vec![
                "xtask",
                "njutest-assessment",
                "run",
                "--temperature",
                "skipped",
            ],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn the_ci_proof_entry_runs_the_complete_ci_campaign_without_a_scope() {
        let cli = Cli::try_parse_from(["xtask", "proofs-ci"]).unwrap();
        assert!(matches!(cli.command, Action::ProofsCi { phase: None }));
        for option in ["--staged", "--base", "--no-verify"] {
            assert!(Cli::try_parse_from(["xtask", "proofs-ci", option]).is_err());
        }
    }

    #[test]
    fn owner_proof_windows_require_positive_seconds_and_preserve_selected_sources() {
        let cli = Cli::try_parse_from([
            "xtask",
            "proofs",
            "--source",
            "crates/rules/src/proof_budget_rules.rs",
            "--window-seconds",
            "900",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Action::Proofs { sources, window_seconds: Some(seconds), .. }
                if sources == ["crates/rules/src/proof_budget_rules.rs"] && seconds.get() == 900
        ));
        for value in ["0", "-1", "4294967296", "unknown"] {
            assert!(Cli::try_parse_from(["xtask", "proofs", "--window-seconds", value]).is_err());
        }
        assert!(matches!(
            Cli::try_parse_from(["xtask", "proofs"]).unwrap().command,
            Action::Proofs {
                window_seconds: None,
                ..
            }
        ));
    }

    #[test]
    fn the_ci_phase_entry_accepts_only_the_complete_closed_phase_inventory() {
        for phase in xtask::proof_ci_phase::Phase::ALL {
            let cli = Cli::try_parse_from(["xtask", "proofs-ci", "--phase", phase.name()]).unwrap();
            assert!(
                matches!(cli.command, Action::ProofsCi { phase: Some(actual) } if actual == phase)
            );
        }
        for phase in ["mutation", "none", "all", "skipped", "kani,runtime"] {
            assert!(Cli::try_parse_from(["xtask", "proofs-ci", "--phase", phase]).is_err());
        }
    }
}
