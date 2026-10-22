//! The machine-level verification gate: finds the code shapes of the installed catalog in a repository, and decides each enforcement point from what owns them.
//!
//! The installed catalog and repository owner maps live under `~/.config/assure/`.
//! An exemption sits in the comment block before the site it exempts.

use crate::refusal::Refusal;
use crate::reply_language_rules::{self, Language};
use crate::shape_rules::{
    self, Bound, Cause, Change, Exemption, MapSource, Owner, Point, Signal, Site, Standing, Verdict,
};
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The catalog every host receives, relative to the home directory.
pub const CATALOG: &str = ".config/assure/shapes.toml";

/// The tracked repository file read only during migration to the owner's configuration.
pub const OWNERS: &str = "assure.toml";
const REPOS: &str = "repos";
const INITIAL_CATALOG_VERSION: u32 = 1;
const COMPARISON_NAME: &str = "Assure";
const COMPARISON_EMAIL: &str = "assure@localhost";
const COMPARISON_DATE: &str = "2000-01-01T00:00:00+00:00";
const COMPARISON_MESSAGE: &str = "chore: compare existing trees";

fn initial_catalog_version() -> u32 {
    INITIAL_CATALOG_VERSION
}

/// The directory beside a cargo-fuzz `fuzz_targets` directory that holds each target's corpus.
const CORPUS: &str = "corpus";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(default = "initial_catalog_version")]
    pub version: u32,
    #[serde(skip)]
    pub sha256: String,
    /// Path segments whose files verify code rather than ship it, which no signal reads.
    pub exclude: Vec<String>,
    #[serde(rename = "shape")]
    pub shapes: Vec<Shape>,
    pub anti_patterns: Option<AntiPatterns>,
    /// The directory that holds the catalog, which relative paths in it start from.
    #[serde(skip)]
    pub directory: PathBuf,
}

/// The structural catalog of generic anti-patterns that ast-grep applies, and the file extensions its rules read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AntiPatterns {
    pub config: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub id: String,
    /// What the shape must have, as a refusal states it.
    pub requirement: String,
    /// What to do about a gap of this shape.
    pub next: String,
    /// The text an owning file carries.
    pub marker: String,
    /// The marker in the site's own file owns it.
    #[serde(default)]
    pub in_file: bool,
    /// The owning file names a cargo-fuzz target, which needs a non-empty corpus.
    #[serde(default)]
    pub corpus: bool,
    pub signals: Vec<SignalSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalSpec {
    pub suffix: String,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owners {
    #[serde(default, rename = "owner")]
    pub owners: Vec<OwnerSpec>,
    #[serde(default, rename = "budget")]
    pub budgets: Vec<BudgetSpec>,
}

/// The file that owns every site of one shape in one file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSpec {
    pub shape: String,
    pub path: String,
    pub by: String,
}

/// A deterministic budget that a change may never weaken.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetSpec {
    pub name: String,
    pub ceiling: Option<u64>,
    pub floor: Option<u64>,
}

impl BudgetSpec {
    fn bound(&self) -> Result<(Bound, u64)> {
        match (self.ceiling, self.floor) {
            (Some(value), None) => Ok((Bound::Ceiling, value)),
            (None, Some(value)) => Ok((Bound::Floor, value)),
            _ => bail!(
                "owner map: budget `{}` needs exactly one of `ceiling` and `floor`",
                self.name
            ),
        }
    }
}

/// Reads a catalog file.
///
/// # Errors
///
/// Returns an error when reading the file fails, or when it holds an unknown key, a shape without a signal, or two shapes with one id.
pub fn catalog(path: &Path) -> Result<Catalog> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let mut catalog: Catalog = toml::from_str(&text).with_context(|| path.display().to_string())?;
    ensure!(
        catalog.version > 0,
        "{}: catalog version must be positive",
        path.display()
    );
    catalog.sha256 = digest(text.as_bytes());
    catalog.directory = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut ids = BTreeSet::new();
    for shape in &catalog.shapes {
        ensure!(
            ids.insert(shape.id.as_str()),
            "{}: two shapes are named `{}`",
            path.display(),
            shape.id
        );
        ensure!(
            !shape.signals.is_empty(),
            "{}: shape `{}` has no signal",
            path.display(),
            shape.id
        );
    }
    Ok(catalog)
}

/// The catalog this host installed.
///
/// # Errors
///
/// Returns a refusal naming `chezmoi apply` when the host lacks the catalog, and an error when it doesn't parse.
pub fn installed() -> Result<Catalog> {
    let home = platform::home::home().context("native home is unavailable")?;
    let path = home.join(CATALOG);
    if !path.is_file() {
        return Err(Refusal::new(
            "assure.catalog",
            "the owner's local shape catalog is not installed",
            "chezmoi apply",
        )
        .evidence(format!("missing {}", path.display()))
        .evidence("owner's local catalog version=unavailable sha256=unavailable; base=not surveyed")
        .into());
    }
    catalog(&path)
}

fn digest(bytes: &[u8]) -> String {
    platform::build_inputs::hex(&Sha256::digest(bytes))
}

/// The owner-side map for the checkout's GitHub origin, or none for an unsupported origin.
///
/// # Errors
/// Returns an error when Git can't read the checkout's origin.
pub fn owner_path(root: &Path, catalog: &Catalog) -> Result<Option<PathBuf>> {
    let output = Tool::Git
        .command()
        .arg("-C")
        .arg(root)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .context("read checkout origin")?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    ensure!(output.status.success(), "cannot read checkout origin");
    let origin = std::str::from_utf8(&output.stdout)?.trim();
    Ok(
        platform::source_identity::github_slug(origin).map(|repository| {
            let key = repository.to_ascii_lowercase().replace('/', "__");
            catalog.directory.join(REPOS).join(format!("{key}.toml"))
        }),
    )
}

#[derive(Debug)]
enum OwnerMap {
    Local { path: PathBuf, bytes: Vec<u8> },
    Tracked { destination: Option<PathBuf> },
    Empty,
}

impl OwnerMap {
    fn configuration(&self, root: &Path, tree: &Tree) -> Result<(Owners, Survey)> {
        let (bytes, source) = match self {
            Self::Local { path, bytes } => (Some(bytes.clone()), path.display().to_string()),
            Self::Tracked { .. } => (
                contents(root, tree, &[OWNERS.to_owned()])?.remove(OWNERS),
                format!("tracked {OWNERS} at {tree:?}"),
            ),
            Self::Empty => (None, "no owner map".to_owned()),
        };
        let owners: Owners = match &bytes {
            Some(bytes) => {
                toml::from_str(std::str::from_utf8(bytes).context("owner map encoding")?)
                    .with_context(|| format!("{source} doesn't parse"))?
            }
            None => Owners::default(),
        };
        let mut survey = Survey {
            owner_input: format!(
                "owner map: {source}; sha256={}",
                bytes
                    .as_ref()
                    .map_or_else(|| "absent".to_owned(), |bytes| digest(bytes))
            ),
            ..Survey::default()
        };
        if matches!(self, Self::Tracked { .. }) {
            survey.owning.insert(OWNERS.to_owned());
        }
        for budget in &owners.budgets {
            survey.budgets.insert(budget.name.clone(), budget.bound()?);
        }
        Ok((owners, survey))
    }

    fn read(root: &Path, catalog: &Catalog, trees: &[&Tree]) -> Result<Self> {
        let destination = owner_path(root, catalog)?;
        let local_exists = destination
            .as_ref()
            .map(|path| path.try_exists())
            .transpose()?
            .unwrap_or(false);
        let mut tracked = if local_exists {
            false
        } else {
            split_nul(&git(root, &["ls-files", "-z", "--cached", "--", OWNERS])?)
                .iter()
                .any(|path| path == OWNERS)
        };
        if !local_exists && !tracked {
            for tree in trees {
                if let Tree::Commit(Some(commit)) = tree {
                    tracked = split_nul(&git(
                        root,
                        &["ls-tree", "-z", "--name-only", commit, "--", OWNERS],
                    )?)
                    .iter()
                    .any(|path| path == OWNERS);
                    if tracked {
                        break;
                    }
                }
            }
        }
        Ok(match shape_rules::map_source(local_exists, tracked) {
            MapSource::Owner => {
                let path = destination.context("owner map path is unavailable")?;
                let bytes = std::fs::read(&path).with_context(|| path.display().to_string())?;
                Self::Local { path, bytes }
            }
            MapSource::Tracked => Self::Tracked { destination },
            MapSource::Empty => Self::Empty,
        })
    }

    fn migration(&self, root: &Path) -> Option<String> {
        let Self::Tracked {
            destination: Some(destination),
        } = self
        else {
            return None;
        };
        let directory = destination.parent()?;
        let source = root.join(OWNERS);
        let (directory, source, destination, root) = (
            directory.to_string_lossy(),
            source.to_string_lossy(),
            destination.to_string_lossy(),
            root.to_string_lossy(),
        );
        #[cfg(not(windows))]
        {
            use crate::refusal::command;
            Some(format!(
                "{} && {} && {}",
                command(&["mkdir", "-p", &directory]),
                command(&["cp", &source, &destination]),
                command(&["git", "-C", &root, "rm", "--", OWNERS]),
            ))
        }
        #[cfg(windows)]
        {
            let quote = |value: &str| format!("'{}'", value.replace('\'', "''"));
            Some(format!(
                "New-Item -ItemType Directory -Force -Path {} && Copy-Item -LiteralPath {} -Destination {} && git -C {} rm -- {OWNERS}",
                quote(&directory),
                quote(&source),
                quote(&destination),
                quote(&root),
            ))
        }
    }
}

/// One state of a repository that the gate reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Tree {
    /// A commit, or with `None` the empty state before a repository's first commit.
    Commit(Option<String>),
    Index,
    WorkingTree,
}

fn git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Tool::Git
        .command()
        .arg("-C")
        .arg(root)
        .args(arguments)
        .stderr(Stdio::piped())
        .output()
        .context("start Git")?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn split_nul(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .filter_map(|path| std::str::from_utf8(path).ok())
        .filter(|path| !path.contains('\n'))
        .map(str::to_owned)
        .collect()
}

/// The commit `name` resolves to, or `None` when it names none.
///
/// # Errors
///
/// Returns an error when Git can't start.
pub fn resolve(root: &Path, name: &str) -> Result<Option<String>> {
    let output = Tool::Git
        .command()
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{name}^{{commit}}"))
        .stderr(Stdio::null())
        .output()
        .context("start Git")?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned()))
}

/// Every path in `tree`.
fn paths(root: &Path, tree: &Tree) -> Result<Vec<String>> {
    Ok(match tree {
        Tree::Commit(None) => Vec::new(),
        Tree::Commit(Some(commit)) => split_nul(&git(
            root,
            &["ls-tree", "-r", "-z", "--full-tree", "--name-only", commit],
        )?),
        Tree::Index => split_nul(&git(root, &["ls-files", "-z", "--cached"])?),
        Tree::WorkingTree => split_nul(&git(
            root,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?)
        .into_iter()
        .filter(|path| root.join(path).is_file())
        .collect(),
    })
}

/// The contents of `wanted` in `tree`, leaving out a path the tree lacks.
fn contents(root: &Path, tree: &Tree, wanted: &[String]) -> Result<BTreeMap<String, Vec<u8>>> {
    let prefix = match tree {
        Tree::Commit(None) => return Ok(BTreeMap::new()),
        Tree::WorkingTree => {
            return Ok(wanted
                .iter()
                .filter_map(|path| {
                    std::fs::read(root.join(path))
                        .ok()
                        .map(|bytes| (path.clone(), bytes))
                })
                .collect());
        }
        Tree::Commit(Some(commit)) => format!("{commit}:"),
        Tree::Index => ":".to_owned(),
    };
    if wanted.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut child = Tool::Git
        .command()
        .arg("-C")
        .arg(root)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("start Git")?;
    let mut input = child.stdin.take().context("Git input is unavailable")?;
    let mut requests = String::new();
    for path in wanted {
        requests.push_str(&prefix);
        requests.push_str(path);
        requests.push('\n');
    }
    let writer = std::thread::spawn(move || input.write_all(requests.as_bytes()));
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .context("Git output is unavailable")?
        .read_to_end(&mut output)?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("the Git request writer panicked"))??;
    ensure!(child.wait()?.success(), "git cat-file --batch failed");
    let mut found = BTreeMap::new();
    let mut rest = output.as_slice();
    for path in wanted {
        let end = rest
            .iter()
            .position(|byte| *byte == b'\n')
            .context("git cat-file ended early")?;
        let (header, tail) = rest.split_at(end);
        let tail = tail.get(1..).unwrap_or_default();
        let header = String::from_utf8_lossy(header);
        let mut fields = header.split(' ');
        let (_, kind, size) = (fields.next(), fields.next(), fields.next());
        match (kind, size.and_then(|size| size.parse::<usize>().ok())) {
            (Some("missing" | "ambiguous"), _) => rest = tail,
            (_, None) => bail!("git cat-file returned a malformed header: {header}"),
            (Some(kind), Some(size)) => {
                let (body, after) = tail
                    .split_at_checked(size)
                    .context("git cat-file returned a short object")?;
                if kind == "blob" {
                    found.insert(path.clone(), body.to_vec());
                }
                rest = after.get(1..).unwrap_or_default();
            }
            (None, _) => bail!("git cat-file returned an empty header"),
        }
    }
    Ok(found)
}

/// What makes a site a gap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    /// A shape that nothing owns or exempts.
    Shape(Cause),
    /// Code that a rule of the anti-pattern catalog refuses, with the rule's message and corrective note.
    AntiPattern { message: String, note: String },
}

/// One gap: a site that nothing owns and nothing exempts, or code the anti-pattern catalog refuses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gap {
    /// The shape, or the anti-pattern rule that refuses the code.
    pub shape: String,
    pub path: String,
    /// The line of the site, counting from one.
    pub line: usize,
    pub kind: Kind,
    /// What identifies the gap across revisions: the shape, the path, the site's text, and its occurrence in the file.
    key: Vec<u8>,
}

/// What the gate read from one state of a repository.
#[derive(Debug, Default)]
pub struct Survey {
    pub gaps: Vec<Gap>,
    budgets: BTreeMap<String, (Bound, u64)>,
    /// The owners file and every file and corpus an owner names, which decide sites outside their own file.
    owning: BTreeSet<String>,
    owner_input: String,
}

impl Survey {
    fn keys(&self) -> Vec<&[u8]> {
        self.gaps.iter().map(|gap| gap.key.as_slice()).collect()
    }
}

fn signals(shape: &Shape) -> impl Iterator<Item = Signal<'_>> {
    shape.signals.iter().map(|signal| Signal {
        suffix: signal.suffix.as_bytes(),
        prefix: signal.prefix.as_bytes(),
    })
}

fn excluded(catalog: &Catalog, path: &str) -> bool {
    catalog
        .exclude
        .iter()
        .any(|segment| shape_rules::within(path.as_bytes(), segment.as_bytes()))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// The corpus directory `fuzz/corpus/<name>/` of a cargo-fuzz target at `fuzz/fuzz_targets/<name>.rs`.
#[must_use]
pub fn corpus_of(target: &str) -> Option<String> {
    let path = Path::new(target);
    let name = path.file_stem()?.to_str()?;
    let fuzz = path.parent()?.parent()?;
    let directory = fuzz.join(CORPUS).join(name);
    Some(format!(
        "{}/",
        directory
            .to_str()?
            .replace('\\', "/")
            .trim_start_matches('/')
    ))
}

/// The exemption in the comment block that holds a site, read upward from the site, or downward from the top for a whole file.
fn exemption_at(lines: &[&[u8]], index: usize, whole: bool) -> Option<Exemption> {
    let block: Vec<&[u8]> = if whole {
        lines
            .iter()
            .copied()
            .take_while(|line| shape_rules::annotation(line))
            .collect()
    } else {
        lines
            .get(..index)
            .unwrap_or_default()
            .iter()
            .rev()
            .copied()
            .take_while(|line| shape_rules::annotation(line))
            .collect()
    };
    let mut found = None;
    for line in block {
        match shape_rules::exemption(line) {
            Some(Exemption::Malformed) => return Some(Exemption::Malformed),
            Some(exemption) if found.is_none() => found = Some(exemption),
            Some(_) => return Some(Exemption::Malformed),
            None => {}
        }
    }
    found
}

/// What owns the sites of `shape` in `path`, as the entries of `owners` declare and `holds` judges each owning file.
fn owner_of(owners: &Owners, shape: &Shape, path: &str, holds: impl Fn(&str) -> bool) -> Owner {
    owners
        .owners
        .iter()
        .filter(|owner| owner.shape == shape.id && owner.path == path)
        .fold(Owner::Undeclared, |standing, owner| {
            match (standing, holds(&owner.by)) {
                (Owner::Present, _) | (_, true) => Owner::Present,
                _ => Owner::Broken,
            }
        })
}

/// The paths of `all` in `scope` whose extension the anti-pattern catalog reads.
fn scanned(catalog: &Catalog, all: &[String], scope: Option<&BTreeSet<String>>) -> Vec<String> {
    let Some(anti) = &catalog.anti_patterns else {
        return Vec::new();
    };
    all.iter()
        .filter(|path| scope.is_none_or(|scope| scope.contains(*path)))
        .filter(|path| {
            path.rsplit_once('.').is_some_and(|(_, extension)| {
                anti.extensions.iter().any(|wanted| wanted == extension)
            })
        })
        .cloned()
        .collect()
}

/// Surveys `tree`, reading only `scope` when it names paths, and every shipped file otherwise.
///
/// # Errors
///
/// Returns an error when Git fails, or the owners file doesn't parse.
pub fn survey(
    root: &Path,
    catalog: &Catalog,
    tree: &Tree,
    scope: Option<&BTreeSet<String>>,
) -> Result<Survey> {
    survey_with(
        root,
        catalog,
        &OwnerMap::read(root, catalog, &[tree])?,
        tree,
        scope,
    )
}

fn survey_with(
    root: &Path,
    catalog: &Catalog,
    map: &OwnerMap,
    tree: &Tree,
    scope: Option<&BTreeSet<String>>,
) -> Result<Survey> {
    let all = paths(root, tree)?;
    let (owners, mut result) = map.configuration(root, tree)?;
    let candidates: Vec<String> = all
        .iter()
        .filter(|path| scope.is_none_or(|scope| scope.contains(*path)))
        .filter(|path| !excluded(catalog, path))
        .filter(|path| {
            catalog.shapes.iter().any(|shape| {
                shape
                    .signals
                    .iter()
                    .any(|signal| path.ends_with(signal.suffix.as_str()))
            })
        })
        .cloned()
        .collect();
    let scanned = scanned(catalog, &all, scope);
    let mut wanted = candidates.clone();
    wanted.extend(scanned.iter().cloned());
    for owner in &owners.owners {
        result.owning.insert(owner.by.clone());
        wanted.push(owner.by.clone());
        if let Some(corpus) = corpus_of(&owner.by) {
            result.owning.insert(corpus);
        }
    }
    let files = contents(root, tree, &wanted)?;
    let corpus_present = |target: &str| {
        corpus_of(target).is_some_and(|corpus| all.iter().any(|path| path.starts_with(&corpus)))
    };
    for path in &candidates {
        let Some(bytes) = files.get(path) else {
            continue;
        };
        let lines: Vec<&[u8]> = bytes
            .split(|byte| *byte == b'\n')
            .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
            .collect();
        let mut seen: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
        for shape in &catalog.shapes {
            let marker = contains(bytes, shape.marker.as_bytes());
            let owner = owner_of(&owners, shape, path, |by| {
                files
                    .get(by)
                    .is_some_and(|text| contains(text, shape.marker.as_bytes()))
                    && (!shape.corpus || corpus_present(by))
            });
            for (index, line) in lines.iter().enumerate() {
                let Some(signal) = signals(shape)
                    .find(|signal| shape_rules::found(*signal, path.as_bytes(), index == 0, line))
                else {
                    continue;
                };
                let whole = signal.prefix.is_empty();
                let site = Site {
                    marker,
                    in_file: shape.in_file,
                    owner,
                    exemption: exemption_at(&lines, index, whole),
                };
                if let Standing::Gap(cause) = shape_rules::standing(site) {
                    let text = if whole { &b""[..] } else { line.trim_ascii() };
                    let mut key = [shape.id.as_bytes(), path.as_bytes(), text].join(&0);
                    let occurrence = seen.entry(key.clone()).or_default();
                    *occurrence += 1;
                    key.extend_from_slice(format!("\0{occurrence}").as_bytes());
                    result.gaps.push(Gap {
                        shape: shape.id.clone(),
                        path: path.clone(),
                        line: index + 1,
                        kind: Kind::Shape(cause),
                        key,
                    });
                }
            }
        }
    }
    if let Some(anti) = &catalog.anti_patterns
        && !scanned.is_empty()
    {
        result.gaps.extend(anti_patterns(
            &catalog.directory.join(&anti.config),
            &scanned,
            &files,
        )?);
    }
    Ok(result)
}

/// One finding of `ast-grep scan --json=stream`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Finding {
    text: String,
    range: Range,
    file: String,
    rule_id: String,
    message: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct Range {
    start: Position,
}

#[derive(Deserialize)]
struct Position {
    line: usize,
}

/// The code of `scanned` that the anti-pattern catalog at `config` refuses, each finding as a gap.
/// The files go to a scratch directory under their own paths, so a revision or the index scans the same as the working tree.
fn anti_patterns(
    config: &Path,
    scanned: &[String],
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<Gap>> {
    if !config.is_file() {
        return Err(Refusal::new(
            "assure.catalog",
            "the owner's local anti-pattern catalog is not installed",
            "chezmoi apply",
        )
        .evidence(format!("missing {}", config.display()))
        .into());
    }
    let scratch = tempfile::tempdir()?;
    for path in scanned {
        if let Some(bytes) = files.get(path) {
            let target = scratch.path().join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, bytes)?;
        }
    }
    let output = Tool::AstGrep
        .command()
        .current_dir(scratch.path())
        .arg("scan")
        .arg("--config")
        .arg(config)
        .args(["--json=stream", "--no-ignore", "hidden", "."])
        .output()
        .map_err(|error| {
            anyhow::Error::from(
                Refusal::new(
                    "assure.ast-grep",
                    "ast-grep applies the anti-pattern catalog and could not start",
                    "chezmoi apply",
                )
                .evidence(error.to_string()),
            )
        })?;
    let stdout = String::from_utf8(output.stdout)?;
    ensure!(
        output.status.success() || !stdout.trim().is_empty(),
        "ast-grep could not apply {}: {}",
        config.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let mut seen: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut gaps = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let finding: Finding =
            serde_json::from_str(line).context("ast-grep wrote a finding that doesn't parse")?;
        let path = finding.file.replace('\\', "/");
        let path = path.trim_start_matches("./").to_owned();
        let mut key = [
            finding.rule_id.as_bytes(),
            path.as_bytes(),
            finding.text.trim().as_bytes(),
        ]
        .join(&0);
        let occurrence = seen.entry(key.clone()).or_default();
        *occurrence += 1;
        key.extend_from_slice(format!("\0{occurrence}").as_bytes());
        gaps.push(Gap {
            shape: finding.rule_id,
            path,
            line: finding.range.start.line + 1,
            kind: Kind::AntiPattern {
                message: finding.message,
                note: finding.note.unwrap_or_default(),
            },
            key,
        });
    }
    Ok(gaps)
}

/// The paths that differ between two states of a repository.
///
/// # Errors
///
/// Returns an error when Git fails.
pub fn changed(root: &Path, before: &Tree, after: &Tree) -> Result<BTreeSet<String>> {
    let empty;
    let base = match before {
        Tree::Commit(Some(commit)) => commit.as_str(),
        Tree::Commit(None) => {
            empty = empty_tree(root)?;
            empty.as_str()
        }
        Tree::Index | Tree::WorkingTree => bail!("a comparison starts from a commit"),
    };
    let listed = match after {
        Tree::Commit(None) => Vec::new(),
        Tree::Commit(Some(commit)) => split_nul(&git(
            root,
            &["diff", "--name-only", "-z", "--no-renames", base, commit],
        )?),
        Tree::Index => split_nul(&git(
            root,
            &[
                "diff",
                "--cached",
                "--name-only",
                "-z",
                "--no-renames",
                base,
            ],
        )?),
        Tree::WorkingTree => {
            let mut listed = split_nul(&git(
                root,
                &["diff", "--name-only", "-z", "--no-renames", base],
            )?);
            listed.extend(split_nul(&git(
                root,
                &["ls-files", "-z", "--others", "--exclude-standard"],
            )?));
            listed
        }
    };
    Ok(listed.into_iter().collect())
}

/// The empty tree of this repository's object format.
fn empty_tree(root: &Path) -> Result<String> {
    let mut child = Tool::Git
        .command()
        .arg("-C")
        .arg(root)
        .args(["hash-object", "-t", "tree", "--stdin"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .context("start Git")?;
    let mut output = String::new();
    child
        .stdout
        .take()
        .context("Git output is unavailable")?
        .read_to_string(&mut output)?;
    ensure!(child.wait()?.success(), "git hash-object failed");
    Ok(output.trim().to_owned())
}

/// The budgets of `before` that `after` weakens or drops.
fn lowered(before: &Survey, after: &Survey) -> Vec<String> {
    before
        .budgets
        .iter()
        .filter(|(name, (bound, value))| {
            let next = after
                .budgets
                .get(*name)
                .filter(|(next, _)| next == bound)
                .map(|(_, next)| *next);
            shape_rules::lowered(*bound, *value, next)
        })
        .map(|(name, (bound, value))| {
            let kind = match bound {
                Bound::Ceiling => "ceiling",
                Bound::Floor => "floor",
            };
            let now = after
                .budgets
                .get(name)
                .map_or_else(|| "removed".to_owned(), |(_, next)| next.to_string());
            format!("owner map: budget `{name}` {kind} {value} became {now}")
        })
        .collect()
}

/// What the gate concluded at one enforcement point.
#[derive(Debug)]
pub struct Outcome {
    pub inputs: Inputs,
    pub migration: Option<String>,
    pub verdict: Verdict,
    pub change: Change,
    /// The gaps behind the verdict: the added or touched ones when it refuses, and every gap otherwise.
    pub gaps: Vec<Gap>,
    pub lowered: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inputs {
    pub catalog_version: u32,
    pub catalog_sha256: String,
    pub base: Option<String>,
    pub inherited: BTreeSet<String>,
    pub owners: BTreeSet<String>,
}

impl Inputs {
    fn new(catalog: &Catalog, base: Option<String>) -> Self {
        Self {
            catalog_version: catalog.version,
            catalog_sha256: catalog.sha256.clone(),
            base,
            inherited: BTreeSet::new(),
            owners: BTreeSet::new(),
        }
    }

    fn description(&self) -> String {
        let mut description = format!(
            "owner's local catalog version={} sha256={}; base={}",
            self.catalog_version,
            self.catalog_sha256,
            self.base.as_deref().unwrap_or("empty tree"),
        );
        if !self.inherited.is_empty() {
            description.push_str("; inherited=");
            description.push_str(&self.inherited.iter().cloned().collect::<Vec<_>>().join(","));
        }
        description
    }

    fn refusal(&self, mut refusal: Refusal) -> Refusal {
        refusal = refusal.evidence(self.description());
        for owner in &self.owners {
            refusal = refusal.evidence(owner);
        }
        refusal
    }

    fn error(&self, error: anyhow::Error) -> anyhow::Error {
        match error.downcast::<Refusal>() {
            Ok(refusal) => self.refusal(refusal).into(),
            Err(error) => error.context(self.description()),
        }
    }
}

impl Outcome {
    /// The refusal of a refusing verdict, naming every gap with its next action.
    #[must_use]
    pub fn refusal(&self, catalog: &Catalog) -> Option<Refusal> {
        let (rule, cause, next) = match self.verdict {
            Verdict::Admit | Verdict::Report => return None,
            Verdict::Added => (
                "assure.added",
                "the owner's local catalog refuses a new unverified code shape or anti-pattern in this change",
                "apply each gap's next action, then retry",
            ),
            Verdict::Touched => (
                "assure.touched",
                "the owner's local catalog refuses an unverified code shape or anti-pattern in a file this turn changed",
                "apply each listed gap's next action before finishing",
            ),
            Verdict::Lowered => (
                "assure.lowered",
                "the owner's local catalog refuses a weakened verification budget",
                "restore each listed budget in the owner map and meet it",
            ),
        };
        let mut refusal = Refusal::new(rule, cause, next);
        for gap in &self.gaps {
            refusal = refusal.evidence(describe(catalog, gap));
        }
        for budget in &self.lowered {
            refusal = refusal.evidence(budget.clone());
        }
        Some(self.inputs.refusal(refusal))
    }
}

/// One gap with what its shape needs and its next action.
#[must_use]
pub fn describe(catalog: &Catalog, gap: &Gap) -> String {
    let cause = match &gap.kind {
        Kind::AntiPattern { message, note } => {
            return format!(
                "{}:{}: {} {message} next: {note}",
                gap.path, gap.line, gap.shape
            );
        }
        Kind::Shape(cause) => *cause,
    };
    let shape = catalog.shapes.iter().find(|shape| shape.id == gap.shape);
    let cause = match cause {
        Cause::Unowned => "has no owner",
        Cause::BrokenOwner => "names an owner in the owner's map that lacks its marker or corpus",
        Cause::MalformedExemption => {
            "carries an exemption without one of semantic-boundary, no-decision, or disproportionate and a justification"
        }
        Cause::StaleExemption => {
            "carries an exemption although an owner exists; delete the exemption"
        }
    };
    format!(
        "{}:{}: {} {cause}; it needs {}; next: {}",
        gap.path,
        gap.line,
        gap.shape,
        shape.map_or("an owner", |shape| shape.requirement.as_str()),
        shape.map_or("add an owner", |shape| shape.next.as_str()),
    )
}

/// The paths that decide the change from `before` to `after`: the changed paths, or every path once an owner changed.
fn scope(
    root: &Path,
    catalog: &Catalog,
    map: &OwnerMap,
    before: &Tree,
    after: &Tree,
    inherited: &[Tree],
) -> Result<(BTreeSet<String>, Option<BTreeSet<String>>)> {
    let changed = changed(root, before, after)?;
    let probe = |tree: &Tree| survey_with(root, catalog, map, tree, Some(&BTreeSet::new()));
    let mut owning: BTreeSet<String> = probe(before)?
        .owning
        .into_iter()
        .chain(probe(after)?.owning)
        .collect();
    for tree in inherited {
        owning.extend(probe(tree)?.owning);
    }
    let widened = changed.iter().any(|path| {
        owning
            .iter()
            .any(|owner| path == owner || (owner.ends_with('/') && path.starts_with(owner)))
    });
    let limit = (!widened).then(|| changed.clone());
    Ok((changed, limit))
}

/// The commit that the message of a `git revert` names, from its `This reverts commit <commit>.` line.
fn reverted_commit(message: &str) -> Option<&str> {
    message.lines().find_map(|line| {
        line.trim()
            .strip_prefix("This reverts commit ")?
            .split([' ', '.'])
            .next()
            .filter(|commit| {
                !commit.is_empty() && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    })
}

fn comparison_commit(root: &Path, base: &str, tree: &str) -> Result<String> {
    let output = Tool::Git
        .command()
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "commit.gpgSign=false",
            "commit-tree",
            tree,
            "-p",
            base,
            "-m",
            COMPARISON_MESSAGE,
        ])
        .env("GIT_AUTHOR_NAME", COMPARISON_NAME)
        .env("GIT_COMMITTER_NAME", COMPARISON_NAME)
        .env("GIT_AUTHOR_EMAIL", COMPARISON_EMAIL)
        .env("GIT_COMMITTER_EMAIL", COMPARISON_EMAIL)
        .env("GIT_AUTHOR_DATE", COMPARISON_DATE)
        .env("GIT_COMMITTER_DATE", COMPARISON_DATE)
        .output()
        .context("construct an unreferenced comparison object")?;
    ensure!(
        output.status.success(),
        "git commit-tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commit = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        shape_rules::commit_id(commit.as_bytes()),
        "Git returned an invalid comparison object"
    );
    Ok(commit)
}

/// The tree that reverting `reverted` onto `onto` yields, or `None` when that revert conflicts.
fn restored(root: &Path, reverted: &str, onto: &str) -> Result<Option<String>> {
    let Some(parent) = resolve(root, &format!("{reverted}^"))? else {
        return Ok(None);
    };
    let left = comparison_commit(root, reverted, &format!("{onto}^{{tree}}"))?;
    let right = comparison_commit(root, reverted, &format!("{parent}^{{tree}}"))?;
    let output = Tool::Git
        .command()
        .arg("-C")
        .arg(root)
        .args(["merge-tree", "--write-tree", "--no-messages"])
        .args([&left, &right])
        .output()
        .context("start Git")?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    ensure!(
        output.status.success(),
        "git merge-tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout)?
        .lines()
        .next()
        .context("Git returned no restored tree")?
        .to_owned();
    ensure!(
        shape_rules::commit_id(tree.as_bytes()),
        "Git returned an invalid restored tree"
    );
    Ok(Some(tree))
}

/// Whether the change from `before` to `after` only reverts earlier commits, each exactly as `git revert` writes it.
/// A commit reads the revert that Git is resolving, and a push or PR reads every commit it adds.
fn reverting(root: &Path, point: Point, before: &Tree, after: &Tree) -> Result<bool> {
    let tree = |commit: &str| {
        git(root, &["rev-parse", &format!("{commit}^{{tree}}")])
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
    };
    match (point, before, after) {
        (Point::Commit, Tree::Commit(Some(head)), Tree::Index) => {
            let Some(reverted) = resolve(root, "REVERT_HEAD")? else {
                return Ok(false);
            };
            let Ok(index) = git(root, &["write-tree"]) else {
                return Ok(false);
            };
            let index = String::from_utf8_lossy(&index).trim().to_owned();
            Ok(restored(root, &reverted, head)?.is_some_and(|tree| tree == index))
        }
        (Point::Push | Point::Ready, Tree::Commit(Some(base)), Tree::Commit(Some(tip))) => {
            let listed = git(
                root,
                &[
                    "rev-list",
                    "--reverse",
                    "--parents",
                    &format!("{base}..{tip}"),
                ],
            )?;
            let listed = String::from_utf8_lossy(&listed);
            let mut commits = listed.lines().peekable();
            if commits.peek().is_none() {
                return Ok(false);
            }
            for line in commits {
                let [commit, parent] = line.split_whitespace().collect::<Vec<_>>()[..] else {
                    return Ok(false);
                };
                let message = git(root, &["log", "-1", "--format=%B", commit])?;
                let message = String::from_utf8_lossy(&message);
                let Some(named) = reverted_commit(&message) else {
                    return Ok(false);
                };
                let Some(reverted) = resolve(root, named)? else {
                    return Ok(false);
                };
                if restored(root, &reverted, parent)? != Some(tree(commit)?) {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Decides `point` for the change from `before` to `after`.
///
/// # Errors
///
/// Returns an error when Git fails or an owners file doesn't parse.
pub fn decide(
    root: &Path,
    catalog: &Catalog,
    point: Point,
    before: &Tree,
    after: &Tree,
) -> Result<Outcome> {
    let inherited = if matches!(point, Point::Commit | Point::Stop)
        && matches!(after, Tree::Index | Tree::WorkingTree)
        && *before == Tree::Commit(resolve(root, "HEAD")?)
    {
        merge_parents(root)?
    } else {
        Vec::new()
    };
    decide_from(root, catalog, point, before, after, &inherited)
}

fn merge_parents(root: &Path) -> Result<Vec<String>> {
    let path = git(root, &["rev-parse", "--git-path", "MERGE_HEAD"])?;
    let path = root.join(String::from_utf8(path)?.trim());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| path.display().to_string()),
    };
    ensure!(!text.trim().is_empty(), "MERGE_HEAD has no parent commits");
    text.lines()
        .map(|line| {
            ensure!(
                shape_rules::commit_id(line.as_bytes()),
                "MERGE_HEAD has an invalid parent commit"
            );
            resolve(root, line)?.context("MERGE_HEAD parent commit is missing")
        })
        .collect()
}

fn decide_from(
    root: &Path,
    catalog: &Catalog,
    point: Point,
    before: &Tree,
    after: &Tree,
    inherited: &[String],
) -> Result<Outcome> {
    let base = match before {
        Tree::Commit(Some(commit)) => resolve(root, commit)?
            .context("comparison base is missing")
            .map(Some)?,
        Tree::Commit(None) => None,
        Tree::Index | Tree::WorkingTree => bail!("comparison base must be a commit"),
    };
    let mut inputs = Inputs::new(catalog, base.clone());
    inputs.inherited.extend(inherited.iter().cloned());
    let before = &Tree::Commit(base);
    let inherited: Vec<_> = inherited
        .iter()
        .map(|commit| Tree::Commit(Some(commit.clone())))
        .collect();
    let trees: Vec<_> = [before, after]
        .into_iter()
        .chain(inherited.iter())
        .collect();
    let map = OwnerMap::read(root, catalog, &trees).map_err(|error| inputs.error(error))?;
    decide_with(root, catalog, point, before, after, &map, &mut inputs)
        .map_err(|error| inputs.error(error))
}

fn decide_with(
    root: &Path,
    catalog: &Catalog,
    point: Point,
    before: &Tree,
    after: &Tree,
    map: &OwnerMap,
    inputs: &mut Inputs,
) -> Result<Outcome> {
    let full = matches!(point, Point::Ready | Point::Start);
    let inherited: Vec<_> = inputs
        .inherited
        .iter()
        .map(|commit| Tree::Commit(Some(commit.clone())))
        .collect();
    let (mut changed, limit) = if point == Point::Start {
        (BTreeSet::new(), None)
    } else {
        scope(root, catalog, map, before, after, &inherited)?
    };
    let limit = if full { None } else { limit };
    let old = if point == Point::Start {
        Survey::default()
    } else {
        survey_with(root, catalog, map, before, limit.as_ref())?
    };
    if !old.owner_input.is_empty() {
        inputs.owners.insert(old.owner_input.clone());
    }
    let new = survey_with(root, catalog, map, after, limit.as_ref())?;
    inputs.owners.insert(new.owner_input.clone());
    let mut known = Vec::new();
    for tree in &inherited {
        let survey = survey_with(root, catalog, map, tree, limit.as_ref())?;
        inputs.owners.insert(survey.owner_input.clone());
        known.push(survey);
        if point == Point::Stop {
            let paths: Vec<_> = changed.iter().cloned().collect();
            let inherited_files = contents(root, tree, &paths)?;
            let current_files = contents(root, after, &paths)?;
            changed.retain(|path| inherited_files.get(path) != current_files.get(path));
        }
    }
    let old_keys: Vec<_> = old
        .keys()
        .into_iter()
        .chain(known.iter().flat_map(Survey::keys))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let new_keys = new.keys();
    let lowered = lowered(&old, &new);
    let change = Change {
        added: if point == Point::Start {
            0
        } else {
            shape_rules::fresh(&old_keys, &new_keys)
        },
        closed: shape_rules::fresh(&new_keys, &old_keys),
        before: old_keys.len(),
        after: new.gaps.len(),
        touched: new
            .gaps
            .iter()
            .filter(|gap| changed.contains(&gap.path))
            .count(),
        lowered: lowered.len(),
        reverted: reverting(root, point, before, after)?,
    };
    let verdict = shape_rules::verdict(point, change);
    let gaps = match verdict {
        Verdict::Added => new
            .gaps
            .iter()
            .filter(|gap| !old_keys.contains(&gap.key.as_slice()))
            .cloned()
            .collect(),
        Verdict::Touched => new
            .gaps
            .iter()
            .filter(|gap| changed.contains(&gap.path))
            .cloned()
            .collect(),
        _ => new.gaps,
    };
    Ok(Outcome {
        inputs: inputs.clone(),
        migration: map.migration(root),
        verdict,
        change,
        gaps,
        lowered,
    })
}

/// The root of the repository that holds the current directory, or `None` outside one.
#[must_use]
pub fn repository() -> Option<PathBuf> {
    let output = Tool::Git
        .command()
        .args(["rev-parse", "--show-toplevel"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
}

fn enforce(catalog: &Catalog, outcome: &Outcome) -> Result<()> {
    report_inputs(outcome);
    match outcome.refusal(catalog) {
        Some(refusal) => Err(refusal.into()),
        None => Ok(()),
    }
}

fn report_inputs(outcome: &Outcome) {
    eprintln!(
        "assure: {:?}; {}",
        outcome.verdict,
        outcome.inputs.description()
    );
    for owner in &outcome.inputs.owners {
        eprintln!("assure: {owner}");
    }
    if let Some(command) = &outcome.migration {
        eprintln!("assure: migrate the tracked owner map to the owner's configuration: {command}");
    }
}

/// The pre-commit point: refuses staged changes that add a gap.
///
/// # Errors
///
/// Returns the refusal, or an error when Git fails.
pub fn commit(root: &Path, catalog: &Catalog) -> Result<()> {
    let head = Tree::Commit(resolve(root, "HEAD")?);
    enforce(
        catalog,
        &decide(root, catalog, Point::Commit, &head, &Tree::Index)?,
    )
}

/// The pre-push point: refuses a pushed commit that adds a gap to what the remote already holds.
/// A new branch counts from the merge base with the upstream default commit advertised by Git.
/// An empty remote counts from the empty tree after a successful advertisement of all refs.
///
/// # Errors
///
/// Returns the refusal, or an error when Git fails or a line of the input lacks its four fields.
pub fn push(root: &Path, catalog: &Catalog, remote: &str, input: &[u8]) -> Result<()> {
    for line in std::str::from_utf8(input)?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let [_, local, _, theirs] = fields[..] else {
            bail!("malformed pre-push input: {line}");
        };
        let zero = |oid: &str| oid.bytes().all(|byte| byte == b'0');
        if zero(local) {
            continue;
        }
        let base = if zero(theirs) {
            upstream_base(root, catalog, remote, local)?
        } else {
            Some(explicit_base(root, catalog, remote, theirs)?)
        };
        let merges = match base.as_deref() {
            Some(base) => git(
                root,
                &[
                    "rev-list",
                    "--merges",
                    "--max-count=1",
                    &format!("{base}..{local}"),
                ],
            )?,
            None => Vec::new(),
        };
        let inherited = if zero(theirs) || merges.is_empty() {
            Vec::new()
        } else {
            upstream_base(root, catalog, remote, local)?
                .into_iter()
                .collect()
        };
        enforce(
            catalog,
            &decide_from(
                root,
                catalog,
                Point::Push,
                &Tree::Commit(base),
                &Tree::Commit(Some(local.to_owned())),
                &inherited,
            )?,
        )?;
    }
    Ok(())
}

fn explicit_base(root: &Path, catalog: &Catalog, remote: &str, base: &str) -> Result<String> {
    let inputs = Inputs::new(catalog, Some(base.to_owned()));
    if !shape_rules::commit_id(base.as_bytes()) {
        return Err(inputs
            .refusal(Refusal::new(
                "assure.base",
                "the owner's local catalog requires an explicit base commit SHA",
                "supply the PR base SHA or the merge base with the upstream default branch",
            ))
            .into());
    }
    resolve(root, base)?.ok_or_else(|| {
        inputs
            .refusal(Refusal::new(
                "assure.base",
                "the owner's local catalog cannot compare against a base commit missing locally",
                crate::refusal::command(&["git", "fetch", remote, base]),
            ))
            .into()
    })
}

fn upstream_base(
    root: &Path,
    catalog: &Catalog,
    remote: &str,
    head: &str,
) -> Result<Option<String>> {
    let advertised = git(root, &["ls-remote", remote, "HEAD"])?;
    if advertised.is_empty() && git(root, &["ls-remote", remote])?.is_empty() {
        return Ok(None);
    }
    let advertised = std::str::from_utf8(&advertised)?;
    let Some(default) = advertised
        .strip_suffix("\tHEAD\n")
        .filter(|commit| shape_rules::commit_id(commit.as_bytes()))
    else {
        return Err(Refusal::new(
            "assure.base", "the owner's local catalog cannot compare a populated remote without an upstream default commit",
            crate::refusal::command(&["git", "ls-remote", remote]),
        ).evidence(format!("owner's local catalog version={} sha256={}; base=unavailable", catalog.version, catalog.sha256)).into());
    };
    let tip = explicit_base(root, catalog, remote, default)?;
    let base = git(root, &["merge-base", head, &tip])?;
    let base = std::str::from_utf8(&base)?.trim();
    explicit_base(root, catalog, remote, base).map(Some)
}

/// The agent stop point over the uncommitted changes.
///
/// # Errors
///
/// Returns an error when Git fails.
pub fn stop(root: &Path, catalog: &Catalog) -> Result<Outcome> {
    let head = Tree::Commit(resolve(root, "HEAD")?);
    decide(root, catalog, Point::Stop, &head, &Tree::WorkingTree)
}

/// The session start report: every gap of the working tree with its next action, or nothing when it has none.
///
/// # Errors
///
/// Returns an error when Git fails.
pub fn start(root: &Path, catalog: &Catalog) -> Result<Option<String>> {
    let outcome = decide(
        root,
        catalog,
        Point::Start,
        &Tree::Commit(resolve(root, "HEAD")?),
        &Tree::WorkingTree,
    )?;
    report_inputs(&outcome);
    if outcome.verdict != Verdict::Report {
        return Ok(None);
    }
    let mut report = format!(
        "The owner's local catalog finds {} verification gaps in this repository ({}). Publication refuses new gaps and PR readiness also refuses weakened budgets:\n",
        outcome.gaps.len(),
        outcome.inputs.description(),
    );
    for gap in &outcome.gaps {
        report.push_str("- ");
        report.push_str(&describe(catalog, gap));
        report.push('\n');
    }
    Ok(Some(report))
}

/// The `pr-workflow ready` point: refuses a PR that adds a gap or weakens a budget.
///
/// # Errors
///
/// Returns the refusal, or an error when Git fails or this checkout lacks the base or the head.
pub fn ready(root: &Path, catalog: &Catalog, base: &str, head: &str) -> Result<()> {
    let base = explicit_base(root, catalog, "origin", base)?;
    let Some(head) = resolve(root, head)? else {
        return Err(Inputs::new(catalog, Some(base))
            .refusal(
                Refusal::new(
                    "assure.head",
                    "the owner's local catalog cannot survey a PR head commit missing locally",
                    crate::refusal::command(&["git", "fetch", "origin"]),
                )
                .evidence(format!("missing {head}")),
            )
            .into());
    };
    enforce(
        catalog,
        &decide(
            root,
            catalog,
            Point::Ready,
            &Tree::Commit(Some(base)),
            &Tree::Commit(Some(head)),
        )?,
    )
}

/// The enforcement points that run as agent hooks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum AgentPoint {
    Start,
    Stop,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ReplyLanguage {
    #[value(name = "Japanese")]
    Japanese,
    #[value(name = "English")]
    English,
}

impl From<ReplyLanguage> for Language {
    fn from(language: ReplyLanguage) -> Self {
        match language {
            ReplyLanguage::Japanese => Self::Japanese,
            ReplyLanguage::English => Self::English,
        }
    }
}

/// The agent hook of `point`: the session start report on standard output, or the stop decision as the client's JSON.
///
/// # Errors
///
/// Returns an error when the hook input holds no JSON, the host lacks the catalog, or Git fails.
pub fn hook(point: AgentPoint, language: Option<Language>) -> Result<()> {
    match point {
        AgentPoint::Start => {
            let Some(root) = repository() else {
                return Ok(());
            };
            let catalog = installed()?;
            if let Some(report) = start(&root, &catalog)? {
                print!("{report}");
            }
            Ok(())
        }
        AgentPoint::Stop => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let input = if input.trim().is_empty() {
                StopInput::default()
            } else {
                serde_json::from_str::<StopInput>(&input)
                    .context("the stop hook input is invalid")?
            };
            let mut reasons = Vec::new();
            if let Some(language) = language
                && reply_language_rules::check(
                    language,
                    input.last_assistant_message.as_deref().unwrap_or_default(),
                ) == reply_language_rules::Verdict::Rewrite
            {
                reasons.push(
                    "The reply prose must be in Japanese. Rewrite your last reply to the user in Japanese; keep code, commands, and outward artifacts as they are.".to_owned(),
                );
            }
            if let Some(root) = repository() {
                let catalog = installed()?;
                let outcome = stop(&root, &catalog)?;
                report_inputs(&outcome);
                if let Some(refusal) = outcome.refusal(&catalog) {
                    reasons.push(refusal.text());
                }
            }
            let reason = (!reasons.is_empty()).then(|| reasons.join("\n\n"));
            println!(
                "{}",
                stop_response(reason.as_deref(), input.stop_hook_active)
            );
            Ok(())
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct StopInput {
    #[serde(default)]
    stop_hook_active: bool,
    last_assistant_message: Option<String>,
}

/// The client's stop hook answer: block once with the reason, and end the turn incomplete when the agent already continued once.
#[must_use]
pub fn stop_response(reason: Option<&str>, continued: bool) -> serde_json::Value {
    use crate::skill_rules::{Completion, completion};
    match completion(reason.is_none(), continued) {
        Completion::Allow => serde_json::json!({}),
        Completion::RequestMaintenance => {
            serde_json::json!({"decision": "block", "reason": reason})
        }
        Completion::EndIncomplete => serde_json::json!({
            "continue": false,
            "stopReason": reason,
            "systemMessage": "Stop requirements remain unmet; automatic continuation is bounded."
        }),
    }
}
