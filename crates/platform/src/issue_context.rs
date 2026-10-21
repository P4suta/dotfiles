//! Checked issue contexts, kept only in owner-local state that every worktree of a repository shares.

pub use crate::signed_git::git;
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

const STATE: &str = ".local/state/pr-workflow/contexts";
const SCHEMA: u8 = 1;
const MAX_CONTEXTS: usize = 64;
const HEX_CHARACTERS_PER_BYTE: usize = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub repository: String,
    pub repository_id: u64,
    pub owner_id: u64,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub schema: u8,
    pub destination: Destination,
    pub issue: Option<u64>,
    pub branch: String,
    pub head: String,
    pub tree: String,
    pub scopes: Vec<String>,
    pub initial_dirty: BTreeMap<String, Option<String>>,
    pub adopted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub context: Context,
    pub object: String,
    pub signed_commit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u8,
    pub receipts: Vec<Receipt>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Local {
    checkout: PathBuf,
    common_git: PathBuf,
    origin: String,
    manifest: Manifest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    checkout: PathBuf,
    common_git: PathBuf,
    origin: String,
}

impl Binding {
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptionApproval {
    checkout: PathBuf,
    common_git: PathBuf,
    origin: String,
    baseline: Context,
}

/// # Errors
/// Returns an error if the directory lacks a readable checkout.
pub fn checkout(directory: &Path) -> Result<PathBuf> {
    crate::paths::canonical(Path::new(
        git(directory, &["rev-parse", "--show-toplevel"], None)?.trim(),
    ))
    .map_err(Into::into)
}

pub(crate) fn common_git(root: &Path) -> Result<PathBuf> {
    let common = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        None,
    )?;
    crate::paths::canonical(Path::new(common.trim())).map_err(Into::into)
}

/// # Errors
/// Returns an error if checkout identity capture fails.
pub fn binding(root: &Path) -> Result<Binding> {
    let root = checkout(root)?;
    Ok(Binding {
        common_git: common_git(&root)?,
        origin: git(&root, &["remote", "get-url", "origin"], None)?,
        checkout: root,
    })
}

/// # Errors
/// Returns an error for a detached or unreadable head.
pub fn branch(root: &Path) -> Result<String> {
    let branch = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"], None)?;
    ensure!(
        !branch.trim().is_empty(),
        "an issue context requires a named branch"
    );
    Ok(branch.trim().to_owned())
}

/// # Errors
/// Returns an error for an absolute, traversing, empty, or non-UTF-8 scope.
pub fn relative(path: &Path) -> Result<String> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "issue scope must be a repository-relative path without traversal: {}",
        path.display()
    );
    Ok(path
        .to_str()
        .context("issue scope must be UTF-8")?
        .replace('\\', "/"))
}

#[must_use]
pub fn scoped(scopes: &[String], path: &str) -> bool {
    scopes
        .iter()
        .any(|scope| rules::pr_rules::registered_path(scope.as_bytes(), path.as_bytes()))
}

/// # Errors
/// Returns an error when the edit crosses the checkout boundary.
pub fn edit_path(root: &Path, path: &Path) -> Result<PathBuf> {
    ensure!(
        !path
            .components()
            .any(|part| matches!(part, Component::ParentDir)),
        "edit path contains traversal"
    );
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    };
    let ancestor = absolute
        .ancestors()
        .find(|path| fs::symlink_metadata(path).is_ok())
        .context("edit has no existing ancestor")?;
    let canonical = crate::paths::canonical(ancestor)?;
    ensure!(
        canonical.starts_with(root),
        "edit path escapes the checkout through a symlink"
    );
    Ok(canonical.join(absolute.strip_prefix(ancestor)?))
}

/// # Errors
/// Returns an error if Git can't list the selected change.
pub fn paths(root: &Path, base: Option<&str>, staged: bool) -> Result<Vec<String>> {
    let mut paths = selected_paths(root, base, staged)?;
    if base.is_none() && !staged {
        paths.extend(selected_paths(root, None, true)?);
        paths.extend(
            git(
                root,
                &["ls-files", "--others", "--exclude-standard", "-z"],
                None,
            )?
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(|path| relative(Path::new(path)))
            .collect::<Result<Vec<_>>>()?,
        );
        paths.sort();
        paths.dedup();
    }
    Ok(paths)
}

fn selected_paths(root: &Path, base: Option<&str>, staged: bool) -> Result<Vec<String>> {
    let mut arguments = vec!["diff", "--name-only", "--no-renames", "-z"];
    let range = base.map(|base| format!("{base}...HEAD"));
    if staged {
        arguments.push("--cached");
    } else if let Some(range) = &range {
        arguments.push(range);
    }
    let output = if staged {
        crate::signed_git::index_git(root, &arguments)?
    } else {
        git(root, &arguments, None)?
    };
    let mut paths = split_paths(&output)?;
    if staged && let Some(merged) = upstream_merge(root) {
        let differing = split_paths(&crate::signed_git::index_git(
            root,
            &[
                "diff",
                "--name-only",
                "--no-renames",
                "-z",
                "--cached",
                &merged,
            ],
        )?)?;
        paths.retain(|path| differing.contains(path));
    }
    Ok(paths)
}

fn split_paths(output: &str) -> Result<Vec<String>> {
    output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| relative(Path::new(path)))
        .collect()
}

/// The commit of an in-progress merge that the remote default branch already holds.
/// A path staged with that commit's content came from the default branch, not from this branch's work.
fn upstream_merge(root: &Path) -> Option<String> {
    let merged = git(root, &["rev-parse", "-q", "--verify", "MERGE_HEAD"], None).ok()?;
    let merged = merged.trim().to_owned();
    git(
        root,
        &[
            "merge-base",
            "--is-ancestor",
            &merged,
            "refs/remotes/origin/HEAD",
        ],
        None,
    )
    .ok()?;
    Some(merged)
}

/// # Errors
/// Returns an error for an invalid scope, destination, or initial source state.
pub fn snapshot(
    root: &Path,
    destination: Destination,
    issue: Option<u64>,
    scopes: &[PathBuf],
) -> Result<Context> {
    ensure!(
        destination.repository_id != 0 && destination.owner_id != 0,
        "checked destination requires numeric repository and owner identities"
    );
    ensure!(
        !destination.required || issue.is_some_and(|issue| issue != 0) && !scopes.is_empty(),
        "personal implementation requires an open issue and explicit --path scope"
    );
    let scopes = scopes
        .iter()
        .map(|scope| relative(scope))
        .collect::<Result<Vec<_>>>()?;
    ensure!(scopes.len() <= MAX_CONTEXTS, "too many issue scopes");
    let head = git(root, &["rev-parse", "HEAD"], None)?.trim().to_owned();
    let tree = git(root, &["rev-parse", &format!("{head}^{{tree}}")], None)?
        .trim()
        .to_owned();
    let dirty = paths(root, None, false)?;
    let mut initial_dirty = BTreeMap::new();
    for path in dirty {
        let path = relative(Path::new(&path))?;
        let bytes = match fs::symlink_metadata(root.join(&path)) {
            Ok(metadata) if metadata.file_type().is_symlink() => Some(
                fs::read_link(root.join(&path))?
                    .to_str()
                    .context("non-UTF-8 dirty symlink")?
                    .as_bytes()
                    .to_vec(),
            ),
            Ok(_) => Some(fs::read(root.join(&path))?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let fingerprint = bytes.map(|bytes| crate::build_inputs::hex(&Sha256::digest(&bytes)));
        initial_dirty.insert(path, fingerprint);
    }
    Ok(Context {
        schema: SCHEMA,
        destination,
        issue,
        branch: branch(root)?,
        head,
        tree,
        scopes,
        initial_dirty,
        adopted: false,
    })
}

/// # Errors
/// Returns an error if the configured Git identity can't sign the context.
pub fn signed(root: &Path, context: Context) -> Result<Receipt> {
    let payload = serde_json::to_vec(&context)?;
    let commit = crate::signed_git::create(root, &context.head, &context.tree, &payload)?;
    Ok(Receipt {
        context,
        object: commit.object,
        signed_commit: commit.raw,
    })
}

/// # Errors
/// Returns an error when the signature, payload, or initial source binding differs.
pub fn authenticate(root: &Path, receipt: &Receipt, signers: &Path) -> Result<()> {
    let verified =
        crate::signed_git::authenticate(root, &receipt.object, &receipt.signed_commit, signers)?;
    let payload: Context = serde_json::from_str(&verified.payload)?;
    ensure!(
        payload == receipt.context && payload.schema == SCHEMA,
        "signed issue-context payload differs"
    );
    ensure!(
        verified.tree == payload.tree && verified.parents == [payload.head.clone()],
        "signed issue-context source closure differs"
    );
    ensure!(
        git(
            root,
            &["rev-parse", &format!("{}^{{tree}}", payload.head)],
            None
        )?
        .trim()
            == payload.tree,
        "issue-context initial tree differs"
    );
    for scope in &payload.scopes {
        ensure!(
            relative(Path::new(scope))? == *scope,
            "noncanonical issue scope"
        );
    }
    for (path, fingerprint) in &payload.initial_dirty {
        ensure!(
            relative(Path::new(path))? == *path
                && fingerprint.as_ref().is_none_or(|digest| digest.len()
                    == Sha256::output_size() * HEX_CHARACTERS_PER_BYTE
                    && digest.bytes().all(|byte| byte.is_ascii_hexdigit())),
            "invalid initial dirty source fingerprint"
        );
    }
    Ok(())
}

fn state_directory(root: &Path, home: &Path) -> Result<PathBuf> {
    let directory = home.join(STATE);
    fs::create_dir_all(&directory)?;
    let directory = crate::paths::canonical(&directory)?;
    ensure!(
        !directory.starts_with(root),
        "issue contexts belong to owner-local state outside the checkout"
    );
    Ok(directory)
}

/// The contexts of the repository `root` belongs to, which every worktree of it shares.
fn state_for(root: &Path, home: &Path) -> Result<PathBuf> {
    let key = crate::build_inputs::hex(&Sha256::digest(
        common_git(root)?.as_os_str().as_encoded_bytes(),
    ));
    Ok(state_directory(root, home)?.join(format!("{key}.json")))
}

/// The state of the checkout `root` alone, which holds its adoption approvals and the contexts of earlier releases.
fn checkout_state_for(root: &Path, home: &Path) -> Result<PathBuf> {
    let key = crate::build_inputs::hex(&Sha256::digest(root.as_os_str().as_encoded_bytes()));
    Ok(state_directory(root, home)?.join(format!("{key}.json")))
}

/// The repository's contexts, or the contexts an earlier release kept for the checkout alone.
fn existing_state(root: &Path, home: &Path) -> Result<Option<PathBuf>> {
    for path in [state_for(root, home)?, checkout_state_for(root, home)?] {
        if path.try_exists()? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// # Errors
/// Returns an error if the owner-local adoption request can't bind the checkout.
pub fn adoption_request(
    root: &Path,
    home: &Path,
    context: &Context,
) -> Result<(PathBuf, AdoptionApproval)> {
    let root = checkout(root)?;
    ensure!(
        !context.adopted,
        "adoption approval requires the original snapshot"
    );
    Ok((
        checkout_state_for(&root, home)?.with_extension("adoption.json"),
        AdoptionApproval {
            common_git: common_git(&root)?,
            origin: git(&root, &["remote", "get-url", "origin"], None)?,
            checkout: root,
            baseline: context.clone(),
        },
    ))
}

fn read_approval(path: &Path) -> Result<AdoptionApproval> {
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "owner adoption approval cannot be a symlink"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// # Errors
/// Returns an error for newly dirty implementation without a prior context or owner approval.
pub fn authorize_initial(root: &Path, context: &mut Context, adopt_existing: bool) -> Result<()> {
    let home = crate::home::home().context("issue-context owner home missing")?;
    let state = checkout_state_for(root, &home)?;
    let (approval_path, expected) = adoption_request(root, &home, context)?;
    if adopt_existing {
        ensure!(
            context.destination.required,
            "external work needs no adoption"
        );
        ensure!(
            !state.with_extension("adopted.json").exists(),
            "owner adoption was already consumed"
        );
        if !approval_path.exists() {
            let request = state.with_extension("adoption-request.json");
            write(&request, &expected)?;
            anyhow::bail!(
                "owner adoption approval missing; review {} and approve it at {} only with explicit owner authorization",
                request.display(),
                approval_path.display()
            );
        }
        ensure!(
            read_approval(&approval_path)? == expected,
            "owner adoption approval differs from this snapshot"
        );
        ensure!(
            rules::pr_rules::initial_change_allowed(
                context.destination.required,
                !context.initial_dirty.is_empty(),
                false,
                true
            ),
            "owner adoption not admitted"
        );
        context.adopted = true;
        return Ok(());
    }
    let previous = if existing_state(root, &home)?.is_some() {
        Some(local_for(root, &home)?)
    } else {
        None
    };
    let baseline = if state.with_extension("adopted.json").exists() {
        let baseline = read_approval(&state.with_extension("adopted.json"))?;
        ensure!(
            baseline.checkout == expected.checkout
                && baseline.common_git == expected.common_git
                && baseline.origin == expected.origin,
            "consumed owner adoption identity differs"
        );
        Some(baseline.baseline)
    } else {
        None
    };
    authorize_registered(context, previous.as_ref(), baseline.as_ref(), |old| {
        current_at(
            root,
            old,
            &context.destination.repository,
            &context.branch,
            &context.head,
        )
    })
}

fn authorize_registered(
    context: &Context,
    previous: Option<&Manifest>,
    baseline: Option<&Context>,
    mut validate_current: impl FnMut(&Context) -> Result<()>,
) -> Result<()> {
    if context.initial_dirty.is_empty() {
        return Ok(());
    }
    let eligible: Vec<_> = previous
        .into_iter()
        .flat_map(|manifest| &manifest.receipts)
        .map(|receipt| &receipt.context)
        .filter(|old| old.branch == context.branch && old.destination == context.destination)
        .collect();
    let baseline = baseline
        .filter(|old| old.branch == context.branch && old.destination == context.destination);
    for old in &eligible {
        validate_current(old)?;
    }
    if let Some(baseline) = baseline {
        validate_current(baseline)?;
    }
    for (path, fingerprint) in &context.initial_dirty {
        let mut covered = eligible.iter().any(|old| scoped(&old.scopes, path));
        if let Some(baseline) = baseline {
            covered |= baseline.initial_dirty.get(path) == Some(fingerprint);
        }
        ensure!(
            rules::pr_rules::initial_change_allowed(
                context.destination.required,
                true,
                covered,
                false
            ),
            "dirty implementation path {path} lacks an earlier checked context; only an explicit owner-approved one-time migration may adopt existing work"
        );
    }
    Ok(())
}

pub(crate) fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("issue-context parent missing")?;
    fs::create_dir_all(parent)?;
    ensure!(
        !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()),
        "issue-context files cannot be symlinks"
    );
    let mut candidate = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(candidate.as_file_mut(), value)?;
    candidate.as_file_mut().write_all(b"\n")?;
    candidate.as_file().sync_all()?;
    candidate.persist(path)?;
    Ok(())
}

/// # Errors
/// Returns an error if saving owner state fails.
pub fn persist(root: &Path, receipt: Receipt, expected: &Binding) -> Result<()> {
    let root = checkout(root)?;
    let home = crate::home::home().context("issue-context owner home missing")?;
    let state = state_for(&root, &home)?;
    let checkout_state = checkout_state_for(&root, &home)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state.with_extension("lock"))?;
    lock.lock()?;
    ensure!(
        binding(&root)? == *expected,
        "checkout identity changed before issue-context persistence"
    );
    let adoption = if receipt.context.adopted {
        let mut original = receipt.context.clone();
        original.adopted = false;
        let (approval, expected) = adoption_request(&root, &home, &original)?;
        ensure!(
            !checkout_state.with_extension("adopted.json").exists(),
            "owner adoption was already consumed"
        );
        ensure!(
            read_approval(&approval)? == expected,
            "owner adoption approval changed before persistence"
        );
        Some(approval)
    } else {
        None
    };
    let mut local: Local = if let Some(existing) = existing_state(&root, &home)? {
        serde_json::from_slice(&fs::read(existing)?)?
    } else {
        Local {
            checkout: expected.checkout.clone(),
            common_git: expected.common_git.clone(),
            origin: expected.origin.clone(),
            manifest: Manifest {
                schema: SCHEMA,
                receipts: Vec::new(),
            },
        }
    };
    ensure!(
        local.common_git == expected.common_git
            && rules::origin_rules::same_origin(&local.origin, &expected.origin)
            && local.manifest.schema == SCHEMA,
        "owner-local issue-context identity differs"
    );
    local.checkout.clone_from(&expected.checkout);
    // Another worktree may have deleted the branch of an earlier context, which then authorizes nothing.
    local.manifest.receipts.retain(|previous| {
        git(
            &root,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", previous.context.branch),
            ],
            None,
        )
        .is_ok()
    });
    require_scope_retention(&local.manifest, &receipt.context)?;
    let history_id = if receipt.object.is_empty() {
        format!(
            "local-{}",
            crate::build_inputs::hex(&Sha256::digest(serde_json::to_vec(&receipt.context)?))
        )
    } else {
        receipt.object.clone()
    };
    let history = state
        .parent()
        .context("issue-context state parent missing")?
        .join(history_id);
    if !history.exists() {
        write(&history, &receipt)?;
    }
    if let Some(approval) = adoption {
        fs::rename(approval, checkout_state.with_extension("adopted.json"))?;
    }
    local.manifest.receipts.retain(|previous| {
        previous.context.branch != receipt.context.branch
            || previous.context.destination == receipt.context.destination
                && previous.context.issue != receipt.context.issue
    });
    local.manifest.receipts.push(receipt);
    ensure!(
        local.manifest.receipts.len() <= MAX_CONTEXTS,
        "too many active issue contexts"
    );
    write(&state, &local)
}

fn require_scope_retention(previous: &Manifest, requested: &Context) -> Result<()> {
    let missing: BTreeSet<_> = previous
        .receipts
        .iter()
        .map(|receipt| &receipt.context)
        .filter(|old| {
            old.branch == requested.branch
                && old.destination == requested.destination
                && old.issue == requested.issue
        })
        .flat_map(|old| &old.scopes)
        .filter(|scope| {
            !requested.scopes.iter().any(|candidate| {
                rules::pr_rules::registered_path(scope.as_bytes(), candidate.as_bytes())
                    && rules::pr_rules::registered_path(candidate.as_bytes(), scope.as_bytes())
            })
        })
        .collect();
    ensure!(
        missing.is_empty(),
        "restarting this issue must keep previously registered scopes: {}",
        missing
            .iter()
            .map(|scope| format!("--path {scope}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

/// # Errors
/// Returns an error if owner context lacks data or differs from the checkout.
pub fn local(root: &Path) -> Result<Manifest> {
    let home = crate::home::home().context("issue-context owner home missing")?;
    local_for(root, &home)
}

/// # Errors
/// Returns an error for malformed or mismatched existing owner-local state.
pub fn optional_local(root: &Path) -> Result<Option<Manifest>> {
    let home = crate::home::home().context("issue-context owner home missing")?;
    let root = checkout(root)?;
    if existing_state(&root, &home)?.is_some() {
        local_for(&root, &home).map(Some)
    } else {
        Ok(None)
    }
}

/// # Errors
/// Returns an error if supplied owner context lacks data or differs from the repository.
pub fn local_for(root: &Path, home: &Path) -> Result<Manifest> {
    let root = checkout(root)?;
    let path = existing_state(&root, home)?.context(
        "checked issue context missing; run pr-workflow start --repo OWNER/REPO --issue NUMBER --path PATH",
    )?;
    let local: Local = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        local.common_git == common_git(&root)?
            && rules::origin_rules::same_origin(
                &local.origin,
                &git(&root, &["remote", "get-url", "origin"], None)?,
            )
            && local.manifest.schema == SCHEMA,
        "owner-local issue-context identity differs"
    );
    Ok(local.manifest)
}

/// # Errors
/// Returns an error when the context names another repository, branch, or source lineage.
pub fn current(root: &Path, context: &Context, repository: &str, branch: &str) -> Result<()> {
    current_at(root, context, repository, branch, "HEAD")
}

fn current_at(
    root: &Path,
    context: &Context,
    repository: &str,
    branch: &str,
    head: &str,
) -> Result<()> {
    ensure!(
        context.schema == SCHEMA
            && context
                .destination
                .repository
                .eq_ignore_ascii_case(repository),
        "issue-context destination differs"
    );
    ensure!(context.branch == branch, "issue-context branch differs");
    git(
        root,
        &["merge-base", "--is-ancestor", &context.head, head],
        None,
    )?;
    Ok(())
}

/// # Errors
/// Returns an error when any edit lacks a checked owner context.
pub fn guard(root: &Path, paths: &[PathBuf]) -> Result<()> {
    let home = crate::home::home().context("issue-context owner home missing")?;
    guard_for(root, paths, &home)
}

/// # Errors
/// Returns an error for an unregistered edit or invalid owner context.
pub fn guard_for(root: &Path, paths: &[PathBuf], home: &Path) -> Result<()> {
    let root = checkout(root)?;
    if crate::local_issues::active_for(&root, home)? {
        return crate::local_issues::guard_for(&root, paths, home);
    }
    let manifest = local_for(&root, home)?;
    let branch = branch(&root)?;
    let mut scopes = Vec::new();
    let mut classification = None;
    for receipt in &manifest.receipts {
        let context = &receipt.context;
        if context.branch != branch {
            continue;
        }
        current(&root, context, &context.destination.repository, &branch)?;
        if let Some(previous) = &classification {
            ensure!(
                *previous == context.destination,
                "mixed owner-local destination classifications"
            );
        }
        classification = Some(context.destination.clone());
        ensure!(
            !context.destination.required
                || context.issue.is_some_and(|issue| issue != 0) && !context.scopes.is_empty(),
            "invalid owner-local issue context"
        );
        for scope in &context.scopes {
            ensure!(
                relative(Path::new(scope))? == *scope,
                "invalid owner-local scope"
            );
        }
        scopes.extend(context.scopes.iter().cloned());
    }
    for path in paths {
        let absolute = edit_path(&root, path)?;
        let relative = relative(
            absolute
                .strip_prefix(&root)
                .context("edit path lies outside the context checkout")?,
        )?;
        let verdict = rules::pr_rules::context_gate(rules::pr_rules::ContextFacts {
            required: classification
                .as_ref()
                .is_none_or(|destination| destination.required),
            present: !scopes.is_empty(),
            authentic: true,
            destination: true,
            branch: true,
            source: true,
            open: true,
            scoped: scoped(&scopes, &relative),
        });
        ensure!(
            matches!(
                verdict,
                rules::pr_rules::ContextGate::Accepted | rules::pr_rules::ContextGate::NotRequired
            ),
            "edit path {relative} lacks a registered issue context ({verdict:?})"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::ensure;

    const TEST_REPOSITORY_ID: u64 = 7;
    const TEST_OWNER_ID: u64 = 11;
    const TEST_ISSUE: u64 = 170;
    const TEST_NEXT_ISSUE: u64 = 171;
    const TEST_REPOSITORY: &str = "fixture/context";
    const TEST_BRANCH: &str = "context-fixture";
    const TEST_ORIGIN: &str = "git@github.com:fixture/context.git";
    const FIRST_SOURCE: &str = "first-source";
    const SECOND_SOURCE: &str = "second-source";
    const BASELINE_SOURCE: &str = "baseline-source";
    const CHILD_ROOT: &str = "DOTFILES_ISSUE_CONTEXT_PERSIST_ROOT";
    const FIRST_OBJECT: &str = "1111111111111111111111111111111111111111";
    const OMITTED_OBJECT: &str = "2222222222222222222222222222222222222222";
    const UNION_OBJECT: &str = "3333333333333333333333333333333333333333";
    const OTHER_OBJECT: &str = "4444444444444444444444444444444444444444";
    const CLASSIFIED_OBJECT: &str = "5555555555555555555555555555555555555555";

    fn fixture_command(
        program: impl AsRef<std::ffi::OsStr>,
        global: &Path,
    ) -> Result<std::process::Command> {
        let scope = crate::paths::canonical(global.parent().context("fixture parent")?)?;
        let temporary = crate::paths::canonical(&std::env::temp_dir())?;
        ensure!(
            scope.starts_with(&temporary) && scope != temporary,
            "owned temporary fixture required"
        );
        let mut command = crate::process::command(program);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        command
            .env("SKILL_OPS_STATE", scope.join("state"))
            .env("GIT_CONFIG_GLOBAL", global)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("XDG_CONFIG_HOME", global.with_extension("config-home"));
        Ok(command)
    }

    fn context(head: &str, scopes: &[&str]) -> Context {
        Context {
            schema: SCHEMA,
            destination: Destination {
                repository: TEST_REPOSITORY.into(),
                repository_id: TEST_REPOSITORY_ID,
                owner_id: TEST_OWNER_ID,
                required: true,
            },
            issue: Some(TEST_ISSUE),
            branch: TEST_BRANCH.into(),
            head: head.into(),
            tree: "fixture-tree".into(),
            scopes: scopes.iter().map(|scope| (*scope).into()).collect(),
            initial_dirty: BTreeMap::new(),
            adopted: false,
        }
    }

    fn receipt(context: Context, object: &str) -> Receipt {
        Receipt {
            context,
            object: object.into(),
            signed_commit: "fixture state only; no authenticated transport claim".into(),
        }
    }

    fn prior_contexts() -> Manifest {
        Manifest {
            schema: SCHEMA,
            receipts: vec![
                receipt(context(FIRST_SOURCE, &["alpha"]), FIRST_OBJECT),
                receipt(context(SECOND_SOURCE, &["beta"]), OTHER_OBJECT),
            ],
        }
    }

    #[test]
    fn writer_terminates_json_without_changing_signed_payload() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("contexts.json");
        let mut manifest = prior_contexts();
        manifest.receipts[0]
            .signed_commit
            .push_str("\n\"quoted\"\\payload\r\n");
        let expected = serde_json::to_vec(&manifest)?;
        write(&path, &manifest)?;
        let bytes = fs::read(path)?;
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert_eq!(&bytes[..bytes.len() - 1], expected);
        let decoded: Manifest = serde_json::from_slice(&bytes)?;
        assert_eq!(decoded.schema, manifest.schema);
        assert_eq!(decoded.receipts, manifest.receipts);
        Ok(())
    }

    #[test]
    fn prior_lineage_queries_are_bound_to_receipts_instead_of_dirty_paths() -> Result<()> {
        let previous = prior_contexts();
        let mut baseline = context(BASELINE_SOURCE, &[]);
        baseline.initial_dirty.insert("removed.rs".into(), None);
        let mut candidate = context("current-source", &["alpha", "beta", "removed.rs"]);
        candidate.initial_dirty = BTreeMap::from([
            ("alpha/a.rs".into(), Some("first".into())),
            ("beta/b.rs".into(), Some("second".into())),
            ("removed.rs".into(), None),
        ]);
        for extra in [None, Some("alpha/another.rs")] {
            if let Some(path) = extra {
                candidate.initial_dirty.insert(path.into(), None);
            }
            let mut queries = Vec::new();
            authorize_registered(&candidate, Some(&previous), Some(&baseline), |old| {
                queries.push(old.head.clone());
                Ok(())
            })?;
            assert_eq!(queries, [FIRST_SOURCE, SECOND_SOURCE, BASELINE_SOURCE]);
        }
        Ok(())
    }

    #[test]
    fn stale_lineage_errors_remain_authoritative_despite_complete_path_coverage() {
        let previous = prior_contexts();
        let mut candidate = context("current-source", &["alpha"]);
        candidate.initial_dirty.insert("alpha/a.rs".into(), None);
        let error = authorize_registered(&candidate, Some(&previous), None, |old| {
            ensure!(old.head != FIRST_SOURCE, "stale prior lineage");
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "stale prior lineage");
    }

    #[test]
    fn a_stale_baseline_is_checked_even_when_registered_scopes_cover_every_path() {
        let previous = prior_contexts();
        let baseline = context(BASELINE_SOURCE, &[]);
        let mut candidate = context("current-source", &["alpha"]);
        candidate.initial_dirty.insert("alpha/a.rs".into(), None);
        let mut queries = Vec::new();
        let error = authorize_registered(&candidate, Some(&previous), Some(&baseline), |old| {
            queries.push(old.head.clone());
            ensure!(old.head != BASELINE_SOURCE, "stale baseline lineage");
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "stale baseline lineage");
        assert_eq!(queries, [FIRST_SOURCE, SECOND_SOURCE, BASELINE_SOURCE]);
    }

    #[test]
    fn empty_dirty_inputs_do_not_query_prior_lineage() -> Result<()> {
        let previous = prior_contexts();
        let baseline = context(BASELINE_SOURCE, &[]);
        let candidate = context("current-source", &["alpha"]);
        let mut queried = false;
        authorize_registered(&candidate, Some(&previous), Some(&baseline), |_| {
            queried = true;
            anyhow::bail!("empty input must not query lineage")
        })?;
        assert!(!queried);
        Ok(())
    }

    #[test]
    fn unrelated_contexts_never_authorize_a_changed_path_or_trigger_lineage_queries() {
        let mut previous = prior_contexts();
        for entry in &mut previous.receipts {
            entry.context.branch = "another-branch".into();
        }
        let mut baseline = context(BASELINE_SOURCE, &[]);
        baseline.destination.repository = "another/repository".into();
        let mut candidate = context("current-source", &["alpha"]);
        candidate.initial_dirty.insert("alpha/a.rs".into(), None);
        let mut queried = false;
        let error = authorize_registered(&candidate, Some(&previous), Some(&baseline), |_| {
            queried = true;
            anyhow::bail!("unrelated input must not query lineage")
        })
        .unwrap_err();
        assert!(!queried);
        assert!(
            error
                .to_string()
                .contains("dirty implementation path alpha/a.rs")
        );
    }

    #[test]
    fn changed_adoption_fingerprints_remain_refused_after_lineage_validation() {
        let mut baseline = context(BASELINE_SOURCE, &[]);
        baseline
            .initial_dirty
            .insert("adopted.rs".into(), Some("old".into()));
        let mut candidate = context("current-source", &["adopted.rs"]);
        candidate
            .initial_dirty
            .insert("adopted.rs".into(), Some("changed".into()));
        let mut queries = Vec::new();
        let error = authorize_registered(&candidate, None, Some(&baseline), |old| {
            queries.push(old.head.clone());
            Ok(())
        })
        .unwrap_err();
        assert_eq!(queries, [BASELINE_SOURCE]);
        assert!(
            error
                .to_string()
                .contains("dirty implementation path adopted.rs")
        );
    }

    fn state_contents(directory: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
        fs::read_dir(directory)?
            .map(|entry| {
                let entry = entry?;
                Ok((
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("fixture filename"))?,
                    fs::read(entry.path())?,
                ))
            })
            .collect()
    }

    #[test]
    fn broader_scopes_do_not_implicitly_replace_exact_previous_registrations() {
        let previous = Manifest {
            schema: SCHEMA,
            receipts: vec![receipt(
                context(FIRST_SOURCE, &["alpha/nested"]),
                FIRST_OBJECT,
            )],
        };
        let candidate = context(FIRST_SOURCE, &["alpha"]);
        let error = require_scope_retention(&previous, &candidate).unwrap_err();
        assert!(error.to_string().contains("--path alpha/nested"));
    }

    fn snapshot_lineage_contract(root: &Path) -> Result<()> {
        let tree = git(root, &["mktree"], Some(b""))?.trim().to_owned();
        let captured = git(root, &["commit-tree", &tree, "-m", "captured"], None)?
            .trim()
            .to_owned();
        let advanced = git(root, &["commit-tree", &tree, "-m", "advanced"], None)?
            .trim()
            .to_owned();
        assert_ne!(captured, advanced);
        git(root, &["update-ref", "HEAD", &advanced], None)?;
        let old = context(&advanced, &["alpha"]);
        current(root, &old, TEST_REPOSITORY, TEST_BRANCH)?;
        assert!(current_at(root, &old, TEST_REPOSITORY, TEST_BRANCH, &captured).is_err());
        current_at(
            root,
            &context(&captured, &["alpha"]),
            TEST_REPOSITORY,
            TEST_BRANCH,
            &captured,
        )?;
        Ok(())
    }

    fn replacement_contract(root: &Path) -> Result<()> {
        snapshot_lineage_contract(root)?;
        let expected = binding(root)?;
        let home = crate::home::home().context("fixture home")?;
        let state = state_for(root, &home)?;
        let directory = state.parent().context("fixture state directory")?;
        persist(
            root,
            receipt(context(FIRST_SOURCE, &["alpha", "beta"]), FIRST_OBJECT),
            &expected,
        )?;
        let before = state_contents(directory)?;
        assert!(!root.join(".pr-workflow").exists());
        let error = persist(
            root,
            receipt(context(FIRST_SOURCE, &["alpha"]), OMITTED_OBJECT),
            &expected,
        )
        .unwrap_err();
        assert!(error.to_string().contains("--path beta"));
        assert_eq!(state_contents(directory)?, before);
        assert!(!directory.join(OMITTED_OBJECT).exists());

        persist(
            root,
            receipt(
                context(FIRST_SOURCE, &["alpha", "beta", "gamma"]),
                UNION_OBJECT,
            ),
            &expected,
        )?;
        let manifest = local_for(root, &home)?;
        assert_eq!(manifest.receipts.len(), 1);
        assert_eq!(
            manifest.receipts[0].context.scopes,
            ["alpha", "beta", "gamma"]
        );
        assert!(directory.join(FIRST_OBJECT).exists());
        assert!(directory.join(UNION_OBJECT).exists());

        let mut other = context(FIRST_SOURCE, &["delta"]);
        other.issue = Some(TEST_NEXT_ISSUE);
        persist(root, receipt(other, OTHER_OBJECT), &expected)?;
        let manifest = local_for(root, &home)?;
        assert!(
            manifest
                .receipts
                .iter()
                .any(|entry| entry.context.issue == Some(TEST_ISSUE))
        );
        assert!(
            manifest
                .receipts
                .iter()
                .any(|entry| entry.context.issue == Some(TEST_NEXT_ISSUE))
        );

        let mut classified = context(FIRST_SOURCE, &["epsilon"]);
        classified.destination.required = false;
        persist(root, receipt(classified, CLASSIFIED_OBJECT), &expected)?;
        assert!(
            local_for(root, &home)?
                .receipts
                .iter()
                .all(|entry| !entry.context.destination.required)
        );
        Ok(())
    }

    fn fixture_git(root: &Path, global: &Path, arguments: &[&str]) -> Result<()> {
        let output = fixture_command("git", global)?
            .current_dir(root)
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "fixture Git {arguments:?}: status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn origin_identity_contract(root: &Path) -> Result<()> {
        snapshot_lineage_contract(root)?;
        let home = crate::home::home().context("fixture home")?;
        let state = state_for(root, &home)?;
        let origins = [
            TEST_ORIGIN,
            "https://github.com/fixture/context.git",
            "ssh://git@github.com/fixture/context.git",
            "https://github.com/FiXtUrE/CoNtExT",
            "git@github.com:FIXTURE/CONTEXT",
        ];
        for stored in origins {
            for current in origins {
                let expected = binding(root)?;
                let original = receipt(context(FIRST_SOURCE, &["alpha"]), FIRST_OBJECT);
                write(
                    &state,
                    &Local {
                        checkout: expected.checkout,
                        common_git: expected.common_git,
                        origin: format!("{stored}\n"),
                        manifest: Manifest {
                            schema: SCHEMA,
                            receipts: vec![original.clone()],
                        },
                    },
                )?;
                git(root, &["remote", "set-url", "origin", current], None)?;
                assert_eq!(
                    local_for(root, &home)?.receipts,
                    std::slice::from_ref(&original)
                );
                persist(root, original.clone(), &binding(root)?)?;
                assert_eq!(local_for(root, &home)?.receipts, [original]);
                let saved: Local = serde_json::from_slice(&fs::read(&state)?)?;
                assert_eq!(saved.origin, format!("{stored}\n"));
            }
        }
        let before = fs::read(&state)?;
        for different in [
            "https://github.com/fixture/another.git",
            "git@github.com:another/context.git",
            "https://example.com/fixture/context.git",
        ] {
            git(root, &["remote", "set-url", "origin", different], None)?;
            assert_eq!(
                local_for(root, &home).unwrap_err().to_string(),
                "owner-local issue-context identity differs"
            );
            assert_eq!(
                persist(
                    root,
                    receipt(context(FIRST_SOURCE, &["alpha"]), OTHER_OBJECT),
                    &binding(root)?,
                )
                .unwrap_err()
                .to_string(),
                "owner-local issue-context identity differs"
            );
            assert_eq!(fs::read(&state)?, before);
        }
        Ok(())
    }

    fn fixture_test(name: &str, contract: fn(&Path) -> Result<()>) -> Result<()> {
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            return contract(Path::new(&root));
        }
        let scope = tempfile::tempdir()?;
        let root = scope.path().join("checkout");
        let home = scope.path().join("owner");
        fs::create_dir_all(&root)?;
        fs::create_dir_all(&home)?;
        let global = scope.path().join("fixture.gitconfig");
        fs::write(
            &global,
            "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n",
        )?;
        fixture_git(
            &root,
            &global,
            &["init", "--quiet", "--initial-branch", TEST_BRANCH],
        )?;
        fixture_git(&root, &global, &["remote", "add", "origin", TEST_ORIGIN])?;
        let output = fixture_command(std::env::current_exe()?, &global)?
            .args(["--exact", name, "--nocapture"])
            .env(CHILD_ROOT, &root)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .output()?;
        ensure!(
            output.status.success(),
            "fixture persistence: status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    #[test]
    fn replacing_a_context_preserves_scopes_before_any_state_or_history_write() -> Result<()> {
        fixture_test(
            "issue_context::tests::replacing_a_context_preserves_scopes_before_any_state_or_history_write",
            replacement_contract,
        )
    }

    #[test]
    fn stored_origin_identity_survives_transport_changes_without_migration() -> Result<()> {
        fixture_test(
            "issue_context::tests::stored_origin_identity_survives_transport_changes_without_migration",
            origin_identity_contract,
        )
    }
}
