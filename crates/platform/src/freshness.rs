//! Refuses to run an installed tool whose sources changed after the build.
//!
//! Each tool embeds the revision and content hash of the files that build it, as `build.rs` shows.
//! A run compares that hash with the same files in the checkout it can find and refuses with the install task when they differ.

use crate::build_inputs::content_hash;
use crate::refusal::Refusal;
use crate::stale_rules::{Origin, freshness, origin, reinstalls, runs};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Path, PathBuf};

/// Names the dotfiles checkout to compare with.
/// Runs outside any checkout and fixtures use it.
pub const SOURCE_VARIABLE: &str = "DOTFILES_SOURCE";

/// An executable that an `install-*` recipe builds and copies into the user's home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Built {
    Coderabbit,
    PrWorkflow,
    SkillOps,
}

impl Built {
    pub const ALL: [Self; 3] = [Self::Coderabbit, Self::PrWorkflow, Self::SkillOps];

    #[must_use]
    pub const fn binary(self) -> &'static str {
        match self {
            Self::Coderabbit => "coderabbit",
            Self::PrWorkflow => "pr-workflow",
            Self::SkillOps => "skill-ops",
        }
    }

    /// The just recipe that rebuilds and installs it from the checkout it runs in.
    #[must_use]
    pub const fn install_task(self) -> &'static str {
        match self {
            Self::Coderabbit => "install-review-guard",
            Self::PrWorkflow => "install-pr-workflow",
            Self::SkillOps => "install-skill-ops",
        }
    }
}

/// The sources that produced a tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Embedded {
    pub revision: String,
    pub root: PathBuf,
    pub hash: String,
}

/// The build facts compiled into this executable.
#[must_use]
pub fn embedded() -> Embedded {
    Embedded {
        revision: env!("DOTFILES_BUILD_REVISION").into(),
        root: env!("DOTFILES_BUILD_ROOT").into(),
        hash: env!("DOTFILES_BUILD_HASH").into(),
    }
}

/// Whether `path` holds the platform crate that every tool builds from, which makes it a dotfiles checkout.
#[must_use]
pub fn is_checkout(path: &Path) -> bool {
    fs::read_to_string(path.join("crates/platform/Cargo.toml"))
        .is_ok_and(|manifest| manifest.contains("name = \"platform\""))
}

/// The dotfiles checkout that contains `start`, if any.
pub fn working_copy(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|directory| is_checkout(directory))
        .map(Path::to_path_buf)
}

/// The checkout to compare with and the reason for the choice.
pub fn locate(
    explicit: Option<&Path>,
    start: Option<&Path>,
    built_in: &Path,
) -> Option<(Origin, PathBuf)> {
    let explicit = explicit.filter(|path| is_checkout(path));
    let working = start.and_then(working_copy);
    let built = is_checkout(built_in).then_some(built_in);
    match origin(explicit.is_some(), working.is_some(), built.is_some())? {
        Origin::Explicit => explicit.map(|path| (Origin::Explicit, path.to_path_buf())),
        Origin::WorkingCopy => working.map(|path| (Origin::WorkingCopy, path)),
        Origin::BuildCheckout => built.map(|path| (Origin::BuildCheckout, path.to_path_buf())),
    }
}

fn describe(origin: Origin) -> &'static str {
    match origin {
        Origin::Explicit => "the checkout named by DOTFILES_SOURCE",
        Origin::WorkingCopy => "the checkout this command runs in",
        Origin::BuildCheckout => "the checkout the tool was built in",
    }
}

/// The hash digits a refusal shows, as many as a short Git revision.
const SHORT_HASH: usize = 12;

fn short(hash: &str) -> String {
    hash.chars().take(SHORT_HASH).collect()
}

fn stale(built: Built, stamp: &Embedded, origin: Origin, root: &Path) -> Refusal {
    let current = content_hash(root).map_or_else(|_| "unreadable".into(), |hash| short(&hash));
    Refusal::new(
        "tool.stale",
        format!(
            "the installed {} was built from sources that differ from {}; rebuild it before running",
            built.binary(),
            describe(origin)
        ),
        format!("just {}", built.install_task()),
    )
    .evidence(format!(
        "built from revision {} with content hash {}",
        stamp.revision,
        short(&stamp.hash)
    ))
    .evidence(format!(
        "checkout {} has content hash {current}",
        root.display()
    ))
    .evidence(format!(
        "run the install task inside {}, which installs from that checkout",
        root.display()
    ))
}

/// Refuses when `stamp` lags the checkout.
/// The search tries the explicit path, the working directory, and the original build location.
///
/// # Errors
///
/// Returns a `tool.stale` refusal when the located checkout's content hash differs from `stamp`, or an error when it can't hash that checkout.
pub fn verify(
    built: Built,
    stamp: &Embedded,
    explicit: Option<&Path>,
    start: Option<&Path>,
) -> Result<()> {
    let located = locate(explicit, start, &stamp.root);
    let matching = match &located {
        Some((_, root)) => content_hash(root)? == stamp.hash,
        None => false,
    };
    if runs(freshness(located.is_some(), matching)) {
        return Ok(());
    }
    let (origin, root) = located.context("a stale build always has a checkout")?;
    Err(stale(built, stamp, origin, &root).into())
}

/// The check every installed tool makes before it does anything else.
///
/// # Errors
///
/// Returns a `tool.stale` refusal when the tool's build lags the located checkout, or an error when it can't hash that checkout.
pub fn require_current(built: Built) -> Result<()> {
    let explicit = std::env::var_os(SOURCE_VARIABLE).map(PathBuf::from);
    let start = std::env::current_dir().ok();
    verify(built, &embedded(), explicit.as_deref(), start.as_deref())
}

fn record_path(home: &Path, built: Built) -> PathBuf {
    home.join(".local/state/dotfiles/installed")
        .join(built.binary())
}

/// Records the content hash of the sources that produced the installed `built`.
/// A later apply or merge compares that hash with the checkout.
///
/// # Errors
///
/// Returns an error when it can't hash the checkout or write the record.
pub fn record_installation(home: &Path, root: &Path, built: Built) -> Result<()> {
    let path = record_path(home, built);
    fs::create_dir_all(path.parent().context("record directory")?)?;
    fs::write(&path, content_hash(root)?)
        .with_context(|| format!("record the {} installation", built.binary()))
}

/// The recorded installations whose sources in `root` have changed since.
///
/// # Errors
///
/// Returns an error when it can't hash the checkout in `root`.
pub fn lagging(home: &Path, root: &Path) -> Result<Vec<Built>> {
    let current = content_hash(root)?;
    Ok(Built::ALL
        .into_iter()
        .filter(|built| {
            let recorded = fs::read_to_string(record_path(home, *built)).ok();
            reinstalls(
                recorded.is_some(),
                recorded.is_some_and(|hash| hash.trim() == current),
            )
        })
        .collect())
}

/// Rebuilds each recorded installation whose sources in `root` changed, as the install task does by hand.
///
/// # Errors
///
/// Returns an error when it can't hash the checkout, or an install task fails or can't start.
pub fn reinstall_lagging(home: &Path, root: &Path) -> Result<()> {
    for built in lagging(home, root)? {
        println!(
            "Reinstalling {}: its sources changed since it was installed",
            built.binary()
        );
        let status = Tool::Just
            .command()
            .current_dir(root)
            .arg(built.install_task())
            .status()
            .context("start just")?;
        ensure!(
            status.success(),
            "just {} failed with {status}",
            built.install_task()
        );
    }
    Ok(())
}
