use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Stdio;

const DIFF_LIMIT: usize = 32 * 1024 * 1024;
const OBJECT_LENGTHS: [usize; 2] = [40, 64];
const GITHUB_ORIGIN_PREFIXES: [&str; 3] = [
    "https://github.com/",
    "ssh://git@github.com/",
    "git@github.com:",
];
const REPOSITORY_COMPONENTS: usize = 2;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewBinding {
    pub repository: String,
    pub base: String,
    pub head: String,
    pub diff_sha256: String,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    platform::build_inputs::hex(&Sha256::digest(bytes))
}

pub(crate) fn oid(value: &str) -> bool {
    OBJECT_LENGTHS.contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(crate) fn git(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Tool::Git
        .command()
        .current_dir(root)
        .args(["--no-pager", "--no-replace-objects"])
        .args(arguments)
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "capture actual committed review scope"
    );
    ensure!(
        output.stdout.len() <= DIFF_LIMIT,
        "committed review scope exceeds its byte bound"
    );
    Ok(output.stdout)
}

pub(crate) fn text(root: &Path, arguments: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git(root, arguments)?)?.trim().to_owned())
}

pub(crate) fn repository(root: &Path) -> Result<String> {
    let origin = text(root, &["remote", "get-url", "origin"])?;
    let relative = GITHUB_ORIGIN_PREFIXES
        .into_iter()
        .find_map(|prefix| origin.strip_prefix(prefix))
        .context("committed review requires an actual GitHub origin")?;
    let repository = relative.strip_suffix(".git").unwrap_or(relative);
    let parts = repository.split('/').collect::<Vec<_>>();
    ensure!(
        parts.len() == REPOSITORY_COMPONENTS
            && parts.iter().all(|part| {
                !part.is_empty()
                    && *part != "."
                    && *part != ".."
                    && part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
            }),
        "committed review origin has an invalid repository identity"
    );
    Ok(repository.to_owned())
}

impl ReviewBinding {
    /// Derives the exact committed diff from actual local objects and the checked origin.
    ///
    /// # Errors
    /// Rejects missing objects, another repository, a nonancestor base, or an oversize diff.
    pub fn capture(root: &Path, expected_repository: &str, base: &str, head: &str) -> Result<Self> {
        ensure!(
            oid(base) && oid(head) && base.len() == head.len(),
            "review scope requires full commit identities"
        );
        ensure!(
            repository(root)?.eq_ignore_ascii_case(expected_repository),
            "review repository differs from actual origin"
        );
        for object in [base, head] {
            ensure!(
                text(
                    root,
                    &["rev-parse", "--verify", &format!("{object}^{{commit}}")]
                )? == object,
                "review object differs from its full commit identity"
            );
        }
        ensure!(
            text(root, &["merge-base", base, head])? == base,
            "review base must be the selected merge base"
        );
        let diff = git(
            root,
            &[
                "diff",
                "--binary",
                "--full-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                base,
                head,
                "--",
            ],
        )?;
        Ok(Self {
            repository: expected_repository.into(),
            base: base.into(),
            head: head.into(),
            diff_sha256: digest(&diff),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub(crate) binding: ReviewBinding,
    pub(crate) tree: String,
    pub(crate) working_directory: String,
    pub(crate) branch: String,
    pub(crate) paths: Vec<String>,
}

pub(crate) fn paths(root: &Path, base: &str, head: &str) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for tree in [base, head] {
        let files = git(root, &["ls-tree", "-r", "--name-only", "-z", tree])?;
        paths.extend(
            std::str::from_utf8(&files)?
                .split('\0')
                .filter(|path| !path.is_empty())
                .map(String::from),
        );
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub(crate) fn snapshot(root: &Path, base: &str) -> Result<Snapshot> {
    ensure!(
        git(
            root,
            &["status", "--porcelain=v1", "--untracked-files=normal"]
        )?
        .is_empty(),
        "committed receipt requires a clean, unchanged checkout"
    );
    let head = text(root, &["rev-parse", "--verify", "HEAD"])?;
    let actual_repository = repository(root)?;
    let binding = ReviewBinding::capture(root, &actual_repository, base, &head)?;
    ensure!(
        binding.base != binding.head,
        "an empty committed scope cannot supply review evidence"
    );
    Ok(Snapshot {
        tree: text(root, &["rev-parse", &format!("{head}^{{tree}}")])?,
        binding,
        working_directory: platform::paths::canonical(root)?
            .to_str()
            .context("review checkout UTF-8")?
            .into(),
        branch: text(root, &["rev-parse", "--abbrev-ref", "HEAD"])?,
        paths: paths(root, base, &head)?,
    })
}
