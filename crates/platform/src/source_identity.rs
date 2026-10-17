use anyhow::{Context, Result, ensure};
use rules::pr_rules::{RepositoryIdentities, SourceRelation, source_relation};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const GITHUB_HTTPS_PREFIX: &str = "https://github.com/";
const GITHUB_REMOTE_PREFIXES: [&str; 3] = [
    GITHUB_HTTPS_PREFIX,
    "git@github.com:",
    "ssh://git@github.com/",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    id: u64,
    repository: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    source: Identity,
    target: Identity,
    network: Identity,
}

impl Binding {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source.repository
    }

    #[must_use]
    pub fn target(&self) -> &str {
        &self.target.repository
    }

    #[must_use]
    pub fn same_repository(&self) -> bool {
        self.source == self.target
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Owner {
    pub login: String,
    pub id: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Network {
    pub id: u64,
    pub full_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Repository {
    #[serde(default)]
    pub id: u64,
    pub full_name: String,
    pub owner: Owner,
    pub fork: bool,
    #[serde(default)]
    pub source: Option<Network>,
    #[serde(default)]
    pub default_branch: Option<String>,
}

#[derive(Debug)]
pub struct Checked {
    root: PathBuf,
    binding: Binding,
}

#[derive(Debug)]
pub struct OriginChanged {
    expected: String,
    actual: String,
}

impl OriginChanged {
    #[must_use]
    pub fn expected(&self) -> &str {
        &self.expected
    }

    #[must_use]
    pub fn actual(&self) -> &str {
        &self.actual
    }
}

impl std::fmt::Display for OriginChanged {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "checkout source changed from {} to {} after repository verification",
            self.expected, self.actual
        )
    }
}

impl std::error::Error for OriginChanged {}

impl Checked {
    #[must_use]
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// # Errors
    /// Returns an error when the checkout origin changed after identity verification.
    pub fn require_current(&self) -> Result<()> {
        let actual = repository(&self.root)?;
        if !actual.eq_ignore_ascii_case(self.binding.source()) {
            return Err(OriginChanged {
                expected: self.binding.source().into(),
                actual,
            }
            .into());
        }
        Ok(())
    }
}

#[must_use]
pub fn github_slug(url: &str) -> Option<String> {
    let path = GITHUB_REMOTE_PREFIXES
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let valid = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    };
    (valid(owner) && valid(name)).then(|| format!("{owner}/{name}"))
}

/// # Errors
/// Returns an error if the actual origin doesn't identify one exact GitHub repository.
pub fn repository(root: &Path) -> Result<String> {
    github_slug(crate::signed_git::git(root, &["remote", "get-url", "origin"], None)?.trim())
        .context("checkout origin is not an exact GitHub repository")
}

fn identities(
    repository: Repository,
    requested: &str,
    network_required: bool,
) -> Result<(Identity, Identity)> {
    ensure!(
        repository.id != 0
            && repository.owner.id != 0
            && repository.full_name.eq_ignore_ascii_case(requested)
            && github_slug(&format!("{GITHUB_HTTPS_PREFIX}{}", repository.full_name)).is_some()
            && requested
                .split('/')
                .next()
                .is_some_and(|owner| owner.eq_ignore_ascii_case(&repository.owner.login)),
        "GitHub repository identity differs from the requested source or destination"
    );
    let identity = Identity {
        id: repository.id,
        repository: repository.full_name.to_ascii_lowercase(),
    };
    let network = match (repository.fork, repository.source) {
        (false, None) => identity.clone(),
        (true, None) if !network_required => identity.clone(),
        (true, Some(source)) => {
            ensure!(
                source.id != 0
                    && source.id != identity.id
                    && github_slug(&format!("{GITHUB_HTTPS_PREFIX}{}", source.full_name)).is_some(),
                "fork network identity is missing or inconsistent"
            );
            Identity {
                id: source.id,
                repository: source.full_name.to_ascii_lowercase(),
            }
        }
        _ => anyhow::bail!("fork network metadata is missing or inconsistent"),
    };
    Ok((identity, network))
}

/// # Errors
/// Returns an error for unknown, unrelated, or mismatched authenticated repository metadata.
pub fn capture(
    root: &Path,
    target: &str,
    mut read: impl FnMut(&str) -> Result<Repository>,
) -> Result<Checked> {
    ensure!(
        github_slug(&format!("{GITHUB_HTTPS_PREFIX}{target}")).as_deref() == Some(target),
        "PR destination does not name one exact GitHub repository"
    );
    let root = crate::issue_context::checkout(root)?;
    let source_name = repository(&root)?;
    let same = source_name.eq_ignore_ascii_case(target);
    let (target_identity, target_network) = identities(read(target)?, target, !same)?;
    let (source, source_network) = if same {
        (target_identity.clone(), target_network.clone())
    } else {
        identities(read(&source_name)?, &source_name, true)?
    };
    let relation = source_relation(RepositoryIdentities {
        source: source.id,
        target: target_identity.id,
        source_network: source_network.id,
        target_network: target_network.id,
    });
    ensure!(
        relation != SourceRelation::Refused
            && source_network == target_network
            && (source == target_identity) == (relation == SourceRelation::Same),
        "checkout source and PR destination do not share a verified repository network"
    );
    let checked = Checked {
        root,
        binding: Binding {
            source,
            target: target_identity,
            network: source_network,
        },
    };
    checked.require_current()?;
    Ok(checked)
}

/// # Errors
/// Returns an error when a qualified head names another source owner or an invalid Git branch.
pub fn head_branch(root: &Path, source: &Binding, head: &str) -> Result<String> {
    let branch = if let Some((owner, branch)) = head.split_once(':') {
        ensure!(
            source
                .source()
                .split('/')
                .next()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(owner))
                && !branch.contains(':'),
            "qualified PR head owner differs from the verified checkout source"
        );
        branch
    } else {
        head
    };
    crate::signed_git::git(root, &["check-ref-format", "--branch", branch], None)?;
    Ok(branch.to_owned())
}
