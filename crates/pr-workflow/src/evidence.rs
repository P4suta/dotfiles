//! Owner-local workflow evidence: signed importance assessments, the assessment each PR last verified, and pause exclusions.
//! No published document or tracked file carries any of it.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

const STATE: &str = ".local/state/pr-workflow";
const ASSESSMENTS: &str = "assessments";
const OBJECTS: &str = "objects";
const PAUSES: &str = "pauses";
const MAX_PACKET_BYTES: u64 = 48 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Packet {
    pub object: String,
    pub raw: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Verified {
    object: String,
}

pub(super) fn home() -> Result<PathBuf> {
    platform::home::home().context("the owner home for local workflow evidence is unavailable")
}

fn plain(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// The directory of `repository` for the evidence `kind`, such as `assessments/p4suta__dotfiles`.
pub(super) fn directory(home: &Path, kind: &str, repository: &str) -> Result<PathBuf> {
    let (owner, name) = repository
        .split_once('/')
        .context("local evidence needs an OWNER/REPO repository")?;
    ensure!(
        plain(owner) && plain(name),
        "local evidence needs a plain OWNER/REPO repository"
    );
    Ok(home.join(STATE).join(kind).join(format!(
        "{}__{}",
        owner.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    )))
}

fn hexadecimal(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "local evidence keys are hexadecimal object names"
    );
    Ok(value)
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("local evidence has no parent")?;
    fs::create_dir_all(parent)?;
    ensure!(
        !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()),
        "local evidence files can't be symlinks"
    );
    let mut candidate = tempfile::NamedTempFile::new_in(parent)?;
    candidate.as_file_mut().write_all(bytes)?;
    candidate.as_file().sync_all()?;
    candidate.persist(path)?;
    Ok(())
}

fn read_packet(path: &Path) -> Result<Packet> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "a local importance assessment must be a regular file"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_PACKET_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_PACKET_BYTES,
        "the local importance assessment exceeds its bounded packet"
    );
    serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("the local importance assessment packet is malformed"))
}

/// Stores the signed assessment of `head` and `diff_sha256` in `repository` under `home`, replacing an earlier one for the same diff.
///
/// # Errors
/// Returns an error for a repository or key that can't name a local file, or when writing fails.
pub fn store_assessment(
    home: &Path,
    repository: &str,
    head: &str,
    diff_sha256: &str,
    object: &str,
    raw: &str,
) -> Result<PathBuf> {
    let directory = directory(home, ASSESSMENTS, repository)?;
    let bytes = serde_json::to_vec(&Packet {
        object: object.into(),
        raw: raw.into(),
    })?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_PACKET_BYTES,
        "the signed assessment exceeds its bounded packet"
    );
    let archived = directory
        .join(OBJECTS)
        .join(format!("{}.json", hexadecimal(object)?));
    if !archived.try_exists()? {
        write(&archived, &bytes)?;
    }
    let current = directory.join(format!(
        "{}-{}.json",
        hexadecimal(head)?,
        hexadecimal(diff_sha256)?
    ));
    write(&current, &bytes)?;
    Ok(current)
}

/// The current signed assessment of `head` and `diff_sha256` in `repository`.
pub(crate) fn assessment(repository: &str, head: &str, diff_sha256: &str) -> Result<Packet> {
    let path = directory(&home()?, ASSESSMENTS, repository)?.join(format!(
        "{}-{}.json",
        hexadecimal(head)?,
        hexadecimal(diff_sha256)?
    ));
    ensure!(
        path.try_exists()?,
        "PR importance assessment is missing on this machine; run pr-workflow assess for this exact diff"
    );
    read_packet(&path)
}

/// The archived signed assessment named `object` in `repository`.
pub(crate) fn object(repository: &str, object: &str) -> Result<Packet> {
    read_packet(
        &directory(&home()?, ASSESSMENTS, repository)?
            .join(OBJECTS)
            .join(format!("{}.json", hexadecimal(object)?)),
    )
}

fn verified_path(repository: &str, pr: u64) -> Result<PathBuf> {
    Ok(directory(&home()?, ASSESSMENTS, repository)?.join(format!("pr-{pr}.json")))
}

/// Records `object` as the assessment that `pr` last verified.
pub(crate) fn remember(repository: &str, pr: u64, object: &str) -> Result<()> {
    write(
        &verified_path(repository, pr)?,
        &serde_json::to_vec(&Verified {
            object: hexadecimal(object)?.into(),
        })?,
    )
}

/// The assessment that `pr` last verified on this machine.
pub(crate) fn remembered(repository: &str, pr: u64) -> Result<Option<String>> {
    let path = verified_path(repository, pr)?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let verified: Verified = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|_| anyhow::anyhow!("the local record of a verified assessment is malformed"))?;
    Ok(Some(hexadecimal(&verified.object)?.to_owned()))
}

fn pause_path(home: &Path, repository: &str, pr: u64) -> Result<PathBuf> {
    Ok(directory(home, PAUSES, repository)?.join(format!("pr-{pr}")))
}

/// Whether the owner's pause, rather than a standing choice, excludes the review of `pr`.
pub(crate) fn paused(repository: &str, pr: u64) -> Result<bool> {
    Ok(pause_path(&home()?, repository, pr)?.try_exists()?)
}

/// Records that the owner's pause excludes the review of `pr`, which stays owed after resumption.
pub(crate) fn record_pause(repository: &str, pr: u64) -> Result<()> {
    write(&pause_path(&home()?, repository, pr)?, b"")
}

/// Forgets the pause exclusion of `pr`.
pub(crate) fn clear_pause(repository: &str, pr: u64) -> Result<()> {
    platform::cleanup::remove_file(&pause_path(&home()?, repository, pr)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_directories_refuse_traversal_and_fold_case() -> Result<()> {
        let home = Path::new("/home/owner");
        assert_eq!(
            directory(home, ASSESSMENTS, "P4suta/Dotfiles")?,
            home.join(STATE).join(ASSESSMENTS).join("p4suta__dotfiles")
        );
        for repository in ["P4suta", "../x", "a/..", "a/b/c", "a/", "/b", "a b/c"] {
            assert!(
                directory(home, ASSESSMENTS, repository).is_err(),
                "{repository}"
            );
        }
        assert!(hexadecimal("../abc").is_err());
        assert!(hexadecimal("").is_err());
        Ok(())
    }

    #[test]
    fn a_stored_assessment_reads_back_by_diff_and_by_object() -> Result<()> {
        let home = tempfile::tempdir()?;
        let path = store_assessment(
            home.path(),
            "P4suta/dotfiles",
            "ab12",
            "cd34",
            "ef56",
            "raw",
        )?;
        let packet = read_packet(&path)?;
        assert_eq!(
            (packet.object.as_str(), packet.raw.as_str()),
            ("ef56", "raw")
        );
        let archived = read_packet(
            &directory(home.path(), ASSESSMENTS, "P4suta/dotfiles")?
                .join(OBJECTS)
                .join("ef56.json"),
        )?;
        assert_eq!(archived.raw, "raw");
        store_assessment(
            home.path(),
            "P4suta/dotfiles",
            "ab12",
            "cd34",
            "0789",
            "next",
        )?;
        assert_eq!(read_packet(&path)?.raw, "next");
        Ok(())
    }

    #[test]
    fn a_pause_record_belongs_to_one_pr_and_clears() -> Result<()> {
        let home = tempfile::tempdir()?;
        let first = pause_path(home.path(), "P4suta/dotfiles", 17)?;
        let other = pause_path(home.path(), "P4suta/dotfiles", 18)?;
        assert!(!first.try_exists()?);
        write(&first, b"")?;
        assert!(first.try_exists()? && !other.try_exists()?);
        platform::cleanup::remove_file(&first);
        platform::cleanup::remove_file(&first);
        assert!(!first.try_exists()?);
        Ok(())
    }
}
