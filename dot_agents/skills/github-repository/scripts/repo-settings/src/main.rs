use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use repo_settings::{Gh, Plan, Profile, Scope, Snapshot, apply, audit, discover, plan_scoped};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "Audit, plan, apply, and verify GitHub settings through gh; never publish releases"
)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Capture repository settings, rulesets, and environment protections without reading secrets.
    Audit {
        #[arg(long)]
        owner: Option<String>,
        #[arg(long = "repo")]
        repositories: Vec<String>,
        #[arg(long, requires = "owner")]
        include_private: bool,
        #[arg(long)]
        output: PathBuf,
    },
    /// Produce a reviewable JSON plan using the bundled baseline or an explicit profile.
    Plan {
        #[arg(long)]
        snapshot: PathBuf,
        #[arg(long)]
        profile: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "governance")]
        scope: Scope,
        #[arg(long)]
        output: PathBuf,
    },
    /// Apply an unchanged plan after checking live settings, and record every verified change.
    Apply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Read live settings again and fail if any managed setting differs from the baseline.
    Verify {
        #[arg(long)]
        snapshot: PathBuf,
        #[arg(long)]
        profile: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "governance")]
        scope: Scope,
        #[arg(long)]
        output: PathBuf,
    },
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("decode {}", path.display()))
}

fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create {}; choose a fresh output path", path.display()))?;
    serde_json::to_writer_pretty(&mut output, value)?;
    writeln!(output)?;
    Ok(())
}

fn profile(path: Option<PathBuf>) -> Result<Profile> {
    path.as_deref().map_or_else(Profile::baseline, read)
}

fn capture(names: BTreeSet<String>) -> Result<Snapshot> {
    ensure!(!names.is_empty(), "specify --owner or at least one --repo");
    let mut repositories = Vec::new();
    let names: Vec<_> = names.into_iter().collect();
    for batch in names.chunks(8) {
        let captured = std::thread::scope(|scope| -> Result<Vec<_>> {
            let handles: Vec<_> = batch
                .iter()
                .map(|name| {
                    eprintln!("Auditing {name}");
                    scope.spawn(move || audit(&Gh, name))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| anyhow::anyhow!("audit worker panicked"))?
                })
                .collect()
        })?;
        repositories.extend(captured);
    }
    Ok(Snapshot {
        schema_version: 1,
        repositories,
    })
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Action::Audit {
            owner,
            repositories,
            include_private,
            output,
        } => {
            let mut names: BTreeSet<_> = repositories.into_iter().collect();
            if let Some(owner) = owner {
                names.extend(discover(&Gh, &owner, include_private)?);
            }
            write(&output, &capture(names)?)?;
        }
        Action::Plan {
            snapshot,
            profile: path,
            scope,
            output,
        } => {
            let planned = plan_scoped(read(&snapshot)?, profile(path)?, scope)?;
            eprintln!(
                "{} repositories, {} changes, {} findings",
                planned.snapshot.repositories.len(),
                planned.changes.len(),
                planned.findings.len()
            );
            write(&output, &planned)?;
        }
        Action::Apply { plan: path, report } => {
            let planned: Plan = read(&path)?;
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&report)
                .context("create fresh apply report")?;
            let mut output = BufWriter::new(file);
            apply(&Gh, &planned, |event| {
                serde_json::to_writer(&mut output, event)?;
                writeln!(output)?;
                output.flush()?;
                if let repo_settings::ApplyEvent::Verified { change } = event {
                    eprintln!(
                        "Verified {}: {}",
                        change.repository,
                        serde_json::to_value(&change.operation)?["kind"]
                    );
                }
                Ok(())
            })?;
            eprintln!("Applied and verified {} changes", planned.changes.len());
        }
        Action::Verify {
            snapshot,
            profile: path,
            scope,
            output,
        } => {
            let previous: Snapshot = read(&snapshot)?;
            ensure!(previous.schema_version == 1, "unsupported snapshot schema");
            let fresh = capture(
                previous
                    .repositories
                    .into_iter()
                    .map(|state| state.repository.full_name)
                    .collect(),
            )?;
            let result = plan_scoped(fresh, profile(path)?, scope)?;
            write(&output, &result)?;
            ensure!(
                result.changes.is_empty(),
                "{} changes remain; see {}",
                result.changes.len(),
                output.display()
            );
            eprintln!(
                "Verified {} repositories against the {scope:?} scope",
                result.snapshot.repositories.len()
            );
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
