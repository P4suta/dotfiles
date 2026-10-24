use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use platform::local_issues as local;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct Project {
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

#[derive(Args, Debug)]
pub struct ImportScope {
    #[command(flatten)]
    project: Project,
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    issue: u64,
    #[arg(long = "path", required = true)]
    paths: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Status {
    Open,
    Closed,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Prepare local Git metadata and owner-local issue storage without a remote.
    Init {
        #[command(flatten)]
        project: Project,
    },
    /// Create a local issue from a checked title and body.
    Create {
        #[command(flatten)]
        project: Project,
        #[arg(long)]
        title: String,
        #[arg(long)]
        body_file: PathBuf,
    },
    /// Register an open local issue and scope before implementation edits.
    Select {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        issue: u64,
        #[arg(long = "path", required = true)]
        paths: Vec<PathBuf>,
    },
    /// Preview the exact staged artifacts of an unborn local project without granting authority.
    ImportPreview {
        #[command(flatten)]
        scope: ImportScope,
    },
    /// Attest the reviewed initial artifacts and grant forward local issue scope.
    Import {
        #[command(flatten)]
        scope: ImportScope,
        #[arg(long)]
        snapshot: String,
    },
    /// Read local issues or explicitly change one issue's open or closed state.
    Status {
        #[command(flatten)]
        project: Project,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        issue: Option<u64>,
        #[arg(long, requires = "issue")]
        state: Option<Status>,
    },
    /// Check local issue scope before the project's required verification stages.
    Check {
        #[command(flatten)]
        project: Project,
        #[arg(long, conflicts_with = "staged")]
        base: Option<String>,
        #[arg(long)]
        staged: bool,
    },
}

pub(super) fn run(action: Action) -> Result<()> {
    match action {
        Action::Init { project } => {
            local::init(&project.root)?;
            println!("Local issue project prepared; no publication authority granted");
        }
        Action::Create {
            project,
            title,
            body_file,
        } => {
            let body = std::fs::read_to_string(body_file).context("read local issue body")?;
            let document = super::validate_issue_contents(&title, body)?;
            prose::check_publication(
                &prose::Bundle::installed()?,
                prose::Publication::Issue,
                &document.title,
                &document.body,
            )?;
            let number = local::create(&project.root, document.title, document.body)?;
            println!("Local issue {number} created");
        }
        Action::Select {
            project,
            issue,
            paths,
        } => {
            local::select(&project.root, issue, &paths)?;
            println!("Local issue {issue} scope selected before supported edits");
        }
        Action::ImportPreview { scope } => {
            println!(
                "{}",
                serde_json::to_string(&local::import_preview(
                    &scope.project.root,
                    scope.issue,
                    &scope.paths
                )?)?
            );
        }
        Action::Import { scope, snapshot } => {
            local::import_initial(&scope.project.root, scope.issue, &scope.paths, &snapshot)?;
            println!(
                "Initial artifacts attested; local issue {} authorizes future scoped edits; no earlier issue chronology or publication authority granted",
                scope.issue
            );
        }
        Action::Status {
            project,
            issue,
            state,
        } => {
            let state = state.map(|state| match state {
                Status::Open => local::Status::Open,
                Status::Closed => local::Status::Closed,
            });
            println!(
                "{}",
                serde_json::to_string(&local::status(&project.root, issue, state)?)?
            );
        }
        Action::Check {
            project,
            base,
            staged,
        } => {
            local::check(&project.root, base.as_deref(), staged)?;
            println!(
                "Local issue context accepted; run the project's normal checks; verification has not run here"
            );
        }
    }
    Ok(())
}
