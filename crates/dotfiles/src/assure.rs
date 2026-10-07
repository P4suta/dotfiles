//! The machine-level verification gate: finds the code shapes of the installed catalog in a repository, and decides each enforcement point from what owns them.
//!
//! The catalog at `~/.config/assure/shapes.toml` holds the signals, and each repository names its owners and budgets in its own `assure.toml`.
//! An exemption sits in the comment block before the site it exempts.

use crate::refusal::Refusal;
use crate::shape_rules::{
    self, Bound, Cause, Change, Exemption, Owner, Point, Signal, Site, Standing, Verdict,
};
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The catalog every host receives, relative to the home directory.
pub const CATALOG: &str = ".config/assure/shapes.toml";

/// The file at a repository's root that names the owners of its shapes and its budgets.
pub const OWNERS: &str = "assure.toml";

/// The directory beside a cargo-fuzz `fuzz_targets` directory that holds each target's corpus.
const CORPUS: &str = "corpus";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    /// Path segments whose files verify code rather than ship it, which no signal reads.
    pub exclude: Vec<String>,
    #[serde(rename = "shape")]
    pub shapes: Vec<Shape>,
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
                "{OWNERS}: budget `{}` needs exactly one of `ceiling` and `floor`",
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
    let catalog: Catalog = toml::from_str(&text).with_context(|| path.display().to_string())?;
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
            "the shape catalog these machines verify every repository against is not installed",
            "chezmoi apply",
        )
        .evidence(format!("missing {}", path.display()))
        .into());
    }
    catalog(&path)
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

/// One gap: a site that nothing owns and nothing exempts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gap {
    pub shape: String,
    pub path: String,
    /// The line of the site, counting from one.
    pub line: usize,
    pub cause: Cause,
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
    let all = paths(root, tree)?;
    let owners: Owners = match contents(root, tree, &[OWNERS.to_owned()])?.get(OWNERS) {
        Some(bytes) => toml::from_str(std::str::from_utf8(bytes).context(OWNERS)?)
            .with_context(|| format!("{OWNERS} doesn't parse"))?,
        None => Owners::default(),
    };
    let mut result = Survey::default();
    result.owning.insert(OWNERS.to_owned());
    for budget in &owners.budgets {
        result.budgets.insert(budget.name.clone(), budget.bound()?);
    }
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
    let mut wanted = candidates.clone();
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
            let owner = owners
                .owners
                .iter()
                .filter(|owner| owner.shape == shape.id && owner.path == *path)
                .fold(Owner::Undeclared, |standing, owner| {
                    let holds = files
                        .get(&owner.by)
                        .is_some_and(|text| contains(text, shape.marker.as_bytes()))
                        && (!shape.corpus || corpus_present(&owner.by));
                    match (standing, holds) {
                        (Owner::Present, _) | (_, true) => Owner::Present,
                        _ => Owner::Broken,
                    }
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
                        cause,
                        key,
                    });
                }
            }
        }
    }
    Ok(result)
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
            format!("{OWNERS}: budget `{name}` {kind} {value} became {now}")
        })
        .collect()
}

/// What the gate concluded at one enforcement point.
#[derive(Debug)]
pub struct Outcome {
    pub verdict: Verdict,
    pub change: Change,
    /// The gaps behind the verdict: the added or touched ones when it refuses, and every gap otherwise.
    pub gaps: Vec<Gap>,
    pub lowered: Vec<String>,
}

impl Outcome {
    /// The refusal of a refusing verdict, naming every gap with its next action.
    #[must_use]
    pub fn refusal(&self, catalog: &Catalog) -> Option<Refusal> {
        let (rule, cause, next) = match self.verdict {
            Verdict::Admit | Verdict::Report => return None,
            Verdict::Added => (
                "assure.added",
                "the change adds code of a shape that nothing in the repository verifies",
                "add the owner each gap names, or an `Exempt as <reason>: <why>` comment above it, then retry",
            ),
            Verdict::Touched => (
                "assure.touched",
                "a file this turn changed still holds a shape that nothing verifies",
                "close each listed gap with its owner or an exemption beside it before finishing",
            ),
            Verdict::Lowered => (
                "assure.lowered",
                "the PR weakens a verification budget",
                "restore each listed budget in assure.toml and meet it",
            ),
            Verdict::Stalled => (
                "assure.stalled",
                "the repository has verification gaps and this PR closes none of them",
                "close at least one gap listed by the session start report in this PR",
            ),
        };
        let mut refusal = Refusal::new(rule, cause, next);
        for gap in &self.gaps {
            refusal = refusal.evidence(describe(catalog, gap));
        }
        for budget in &self.lowered {
            refusal = refusal.evidence(budget.clone());
        }
        Some(refusal)
    }
}

/// One gap with what its shape needs and its next action.
#[must_use]
pub fn describe(catalog: &Catalog, gap: &Gap) -> String {
    let shape = catalog.shapes.iter().find(|shape| shape.id == gap.shape);
    let cause = match gap.cause {
        Cause::Unowned => "has no owner",
        Cause::BrokenOwner => "names an owner in assure.toml that lacks its marker or corpus",
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
    before: &Tree,
    after: &Tree,
) -> Result<(BTreeSet<String>, Option<BTreeSet<String>>)> {
    let changed = changed(root, before, after)?;
    let probe = |tree: &Tree| survey(root, catalog, tree, Some(&BTreeSet::new()));
    let owning: BTreeSet<String> = probe(before)?
        .owning
        .into_iter()
        .chain(probe(after)?.owning)
        .collect();
    let widened = changed.iter().any(|path| {
        owning
            .iter()
            .any(|owner| path == owner || (owner.ends_with('/') && path.starts_with(owner)))
    });
    let limit = (!widened).then(|| changed.clone());
    Ok((changed, limit))
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
    let full = matches!(point, Point::Ready | Point::Start);
    let (changed, limit) = if point == Point::Start {
        (BTreeSet::new(), None)
    } else {
        scope(root, catalog, before, after)?
    };
    let limit = if full { None } else { limit };
    let old = if point == Point::Start {
        Survey::default()
    } else {
        survey(root, catalog, before, limit.as_ref())?
    };
    let new = survey(root, catalog, after, limit.as_ref())?;
    let (old_keys, new_keys) = (old.keys(), new.keys());
    let lowered = lowered(&old, &new);
    let change = Change {
        added: if point == Point::Start {
            0
        } else {
            shape_rules::fresh(&old_keys, &new_keys)
        },
        closed: shape_rules::fresh(&new_keys, &old_keys),
        before: old.gaps.len(),
        after: new.gaps.len(),
        touched: new
            .gaps
            .iter()
            .filter(|gap| changed.contains(&gap.path))
            .count(),
        lowered: lowered.len(),
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
    match outcome.refusal(catalog) {
        Some(refusal) => Err(refusal.into()),
        None => Ok(()),
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
/// A new branch counts from where it leaves the remote's default branch.
/// A push with no such base adds nothing, since every commit passed its pre-commit point.
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
            None
        } else {
            resolve(root, theirs)?
        };
        let base = match base {
            Some(base) => Some(base),
            None => match resolve(root, &format!("refs/remotes/{remote}/HEAD"))? {
                Some(default) => {
                    let output = git(root, &["merge-base", local, &default]).ok();
                    output.map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
                }
                None => None,
            },
        };
        let Some(base) = base else {
            continue;
        };
        enforce(
            catalog,
            &decide(
                root,
                catalog,
                Point::Push,
                &Tree::Commit(Some(base)),
                &Tree::Commit(Some(local.to_owned())),
            )?,
        )?;
    }
    Ok(())
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
        &Tree::Commit(None),
        &Tree::WorkingTree,
    )?;
    if outcome.verdict != Verdict::Report {
        return Ok(None);
    }
    let mut report = format!(
        "This repository has {} verification gaps. Commits and pushes refuse only new ones, and each PR must close at least one:\n",
        outcome.gaps.len()
    );
    for gap in &outcome.gaps {
        report.push_str("- ");
        report.push_str(&describe(catalog, gap));
        report.push('\n');
    }
    Ok(Some(report))
}

/// The `pr-workflow ready` point: refuses a PR that adds a gap or weakens a budget, and in a repository with gaps one that closes none.
///
/// # Errors
///
/// Returns the refusal, or an error when Git fails or this checkout lacks the base or the head.
pub fn ready(root: &Path, catalog: &Catalog, base: &str, head: &str) -> Result<()> {
    let fetch = || crate::refusal::command(&["git", "fetch", "origin", base]);
    let Some(tip) = resolve(root, &format!("refs/remotes/origin/{base}"))? else {
        return Err(Refusal::new(
            "assure.base",
            "the PR's base branch is missing locally, so the gate can't compare the PR with it",
            fetch(),
        )
        .evidence(format!("missing refs/remotes/origin/{base}"))
        .into());
    };
    let Some(head) = resolve(root, head)? else {
        return Err(Refusal::new(
            "assure.head",
            "the PR's head commit is missing locally, so the gate can't survey it",
            crate::refusal::command(&["git", "fetch", "origin"]),
        )
        .evidence(format!("missing {head}"))
        .into());
    };
    let base = String::from_utf8_lossy(&git(root, &["merge-base", &tip, &head])?)
        .trim()
        .to_owned();
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

/// The agent hook of `point`: the session start report on standard output, or the stop decision as the client's JSON.
///
/// # Errors
///
/// Returns an error when the hook input holds no JSON, the host lacks the catalog, or Git fails.
pub fn hook(point: AgentPoint) -> Result<()> {
    let Some(root) = repository() else {
        if point == AgentPoint::Stop {
            println!("{{}}");
        }
        return Ok(());
    };
    let catalog = installed()?;
    match point {
        AgentPoint::Start => {
            if let Some(report) = start(&root, &catalog)? {
                print!("{report}");
            }
            Ok(())
        }
        AgentPoint::Stop => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let continued = if input.trim().is_empty() {
                false
            } else {
                let value: serde_json::Value =
                    serde_json::from_str(&input).context("the stop hook input is not JSON")?;
                value["stop_hook_active"] == true
            };
            let outcome = stop(&root, &catalog)?;
            let reason = outcome.refusal(&catalog).map(|refusal| refusal.text());
            println!("{}", stop_response(reason.as_deref(), continued));
            Ok(())
        }
    }
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
            "systemMessage": "Verification gaps remain in the files this turn changed; automatic continuation is bounded."
        }),
    }
}
