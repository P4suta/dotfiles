use super::{
    Destination, Github, Target, destination, inspect_issue, require_issue, workflow_policy,
};
use anyhow::{Context, Result, ensure};
use platform::issue_context::{self as state, Receipt};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

const KEY_PAGE_LIMIT: usize = 100;
const SSH_KEY_MINIMUM_FIELDS: usize = 2;

#[derive(Deserialize)]
struct SigningKey {
    id: u64,
    key: String,
}

fn signers(github: &Github, repository: &str) -> Result<tempfile::NamedTempFile> {
    let owner = repository
        .split('/')
        .next()
        .context("repository owner missing")?;
    signers_for_login(github, owner)
}

pub(super) fn signers_for_login(github: &Github, owner: &str) -> Result<tempfile::NamedTempFile> {
    let keys: Vec<SigningKey> = github.read(&format!(
        "users/{owner}/ssh_signing_keys?per_page={KEY_PAGE_LIMIT}"
    ))?;
    ensure!(
        !keys.is_empty() && keys.len() < KEY_PAGE_LIMIT,
        "owner SSH signing keys are missing or incomplete"
    );
    let mut file = tempfile::NamedTempFile::new()?;
    for key in keys {
        ensure!(
            key.id != 0
                && !key.key.chars().any(char::is_control)
                && key.key.split_whitespace().count() >= SSH_KEY_MINIMUM_FIELDS,
            "malformed owner signing key"
        );
        writeln!(file, "* namespaces=\"git\" {}", key.key)?;
    }
    file.as_file().sync_all()?;
    Ok(file)
}

pub(super) fn start(
    github: &Github,
    target: &Target,
    destination: Destination,
    paths: &[PathBuf],
    adopt_existing: bool,
    promote_local: bool,
) -> Result<()> {
    let root = state::checkout(&std::env::current_dir()?)?;
    let binding = state::binding(&root)?;
    let identity = super::source_identity(github, &root, &target.repo)?;
    let required = destination.personal && !destination.fork;
    let mut context = state::snapshot(
        &root,
        state::Destination {
            repository: target.repo.clone(),
            repository_id: destination.repository_id,
            owner_id: destination.owner_id,
            required,
        },
        target.issue,
        paths,
    )?;
    let promotion = if promote_local {
        ensure!(
            required && !adopt_existing,
            "local promotion requires a personal remote issue and cannot use baseline adoption"
        );
        Some(platform::local_issues::prepare_promotion(
            &root,
            &context.initial_dirty,
        )?)
    } else {
        state::authorize_initial(&root, &mut context, adopt_existing)?;
        None
    };
    let receipt = if required {
        let signers = signers(github, &target.repo)?;
        let receipt = state::signed(&root, context)?;
        state::authenticate(&root, &receipt, signers.path())?;
        receipt
    } else {
        Receipt {
            context,
            object: String::new(),
            signed_commit: String::new(),
        }
    };
    let mut final_snapshot = state::snapshot(
        &root,
        receipt.context.destination.clone(),
        target.issue,
        paths,
    )?;
    final_snapshot.adopted = receipt.context.adopted;
    ensure!(
        final_snapshot == receipt.context,
        "source changed while the issue context was being signed"
    );
    super::require_current_source(&identity)?;
    println!(
        "Initial source: {} previously authorized dirty path(s); owner migration: {}; no retroactive start-order evidence",
        receipt.context.initial_dirty.len(),
        receipt.context.adopted
    );
    let object = receipt.object.clone();
    state::persist(&root, receipt, &binding)?;
    if let Some(promotion) = promotion {
        platform::local_issues::record_promotion(&root, &promotion, target.repo.clone(), object)?;
        println!(
            "Actual local issue history linked as prior work; remote authorization begins at this checked start"
        );
    }
    Ok(())
}

fn identity(root: &Path) -> Result<(String, String)> {
    let url = state::git(root, &["remote", "get-url", "origin"], None)?;
    Ok((
        super::github_slug(url.trim())
            .context("issue-context origin is not an exact GitHub repository")?,
        state::branch(root)?,
    ))
}

fn require_registered_paths(scopes: &[String], paths: &[String]) -> Result<()> {
    for path in paths {
        let verdict = rules::pr_rules::context_gate(rules::pr_rules::ContextFacts {
            required: true,
            present: true,
            authentic: true,
            destination: true,
            branch: true,
            source: true,
            open: true,
            scoped: state::scoped(scopes, path),
        });
        ensure!(
            verdict == rules::pr_rules::ContextGate::Accepted,
            "issue context does not register changed path {path}"
        );
    }
    Ok(())
}

/// The owner-local contexts of `branch`, which any worktree of the repository may have registered.
fn selected_context(root: &Path, branch: &str) -> Result<Option<state::Manifest>> {
    let Some(mut manifest) = state::optional_local(root)? else {
        return Ok(None);
    };
    manifest
        .receipts
        .retain(|receipt| receipt.context.branch == branch);
    Ok((!manifest.receipts.is_empty()).then_some(manifest))
}

fn selected_repository(manifest: &state::Manifest) -> Result<String> {
    let repository = &manifest
        .receipts
        .first()
        .context("issue context missing")?
        .context
        .destination
        .repository;
    ensure!(
        manifest.receipts.iter().all(|receipt| receipt
            .context
            .destination
            .repository
            .eq_ignore_ascii_case(repository)),
        "active issue contexts name different destinations"
    );
    Ok(repository.clone())
}

struct ChangeDestination {
    repository: String,
    branch: String,
    destination: Destination,
    selected: Option<state::Manifest>,
}

fn change_destination(root: &Path, github: &Github) -> Result<ChangeDestination> {
    let (mut repository, branch) = identity(root)?;
    let selected = selected_context(root, &branch)?;
    if let Some(manifest) = &selected {
        repository = selected_repository(manifest)?;
    }
    let destination = destination(github, &repository)?;
    super::source_identity(github, root, &repository)?;
    if (!destination.personal || destination.fork)
        && let Some(manifest) = &selected
    {
        ensure!(
            manifest
                .receipts
                .iter()
                .all(|receipt| !receipt.context.destination.required),
            "required issue context cannot be downgraded to external or fork policy"
        );
        let keys = if manifest
            .receipts
            .iter()
            .any(|receipt| !receipt.object.is_empty())
        {
            Some(signers(github, &repository)?)
        } else {
            None
        };
        for receipt in &manifest.receipts {
            ensure!(
                receipt.context.destination.repository_id == destination.repository_id
                    && receipt.context.destination.owner_id == destination.owner_id,
                "issue-context numeric destination differs"
            );
            state::current(root, &receipt.context, &repository, &receipt.context.branch)?;
            if let Some(keys) = &keys {
                state::authenticate(root, receipt, keys.path())?;
            }
        }
    }
    Ok(ChangeDestination {
        repository,
        branch,
        destination,
        selected,
    })
}

struct RegisteredChanges {
    scopes: Vec<String>,
    prior: BTreeSet<String>,
}

fn registered_context(
    root: &Path,
    github: &Github,
    repository: &str,
    destination: Destination,
    branches: &BTreeSet<String>,
    manifest: &state::Manifest,
) -> Result<RegisteredChanges> {
    let signers = signers(github, repository)?;
    let mut scopes = Vec::new();
    let mut prior = BTreeSet::new();
    for receipt in &manifest.receipts {
        let context = &receipt.context;
        state::authenticate(root, receipt, signers.path())?;
        ensure!(
            context.destination.required
                && context.destination.repository_id == destination.repository_id
                && context.destination.owner_id == destination.owner_id,
            "issue-context numeric destination differs"
        );
        ensure!(
            branches.contains(&context.branch),
            "issue-context branch is not the actual change branch"
        );
        state::current(root, context, repository, &context.branch)?;
        let target = Target {
            repo: repository.into(),
            generation: super::Generation::Local,
            title_policy: None,
            issue: context.issue,
            cli_receipt: None,
            prior_cli_receipt: None,
        };
        let gate = inspect_issue(
            github,
            &target,
            destination,
            None,
            "pr-workflow change-check",
        )?;
        require_issue(gate, &target)?;
        scopes.extend(context.scopes.iter().cloned());
        prior.extend(context.initial_dirty.keys().cloned());
    }
    Ok(RegisteredChanges { scopes, prior })
}

pub(super) fn check(root: &Path, base: Option<&str>, staged: bool) -> Result<()> {
    let root = state::checkout(root)?;
    if platform::local_issues::active(&root)? {
        platform::local_issues::check(&root, base, staged)?;
        println!("Local issue context accepted; project verification remains required");
        return Ok(());
    }
    let paths = state::paths(&root, base, staged)?;
    if paths.is_empty() {
        println!("Issue context: no implementation change");
        return Ok(());
    }
    let github = Github {
        policy: workflow_policy()?,
        login: String::new(),
        repositories: std::cell::RefCell::default(),
    };
    let ChangeDestination {
        repository,
        branch,
        destination,
        selected,
    } = change_destination(&root, &github)?;
    if !destination.personal || destination.fork {
        println!("Issue context follows the external or fork destination policy");
        return Ok(());
    }
    let manifest = selected.context(
        "checked issue context missing for the current branch; run pr-workflow start --repo OWNER/REPO --issue NUMBER --path PATH in any worktree of this repository",
    )?;
    let branches = BTreeSet::from([branch]);
    let RegisteredChanges { scopes, prior } = registered_context(
        &root,
        &github,
        &repository,
        destination,
        &branches,
        &manifest,
    )?;
    require_registered_paths(&scopes, &paths)?;
    println!(
        "Issue context: {} changed path(s) bound to live issues; {} initial dirty path(s) remain prior work without a start-order claim",
        paths.len(),
        paths.iter().filter(|path| prior.contains(*path)).count()
    );
    Ok(())
}
