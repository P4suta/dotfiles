//! Refuses to run an installed tool whose sources changed after the build.
//!
//! Each tool embeds the revision and content hash of the files that build it, as `build.rs` shows.
//! A run compares that hash with the same files in the checkout it can find and refuses with the install task when they differ.

use crate::build_inputs::content_hash;
use crate::refusal::Refusal;
use crate::stale_rules::{Origin, freshness, origin, reinstalls, runs};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub const BUILD_IDENTITY_ARGUMENT: &str = "--dotfiles-build-identity";
const INSTALLATION_SCHEMA: u32 = 1;
const HASH_DIGITS: usize = 64;
const RECORD_DIRECTORY: &str = ".local/state/dotfiles/installed";

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

    #[must_use]
    pub fn installed_binary(self, home: &Path) -> PathBuf {
        home.join(".local/bin")
            .join(format!("{}{}", self.binary(), std::env::consts::EXE_SUFFIX))
    }
}

/// The sources that produced a tool.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Embedded {
    pub revision: String,
    pub root: PathBuf,
    pub hash: String,
}

/// Emits only compiled metadata for the exact reserved request, before normal command guards.
///
/// # Errors
///
/// Returns an error when metadata serialization or output writes fail.
pub fn print_build_identity() -> Result<bool> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(BUILD_IDENTITY_ARGUMENT))
        || arguments.next().is_some()
    {
        return Ok(false);
    }
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, &embedded())?;
    output.write_all(b"\n")?;
    Ok(true)
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
    home.join(RECORD_DIRECTORY).join(built.binary())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Installation {
    schema: u32,
    build: Embedded,
}

fn origin_refusal(built: Built, cause: impl Into<String>, evidence: &Path) -> Refusal {
    Refusal::new(
        "tool.installation-origin",
        cause,
        format!(
            "run just {} explicitly in the intended source checkout",
            built.install_task()
        ),
    )
    .evidence(format!("installation evidence: {}", evidence.display()))
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == HASH_DIGITS && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn bound_build(root: &Path, mut stamp: Embedded, built: Built) -> Result<Embedded> {
    if !stamp.root.is_absolute() {
        return Err(origin_refusal(
            built,
            "artifact metadata must name an absolute producing checkout",
            &stamp.root,
        )
        .into());
    }
    let requested = crate::paths::canonical(root)?;
    ensure!(
        is_checkout(&requested),
        "installation source is not a dotfiles checkout"
    );
    let producing = crate::paths::canonical(&stamp.root).map_err(|_| {
        origin_refusal(
            built,
            "the artifact's producing checkout is unavailable",
            &stamp.root,
        )
    })?;
    if producing != requested || !valid_hash(&stamp.hash) || content_hash(&requested)? != stamp.hash
    {
        return Err(origin_refusal(
            built,
            "the artifact was not built from the requested checkout's current sources",
            &producing,
        )
        .into());
    }
    stamp.root = producing;
    Ok(stamp)
}

/// Reads a built artifact's compiled origin and refuses a different source or content before deployment.
///
/// # Errors
///
/// Refuses unsuccessful, malformed, moved, foreign, or stale artifact metadata.
pub fn verify_artifact(root: &Path, binary: &Path, built: Built) -> Result<Embedded> {
    let output = crate::tool::external(binary)
        .arg(BUILD_IDENTITY_ARGUMENT)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("read {} build identity", built.binary()))?;
    if !output.status.success() {
        return Err(origin_refusal(
            built,
            "the artifact did not return its compiled build identity",
            binary,
        )
        .into());
    }
    let stamp = serde_json::from_slice(&output.stdout).map_err(|_| {
        origin_refusal(
            built,
            "the artifact returned malformed build identity",
            binary,
        )
    })?;
    bound_build(root, stamp, built)
}

fn write_installation(home: &Path, built: Built, stamp: Embedded) -> Result<()> {
    let path = record_path(home, built);
    let directory = path.parent().context("record directory")?;
    fs::create_dir_all(directory)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(
        &mut file,
        &Installation {
            schema: INSTALLATION_SCHEMA,
            build: stamp,
        },
    )?;
    file.write_all(b"\n")?;
    file.persist(&path)
        .with_context(|| format!("record the {} installation", built.binary()))?;
    Ok(())
}

/// Records the actual installed artifact's canonical producing checkout and content identity.
///
/// # Errors
///
/// Refuses an unavailable, foreign, or stale artifact and reports record write failures.
pub fn record_installation(home: &Path, root: &Path, built: Built) -> Result<()> {
    let stamp = verify_artifact(root, &built.installed_binary(home), built)?;
    write_installation(home, built, stamp)
}

fn installation(home: &Path, built: Built) -> Result<Option<Installation>> {
    let path = record_path(home, built);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("read installation origin"),
    };
    let record: Installation = serde_json::from_slice(&bytes).map_err(|_| {
        origin_refusal(
            built,
            "legacy or malformed installation records have no verified source origin",
            &path,
        )
    })?;
    if record.schema != INSTALLATION_SCHEMA
        || !record.build.root.is_absolute()
        || !valid_hash(&record.build.hash)
    {
        return Err(origin_refusal(
            built,
            "the installation record has an unsupported or invalid origin",
            &path,
        )
        .into());
    }
    let actual = crate::paths::canonical(&record.build.root).map_err(|_| {
        origin_refusal(
            built,
            "the recorded installation checkout moved or is unavailable",
            &record.build.root,
        )
    })?;
    if actual != record.build.root || !is_checkout(&actual) {
        return Err(origin_refusal(
            built,
            "the recorded checkout no longer has its canonical identity",
            &record.build.root,
        )
        .into());
    }
    Ok(Some(record))
}

/// The recorded installations whose sources in `root` have changed since.
///
/// # Errors
///
/// Returns an error when it can't hash the checkout in `root`.
pub fn lagging(home: &Path, root: &Path) -> Result<Vec<Built>> {
    let root = crate::paths::canonical(root)?;
    let mut lagging = Vec::new();
    let mut current = None;
    for built in Built::ALL {
        if built == Built::SkillOps && !crate::incidents::enabled_at(home) {
            continue;
        }
        let record = installation(home, built)?;
        let same_source = record
            .as_ref()
            .is_some_and(|record| record.build.root == root);
        if same_source && current.is_none() {
            current = Some(content_hash(&root)?);
        }
        let matching = record
            .as_ref()
            .is_some_and(|record| current.as_deref() == Some(record.build.hash.as_str()));
        if reinstalls(record.is_some(), same_source, matching) {
            lagging.push(built);
        }
    }
    Ok(lagging)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_refresh_preserves_disabled_skill_records_and_refuses_active_legacy_origins()
    -> Result<()> {
        const LEGACY: &[u8] = b"46253b0b21a5c408ac46884fbe5fcd7dc1508ceba5bb990ec45bd67ccdefd4af\n";
        for (built, opted_in, rejected) in [
            (Built::SkillOps, false, false),
            (Built::SkillOps, true, true),
            (Built::Coderabbit, false, true),
            (Built::PrWorkflow, false, true),
        ] {
            let scope = tempfile::tempdir()?;
            let result: Result<()> = (|| {
                let home = scope.path().join("home");
                let record = record_path(&home, built);
                fs::create_dir_all(record.parent().context("record parent")?)?;
                fs::write(&record, LEGACY)?;
                let history = home.join(".local/state/skill-ops/history.json");
                fs::create_dir_all(history.parent().context("history parent")?)?;
                fs::write(&history, b"preserved history\n")?;
                if opted_in {
                    let consent = home.join(crate::incidents::EXPERIMENTAL);
                    fs::create_dir_all(consent.parent().context("consent parent")?)?;
                    fs::write(consent, rules::incident_rules::EXPERIMENTAL_OPT_IN)?;
                }
                let actual = lagging(&home, scope.path());
                if rejected {
                    let error = actual.expect_err("active legacy origins must be refused");
                    let refusal = error.downcast_ref::<Refusal>().unwrap();
                    assert_eq!(refusal.rule, "tool.installation-origin");
                    assert_eq!(
                        refusal.cause,
                        "legacy or malformed installation records have no verified source origin"
                    );
                } else {
                    assert_eq!(actual?, Vec::<Built>::new());
                    reinstall_lagging(&home, scope.path())?;
                }
                assert_eq!(fs::read(&record)?, LEGACY);
                assert_eq!(fs::read(&history)?, b"preserved history\n");
                Ok(())
            })();
            crate::cleanup::report(scope.close(), "remove the owned freshness consent fixture");
            result?;
        }
        Ok(())
    }

    #[test]
    fn relative_artifact_origins_are_refused_before_any_caller_directory_is_resolved() {
        let stamp = Embedded {
            revision: "fixture".into(),
            root: PathBuf::from("."),
            hash: "0".repeat(HASH_DIGITS),
        };
        let error =
            bound_build(Path::new("unreadable-request"), stamp, Built::SkillOps).unwrap_err();
        let refusal = error.downcast_ref::<Refusal>().unwrap();
        assert_eq!(refusal.rule, "tool.installation-origin");
        assert_eq!(
            refusal.cause,
            "artifact metadata must name an absolute producing checkout"
        );
    }
}
