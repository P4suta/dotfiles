//! The gates a change selects, read from Git; CI jobs and local hooks use this one implementation.

use crate::change_rules::{Gates, select};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::path::Path;

/// Which change set to classify.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Commits on `HEAD` since it diverged from the reference, as `git diff REF...HEAD` reports them.
    Base(String),
    /// The paths staged for the next commit.
    Staged,
}

#[derive(Clone, Debug, Default, clap::Args)]
pub struct Scope {
    /// Classify the commits since `HEAD` diverged from this reference.
    #[arg(long, conflicts_with_all = ["staged", "gates"])]
    base: Option<String>,
    /// Classify the staged paths.
    #[arg(long, conflicts_with = "gates")]
    staged: bool,
    /// Run exactly these comma-separated gates, as `classify` printed them.
    #[arg(long, value_parser = Gates::parse)]
    gates: Option<Gates>,
}

impl Scope {
    /// The gates to run: every gate unless the scope names a change set or gates.
    pub fn resolve(&self, root: &Path) -> Result<Gates> {
        if let Some(gates) = self.gates {
            return Ok(gates);
        }
        let source = match (&self.base, self.staged) {
            (Some(base), _) => Source::Base(base.clone()),
            (None, true) => Source::Staged,
            (None, false) => return Ok(Gates::ALL),
        };
        gates(root, &source)
    }
}

/// Paths changed in the source; a renamed path lists both names, so each is classified.
pub fn paths(root: &Path, source: &Source) -> Result<Vec<String>> {
    let mut command = Tool::Git.command();
    command
        .current_dir(root)
        .args(["diff", "--name-only", "--no-renames", "-z"]);
    match source {
        Source::Base(base) => {
            command.arg(format!("{base}...HEAD"));
        }
        Source::Staged => {
            command.arg("--cached");
        }
    }
    let output = command.output().context("start git")?;
    ensure!(
        output.status.success(),
        "git cannot list the change set: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect())
}

pub fn gates(root: &Path, source: &Source) -> Result<Gates> {
    Ok(select(paths(root, source)?.iter().map(String::as_str)))
}

/// Prints `gates=<names>`, the form a CI step appends to its output file.
pub fn run(root: &Path, scope: &Scope) -> Result<()> {
    println!("gates={}", scope.resolve(root)?.names());
    Ok(())
}
