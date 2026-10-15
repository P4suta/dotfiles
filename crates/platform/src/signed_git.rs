//! Signed Git objects whose verifier receives trusted signers outside the object.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

const SSH_FORMAT: &str = "gpg.format=ssh";
const SSH_VERIFIER: &str = "gpg.ssh.program=ssh-keygen";

#[derive(Debug)]
pub struct Commit {
    pub object: String,
    pub raw: String,
}

#[derive(Debug)]
pub struct Verified {
    pub payload: String,
    pub tree: String,
    pub parents: Vec<String>,
}

/// # Errors
/// Returns an error when Git fails or returns non-UTF-8 output.
pub fn git(root: &Path, arguments: &[&str], input: Option<&[u8]>) -> Result<String> {
    output(root, arguments, input, true)
}

/// # Errors
/// Returns an error if Git can't read the actual transaction index.
pub fn index_git(root: &Path, arguments: &[&str]) -> Result<String> {
    output(root, arguments, None, false)
}

fn output(
    root: &Path,
    arguments: &[&str],
    input: Option<&[u8]>,
    isolate_index: bool,
) -> Result<String> {
    let mut command = Tool::Git.command();
    command
        .current_dir(root)
        .arg("--no-replace-objects")
        .args(arguments);
    if !isolate_index && let Some(index) = std::env::var_os("GIT_INDEX_FILE") {
        command.env("GIT_INDEX_FILE", index);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = command.spawn().context("start signed-evidence Git")?;
    if let Some(input) = input {
        let written = child
            .stdin
            .take()
            .context("signed-evidence Git stdin missing")
            .and_then(|mut pipe| pipe.write_all(input).map_err(Into::into));
        if let Err(error) = written {
            crate::cleanup::report(child.kill(), "stop signed-evidence Git");
            crate::cleanup::report(child.wait(), "wait for signed-evidence Git");
            return Err(error);
        }
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "signed-evidence Git failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).context("signed-evidence Git returned non-UTF-8 data")
}

/// # Errors
/// Returns an error if the normal owner signing environment can't create the object.
pub fn create(root: &Path, head: &str, tree: &str, payload: &[u8]) -> Result<Commit> {
    let object = git(
        root,
        &["commit-tree", "-S", tree, "-p", head],
        Some(payload),
    )?
    .trim()
    .to_owned();
    let raw = git(root, &["cat-file", "commit", &object], None)?;
    Ok(Commit { object, raw })
}

/// # Errors
/// Returns an error for an invalid object identity, signature, or payload.
pub fn authenticate(root: &Path, object: &str, raw: &str, signers: &Path) -> Result<Verified> {
    let format = git(root, &["rev-parse", "--show-object-format"], None)?;
    ensure!(
        matches!(format.trim(), "sha1" | "sha256"),
        "unsupported signed Git object format"
    );
    let temporary = tempfile::tempdir()?;
    let database = temporary.path();
    git(
        database,
        &[
            "init",
            "--bare",
            "--quiet",
            &format!("--object-format={}", format.trim()),
        ],
        None,
    )?;
    let actual = git(
        database,
        &["hash-object", "-w", "-t", "commit", "--stdin"],
        Some(raw.as_bytes()),
    )?;
    ensure!(actual.trim() == object, "signed object identity differs");
    git(
        database,
        &[
            "-c",
            SSH_FORMAT,
            "-c",
            SSH_VERIFIER,
            "-c",
            &format!("gpg.ssh.allowedSignersFile={}", signers.display()),
            "verify-commit",
            object,
        ],
        None,
    )?;
    project(raw, object.len())
}

fn project(raw: &str, object_length: usize) -> Result<Verified> {
    let (header, payload) = raw
        .split_once("\n\n")
        .context("signed commit body missing")?;
    let valid = |object: &str| {
        object.len() == object_length
            && object
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    };
    let mut fields = header.split('\n');
    let tree = fields
        .next()
        .and_then(|field| field.strip_prefix("tree "))
        .filter(|tree| valid(tree))
        .context("signed source tree missing or malformed")?;
    let mut parents = Vec::new();
    for field in fields {
        ensure!(!field.starts_with("tree "), "duplicate signed source tree");
        if let Some(parent) = field.strip_prefix("parent ") {
            ensure!(valid(parent), "malformed signed source parent");
            parents.push(parent.to_owned());
        }
    }
    Ok(Verified {
        payload: payload.to_owned(),
        tree: tree.to_owned(),
        parents,
    })
}

#[cfg(test)]
mod tests {
    use super::project;

    #[test]
    fn authenticated_commit_projection_needs_no_parent_objects() {
        for length in [40, 64] {
            let tree = "a".repeat(length);
            let parent = "b".repeat(length);
            let raw = format!(
                "tree {tree}\nparent {parent}\nauthor Fixture <fixture@example.com> 1 +0000\ngpgsig signature\n parent forged\n \n more signature\n\n{{\"value\":1}}\n"
            );
            let verified = project(&raw, length).unwrap();
            assert_eq!(verified.tree, tree);
            assert_eq!(verified.parents, [parent]);
            assert_eq!(verified.payload, "{\"value\":1}\n");
        }
    }

    #[test]
    fn malformed_signed_closure_is_refused() {
        let tree = "a".repeat(40);
        for raw in [
            "author Fixture\n\nbody".to_owned(),
            format!("tree {tree}\ntree {tree}\n\nbody"),
            format!("tree {tree}\nparent missing\n\nbody"),
            format!("tree {tree}\nparent {}\n\nbody", "b".repeat(64)),
            format!("tree {}\n\nbody", "A".repeat(40)),
            format!("tree {tree}\r\n\nbody"),
            format!("tree {tree}\nbody"),
        ] {
            assert!(project(&raw, 40).is_err(), "{raw}");
        }
    }
}
