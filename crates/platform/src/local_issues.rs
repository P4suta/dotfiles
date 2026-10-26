//! Local issue authority stored outside the project and separate from remote receipts.

use crate::issue_context::{self as context, common_git, git};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const STATE: &str = ".local/state/pr-workflow/local-issues";
const SCHEMA: u8 = 1;
const MISSING_HEAD: i32 = 1;
const IGNORE_FILE: &str = ".gitignore";
const NESTED_IGNORE_FILES: &str = ":(glob)**/.gitignore";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Open,
    Closed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Source {
    Unborn,
    Commit { head: String, tree: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub status: Status,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    issue: u64,
    branch: String,
    source: Source,
    scopes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum BaselineKind {
    InitialLocalImport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct InitialBaseline {
    kind: BaselineKind,
    checkout: PathBuf,
    common_git: PathBuf,
    revision: u64,
    branch: String,
    issue: u64,
    scopes: Vec<String>,
    paths: Vec<String>,
    tree: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Import {
    baseline: InitialBaseline,
    object: String,
    signed_commit: String,
}

#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub snapshot: String,
    pub tree: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoteLink {
    local_revision: u64,
    local_digest: String,
    repository: String,
    signed_context: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Project {
    schema: u8,
    checkout: PathBuf,
    common_git: PathBuf,
    revision: u64,
    previous: Option<String>,
    issues: BTreeMap<u64, Issue>,
    selections: Vec<Selection>,
    remote_links: Vec<RemoteLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    imports: Vec<Import>,
}

#[derive(Debug)]
pub struct Promotion {
    revision: u64,
    digest: String,
}

#[derive(Clone, Copy)]
enum Persistence {
    Local,
    Promotion,
}

fn home() -> Result<PathBuf> {
    crate::home::home().context("local issue owner home missing")
}

fn path(root: &Path, home: &Path) -> PathBuf {
    let key = crate::build_inputs::hex(&Sha256::digest(root.as_os_str().as_encoded_bytes()));
    home.join(STATE).join(format!("{key}.json"))
}

fn no_remote(root: &Path) -> Result<()> {
    ensure!(
        git(root, &["remote"], None)?.trim().is_empty(),
        "local-only issue authority cannot authorize a remote project; use checked remote start with --promote-local"
    );
    Ok(())
}

fn read(root: &Path, home: &Path) -> Result<Project> {
    let path = path(root, home);
    let parent =
        crate::paths::canonical(path.parent().context("local issue state parent missing")?)?;
    ensure!(
        parent.starts_with(crate::paths::canonical(home)?) && !parent.starts_with(root),
        "local issues belong to the owner home outside the project"
    );
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "local issue state cannot be a symlink"
    );
    let project: Project = serde_json::from_slice(&fs::read(&path)?)?;
    ensure!(
        project.schema == SCHEMA
            && project.revision != 0
            && project.checkout == root
            && project.common_git == common_git(root)?,
        "owner-local project identity differs"
    );
    for (number, issue) in &project.issues {
        ensure!(
            *number != 0 && issue.number == *number,
            "invalid local issue identity"
        );
    }
    for selection in &project.selections {
        ensure!(
            project.issues.contains_key(&selection.issue)
                && !selection.branch.is_empty()
                && !selection.scopes.is_empty(),
            "invalid local issue selection"
        );
        for scope in &selection.scopes {
            ensure!(
                context::relative(Path::new(scope))? == *scope,
                "invalid local issue scope"
            );
        }
    }
    for import in &project.imports {
        authenticate_import(root, home, &project, import)?;
    }
    let mut bytes = serde_json::to_vec(&project)?;
    let record = path.with_extension("history").join(format!(
        "{}.json",
        crate::build_inputs::hex(&Sha256::digest(&bytes))
    ));
    bytes.push(b'\n');
    ensure!(
        !fs::symlink_metadata(&record)?.file_type().is_symlink() && fs::read(record)? == bytes,
        "local issue state has no matching actual history record"
    );
    Ok(project)
}

fn lock(root: &Path, home: &Path) -> Result<fs::File> {
    let path = path(root, home);
    let directory = path.parent().context("local issue state parent missing")?;
    fs::create_dir_all(directory)?;
    let canonical = crate::paths::canonical(directory)?;
    ensure!(
        canonical.starts_with(crate::paths::canonical(home)?) && !canonical.starts_with(root),
        "local issues belong to the owner home outside the project"
    );
    ensure!(
        !fs::symlink_metadata(path.with_extension("lock"))
            .is_ok_and(|metadata| metadata.file_type().is_symlink()),
        "local issue lock cannot be a symlink"
    );
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    lock.lock()?;
    Ok(lock)
}

fn save(root: &Path, home: &Path, project: &mut Project, kind: Persistence) -> Result<()> {
    ensure!(
        project.checkout == context::checkout(root)? && project.common_git == common_git(root)?,
        "local project identity changed before persistence"
    );
    if matches!(kind, Persistence::Local) {
        no_remote(root)?;
    }
    project.revision = project
        .revision
        .checked_add(1)
        .context("local issue revision exhausted")?;
    let path = path(root, home);
    ensure!(
        !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()),
        "local issue state cannot be a symlink"
    );
    project.previous = match fs::read(&path) {
        Ok(bytes) => Some(crate::build_inputs::hex(&Sha256::digest(bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let history = path.with_extension("history");
    fs::create_dir_all(&history)?;
    ensure!(
        crate::paths::canonical(&history)?.starts_with(crate::paths::canonical(
            path.parent().context("local state parent missing")?
        )?),
        "local issue history escaped its owner directory"
    );
    let mut bytes = serde_json::to_vec(project)?;
    let record = history.join(format!(
        "{}.json",
        crate::build_inputs::hex(&Sha256::digest(&bytes))
    ));
    bytes.push(b'\n');
    if record.exists() {
        ensure!(
            fs::read(&record)? == bytes,
            "local issue history content differs"
        );
    } else {
        context::write(&record, project)?;
    }
    context::write(&path, project)
}

/// # Errors
/// Returns an error if preparing local Git metadata or owner-local state fails.
pub fn init(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)?;
    let root = crate::paths::canonical(directory)?;
    if !root.join(".git").exists() {
        ensure!(
            context::checkout(&root).is_err(),
            "local initialization cannot nest a project in another checkout"
        );
        git(&root, &["init", "--quiet", "--initial-branch=main"], None)?;
    }
    ensure!(
        context::checkout(&root)? == root,
        "local project root differs"
    );
    no_remote(&root)?;
    let home = home()?;
    let _lock = lock(&root, &home)?;
    if path(&root, &home).exists() {
        read(&root, &home)?;
        return Ok(());
    }
    let mut project = Project {
        schema: SCHEMA,
        common_git: common_git(&root)?,
        checkout: root.clone(),
        revision: 0,
        previous: None,
        issues: BTreeMap::new(),
        selections: Vec::new(),
        remote_links: Vec::new(),
        imports: Vec::new(),
    };
    save(&root, &home, &mut project, Persistence::Local)
}

fn prepared_for(root: &Path, home: &Path) -> Result<bool> {
    Ok(path(&context::checkout(root)?, home).exists())
}

/// # Errors
/// Returns an error if reading the prepared project's remote configuration fails.
pub fn active(root: &Path) -> Result<bool> {
    active_for(root, &home()?)
}

/// # Errors
/// Returns an error if reading the prepared project's remote configuration fails.
pub fn active_for(root: &Path, home: &Path) -> Result<bool> {
    Ok(prepared_for(root, home)? && git(root, &["remote"], None)?.trim().is_empty())
}

/// # Errors
/// Returns an error when Git returns an unsupported head state.
pub fn source(root: &Path) -> Result<Source> {
    let output = crate::tool::Tool::Git
        .command()
        .current_dir(root)
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .output()?;
    match output.status.code() {
        Some(0) => {
            let head = String::from_utf8(output.stdout)?.trim().to_owned();
            let tree = git(root, &["rev-parse", &format!("{head}^{{tree}}")], None)?
                .trim()
                .to_owned();
            Ok(Source::Commit { head, tree })
        }
        Some(MISSING_HEAD) => {
            context::branch(root)?;
            Ok(Source::Unborn)
        }
        _ => anyhow::bail!(
            "local source inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

fn current(root: &Path, selection: &Selection) -> Result<()> {
    ensure!(
        context::branch(root)? == selection.branch,
        "local issue branch differs"
    );
    if let Source::Commit { head, tree } = &selection.source {
        ensure!(
            git(root, &["rev-parse", &format!("{head}^{{tree}}")], None)?.trim() == tree,
            "local issue source tree differs"
        );
        git(root, &["merge-base", "--is-ancestor", head, "HEAD"], None)?;
    } else {
        source(root)?;
    }
    Ok(())
}

fn observe_source(root: &Path, project: &mut Project) -> Result<bool> {
    let branch = context::branch(root)?;
    let source = source(root)?;
    let mut changed = false;
    if matches!(source, Source::Commit { .. }) {
        for selection in &mut project.selections {
            if selection.branch == branch && selection.source == Source::Unborn {
                selection.source = source.clone();
                changed = true;
            }
        }
    }
    Ok(changed)
}

/// # Errors
/// Returns an error if creating the local issue fails in the prepared project.
pub fn create(root: &Path, title: String, body: String) -> Result<u64> {
    let root = context::checkout(root)?;
    no_remote(&root)?;
    let home = home()?;
    let _lock = lock(&root, &home)?;
    let mut project = read(&root, &home)?;
    let number = project
        .issues
        .last_key_value()
        .map_or(0, |(number, _)| *number)
        .checked_add(1)
        .context("local issue identity exhausted")?;
    project.issues.insert(
        number,
        Issue {
            number,
            title,
            body,
            status: Status::Open,
        },
    );
    save(&root, &home, &mut project, Persistence::Local)?;
    Ok(number)
}

/// # Errors
/// Returns an error if the selected issue has closed, gone stale, or would permit a late edit.
pub fn select(root: &Path, issue: u64, scopes: &[PathBuf]) -> Result<()> {
    let root = context::checkout(root)?;
    no_remote(&root)?;
    let home = home()?;
    let _lock = lock(&root, &home)?;
    let mut project = read(&root, &home)?;
    ensure!(
        project
            .issues
            .get(&issue)
            .is_some_and(|issue| issue.status == Status::Open),
        "local issue missing or closed"
    );
    observe_source(&root, &mut project)?;
    let scopes = scopes
        .iter()
        .map(|scope| context::relative(scope))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !scopes.is_empty(),
        "local implementation needs an explicit --path scope"
    );
    for path in selection_paths(&root)? {
        check_path(&root, &project, &path).with_context(|| {
            format!("late local selection cannot adopt unregistered dirty path {path}")
        })?;
    }
    let selection = Selection {
        issue,
        branch: context::branch(&root)?,
        source: source(&root)?,
        scopes,
    };
    project
        .selections
        .retain(|previous| previous.issue != issue || previous.branch != selection.branch);
    project.selections.push(selection);
    save(&root, &home, &mut project, Persistence::Local)
}

fn selection_paths(root: &Path) -> Result<Vec<String>> {
    let inherited = match source(root)? {
        Source::Commit { head, .. } => Some(format!("--with-tree={head}")),
        Source::Unborn => None,
    };
    let mut arguments = vec!["ls-files", "--cached", "-z"];
    if let Some(inherited) = &inherited {
        arguments.push(inherited);
    }
    arguments.extend(["--", IGNORE_FILE, NESTED_IGNORE_FILES]);
    let ignores = crate::signed_git::index_git(root, &arguments)?;
    for path in ignores.split('\0').filter(|path| !path.is_empty()) {
        if !fs::symlink_metadata(root.join(path))
            .is_ok_and(|metadata| metadata.file_type().is_file())
        {
            return Err(crate::refusal::Refusal::new(
                "pr.local-ignore",
                format!("missing ignore file {path}; local selection needs the versioned ignore rules before listing untracked artifacts"),
                format!("restore {path} from the index or its source commit, then retry local selection"),
            ).into());
        }
    }
    context::paths(root, None, false)
}

fn import_signers(root: &Path, home: &Path) -> Result<PathBuf> {
    let output = crate::tool::Tool::Git
        .command()
        .current_dir(root)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env_remove("GIT_CONFIG_GLOBAL")
        .args([
            "config",
            "--global",
            "--path",
            "--get",
            "gpg.ssh.allowedSignersFile",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "initial import owner signers are not configured"
    );
    let configured = String::from_utf8(output.stdout)?;
    let signers = crate::paths::canonical(Path::new(configured.trim()))?;
    ensure!(
        signers.starts_with(crate::paths::canonical(home)?) && !signers.starts_with(root),
        "initial import requires trusted owner signers outside the project"
    );
    Ok(signers)
}

fn authenticate_import(root: &Path, home: &Path, project: &Project, import: &Import) -> Result<()> {
    let verified = crate::signed_git::authenticate(
        root,
        &import.object,
        &import.signed_commit,
        &import_signers(root, home)?,
    )?;
    let baseline: InitialBaseline = serde_json::from_str(&verified.payload)?;
    ensure!(
        baseline == import.baseline
            && baseline.checkout == project.checkout
            && baseline.common_git == project.common_git
            && baseline.revision != 0
            && baseline.revision < project.revision
            && !baseline.branch.is_empty()
            && project.issues.contains_key(&baseline.issue)
            && !baseline.scopes.is_empty()
            && baseline
                .scopes
                .iter()
                .all(|scope| context::relative(Path::new(scope))
                    .is_ok_and(|relative| relative == *scope))
            && !baseline.paths.is_empty()
            && baseline
                .paths
                .iter()
                .all(|path| context::relative(Path::new(path))
                    .is_ok_and(|relative| relative == *path)
                    && context::scoped(&baseline.scopes, path))
            && verified.tree == baseline.tree
            && verified.parents.is_empty(),
        "signed initial import baseline differs"
    );
    Ok(())
}

fn initial_baseline(
    root: &Path,
    project: &Project,
    issue: u64,
    scopes: &[PathBuf],
    expected: Option<&str>,
) -> Result<InitialBaseline> {
    ensure!(
        std::env::var_os("GIT_INDEX_FILE").is_none(),
        "initial import cannot use an alternate Git index"
    );
    let mut scopes = scopes
        .iter()
        .map(|scope| context::relative(scope))
        .collect::<Result<Vec<_>>>()?;
    scopes.sort();
    scopes.dedup();
    let paths = git(root, &["ls-files", "--cached", "-z"], None)?
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| {
            let relative = context::relative(Path::new(path))?;
            ensure!(
                relative == path,
                "initial import path changes identity during normalization"
            );
            Ok(relative)
        })
        .collect::<Result<Vec<_>>>()?;
    let baseline = InitialBaseline {
        kind: BaselineKind::InitialLocalImport,
        checkout: root.to_path_buf(),
        common_git: common_git(root)?,
        revision: project.revision,
        branch: context::branch(root)?,
        issue,
        scopes,
        paths,
        tree: git(root, &["write-tree"], None)?.trim().to_owned(),
    };
    let snapshot = baseline_digest(&baseline)?;
    let facts = rules::pr_rules::InitialImportFacts {
        local: git(root, &["remote"], None)?.trim().is_empty(),
        unborn: source(root)? == Source::Unborn
            && git(root, &["for-each-ref", "--format=%(refname)"], None)?
                .trim()
                .is_empty(),
        compatible_history: project.selections.iter().all(|selection| {
            selection.issue == issue
                && selection.branch == baseline.branch
                && selection.source == Source::Unborn
        }) && project.remote_links.is_empty()
            && project.imports.is_empty(),
        open: project
            .issues
            .get(&issue)
            .is_some_and(|issue| issue.status == Status::Open),
        nonempty: !baseline.paths.is_empty() && !baseline.scopes.is_empty(),
        clean: git(root, &["diff", "--name-only", "--no-renames", "-z"], None)?.is_empty()
            && git(
                root,
                &["ls-files", "--others", "--exclude-standard", "-z"],
                None,
            )?
            .is_empty()
            && git(root, &["ls-files", "-v", "-z"], None)?
                .split('\0')
                .filter(|entry| !entry.is_empty())
                .all(|entry| entry.starts_with("H ")),
        scoped: baseline
            .paths
            .iter()
            .all(|path| context::scoped(&baseline.scopes, path)),
        unchanged: expected.is_none_or(|expected| snapshot == expected),
    };
    ensure!(
        rules::pr_rules::initial_import(facts) == rules::pr_rules::InitialImport::Ready,
        "initial import requires a local unborn project with compatible selection history, an open issue, a nonempty staged scoped baseline without unstaged or untracked extras, and an unchanged preview: {facts:?}"
    );
    Ok(baseline)
}

fn baseline_digest(baseline: &InitialBaseline) -> Result<String> {
    Ok(crate::build_inputs::hex(&Sha256::digest(
        serde_json::to_vec(baseline)?,
    )))
}

fn import_project(root: &Path, home: &Path) -> Result<Project> {
    let mut project = read(root, home)?;
    let mut selections = Vec::new();
    for selection in project.selections {
        if let Source::Commit { head, .. } = &selection.source {
            ensure!(
                rules::shape_rules::commit_id(head.as_bytes()),
                "invalid local selection source commit"
            );
            let head = head.to_ascii_lowercase();
            let object = git(
                root,
                &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
                Some(format!("{head}\n").as_bytes()),
            )?;
            if object.trim() == format!("{head} missing") {
                continue;
            }
            ensure!(
                object.trim() == format!("{head} commit"),
                "local selection source is not a commit"
            );
        }
        selections.push(selection);
    }
    project.selections = selections;
    Ok(project)
}

/// # Errors
/// Returns an error unless the existing staged artifacts qualify for an explicit initial import.
pub fn import_preview(root: &Path, issue: u64, scopes: &[PathBuf]) -> Result<ImportPreview> {
    let root = context::checkout(root)?;
    no_remote(&root)?;
    let home = home()?;
    let _lock = lock(&root, &home)?;
    let project = import_project(&root, &home)?;
    let baseline = initial_baseline(&root, &project, issue, scopes, None)?;
    Ok(ImportPreview {
        snapshot: baseline_digest(&baseline)?,
        tree: baseline.tree,
        paths: baseline.paths,
    })
}

/// # Errors
/// Returns an error if the reviewed baseline changed or attestation with its owner signing identity fails.
pub fn import_initial(root: &Path, issue: u64, scopes: &[PathBuf], expected: &str) -> Result<()> {
    let root = context::checkout(root)?;
    no_remote(&root)?;
    let home = home()?;
    let _lock = lock(&root, &home)?;
    let mut project = import_project(&root, &home)?;
    let baseline = initial_baseline(&root, &project, issue, scopes, Some(expected))?;
    let signed =
        crate::signed_git::create_initial(&root, &baseline.tree, &serde_json::to_vec(&baseline)?)?;
    ensure!(
        initial_baseline(
            &root,
            &import_project(&root, &home)?,
            issue,
            scopes,
            Some(expected)
        )? == baseline,
        "initial import baseline changed during signing"
    );
    let selection = Selection {
        issue,
        branch: baseline.branch.clone(),
        source: Source::Unborn,
        scopes: baseline.scopes.clone(),
    };
    let import = Import {
        baseline,
        object: signed.object,
        signed_commit: signed.raw,
    };
    let verified = crate::signed_git::authenticate(
        &root,
        &import.object,
        &import.signed_commit,
        &import_signers(&root, &home)?,
    )?;
    ensure!(
        verified.tree == import.baseline.tree && verified.parents.is_empty(),
        "initial import signature closure differs"
    );
    ensure!(
        initial_baseline(
            &root,
            &import_project(&root, &home)?,
            issue,
            scopes,
            Some(expected)
        )? == import.baseline,
        "initial import baseline changed before persistence"
    );
    project.selections = vec![selection];
    project.imports.push(import);
    save(&root, &home, &mut project, Persistence::Local)
}

fn check_path(root: &Path, project: &Project, path: &str) -> Result<()> {
    let mut closed = false;
    for selection in &project.selections {
        if selection.branch != context::branch(root)? || !context::scoped(&selection.scopes, path) {
            continue;
        }
        current(root, selection)?;
        let open = project
            .issues
            .get(&selection.issue)
            .context("selected local issue missing")?
            .status
            == Status::Open;
        closed |= !open;
        let verdict = rules::pr_rules::context_gate(rules::pr_rules::ContextFacts {
            required: true,
            present: true,
            authentic: true,
            destination: true,
            branch: true,
            source: true,
            open,
            scoped: true,
        });
        if verdict == rules::pr_rules::ContextGate::Accepted {
            return Ok(());
        }
    }
    ensure!(!closed, "local issue for path {path} is closed");
    anyhow::bail!("path {path} lacks a selected open local issue in the current branch")
}

/// # Errors
/// Returns an error when a supported edit lacks current local issue authority.
pub fn guard_for(root: &Path, paths: &[PathBuf], home: &Path) -> Result<()> {
    let root = context::checkout(root)?;
    no_remote(&root)?;
    let _lock = lock(&root, home)?;
    let mut project = read(&root, home)?;
    let observed = observe_source(&root, &mut project)?;
    if observed {
        save(&root, home, &mut project, Persistence::Local)?;
    }
    for path in paths {
        let absolute = context::edit_path(&root, path)?;
        let path = context::relative(absolute.strip_prefix(&root)?)?;
        check_path(&root, &project, &path)?;
    }
    Ok(())
}

/// # Errors
/// Returns an error for a local change outside selected open issues.
pub fn check(root: &Path, base: Option<&str>, staged: bool) -> Result<()> {
    let root = context::checkout(root)?;
    let paths = context::paths(&root, base, staged)?
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    guard_for(&root, &paths, &home()?)
}

/// # Errors
/// Returns an error if inspecting or transitioning local issues fails.
pub fn status(root: &Path, number: Option<u64>, status: Option<Status>) -> Result<Vec<Issue>> {
    let root = context::checkout(root)?;
    no_remote(&root)?;
    let home = home()?;
    let _lock = lock(&root, &home)?;
    let mut project = read(&root, &home)?;
    if let Some(status) = status {
        project
            .issues
            .get_mut(&number.context("local status transition needs --issue")?)
            .context("local issue missing")?
            .status = status;
        save(&root, &home, &mut project, Persistence::Local)?;
    }
    if let Some(number) = number {
        Ok(vec![
            project
                .issues
                .get(&number)
                .context("local issue missing")?
                .clone(),
        ])
    } else {
        Ok(project.issues.into_values().collect())
    }
}

/// # Errors
/// Returns an error unless every prior local dirty path retains actual scoped authority.
pub fn prepare_promotion(
    root: &Path,
    dirty: &BTreeMap<String, Option<String>>,
) -> Result<Promotion> {
    let home = home()?;
    let _lock = lock(root, &home)?;
    let mut project = read(root, &home)?;
    let observed = observe_source(root, &mut project)?;
    let branch = context::branch(root)?;
    let active = project
        .selections
        .iter()
        .filter(|selection| {
            selection.branch == branch
                && project
                    .issues
                    .get(&selection.issue)
                    .is_some_and(|issue| issue.status == Status::Open)
        })
        .collect::<Vec<_>>();
    ensure!(
        !active.is_empty(),
        "local promotion has no current open selected issue history"
    );
    for selection in active {
        current(root, selection)?;
    }
    for path in dirty.keys() {
        check_path(root, &project, path)?;
    }
    if observed {
        save(root, &home, &mut project, Persistence::Promotion)?;
    }
    Ok(Promotion {
        revision: project.revision,
        digest: crate::build_inputs::hex(&Sha256::digest(serde_json::to_vec(&project)?)),
    })
}

/// # Errors
/// Returns an error if the actual local history changed before recording its remote link.
pub fn record_promotion(
    root: &Path,
    expected: &Promotion,
    repository: String,
    object: String,
) -> Result<()> {
    let home = home()?;
    let _lock = lock(root, &home)?;
    let mut project = read(root, &home)?;
    ensure!(
        project.revision == expected.revision
            && crate::build_inputs::hex(&Sha256::digest(serde_json::to_vec(&project)?))
                == expected.digest,
        "local issue history changed during remote promotion"
    );
    project.remote_links.push(RemoteLink {
        local_revision: expected.revision,
        local_digest: expected.digest.clone(),
        repository,
        signed_context: object,
    });
    save(root, &home, &mut project, Persistence::Promotion)
}
