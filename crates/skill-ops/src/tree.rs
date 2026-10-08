//! The shared skill tree: each skill's portable metadata, its links, and its Claude symlink.

use anyhow::{Context, Result, ensure};
use pulldown_cmark::{Event, Parser, Tag};
use std::fs;
use std::path::{Path, PathBuf};
use yaml_rust2::YamlLoader;

use crate::canonical;

/// The Agent Skills specification's limits on a skill's name and description.
const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_CHARACTERS: usize = 1024;

#[derive(Debug, PartialEq)]
pub struct Metadata {
    pub name: String,
    pub description: String,
}

/// The portable name and description of a skill file in `directory`.
///
/// # Errors
///
/// Returns an error when the frontmatter lacks one YAML mapping, or it has a non-portable field.
/// Also returns an error when the name fails validation or differs from `directory`, the description has no text or exceeds 1024 characters, or the body has no text.
pub fn metadata(text: &str, directory: &str) -> Result<Metadata> {
    let (header, body) = split_frontmatter(text)?;
    let documents = YamlLoader::load_from_str(header)?;
    ensure!(
        documents.len() == 1,
        "frontmatter must be one YAML document"
    );
    let document = &documents[0];
    let fields = document
        .as_hash()
        .context("frontmatter must be a mapping")?;
    for key in fields.keys() {
        ensure!(
            matches!(
                key.as_str(),
                Some("name" | "description" | "license" | "compatibility" | "metadata")
            ),
            "non-portable frontmatter field: {key:?}"
        );
    }
    let name = document["name"].as_str().context("name must be a string")?;
    ensure!(name == directory, "name must match directory {directory}");
    ensure!(
        !name.is_empty()
            && name.len() <= MAX_NAME_LENGTH
            && name != "synced"
            && name.split('-').all(|part| !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())),
        "invalid portable skill name: {name}"
    );
    let description = document["description"]
        .as_str()
        .context("description must be a string")?;
    ensure!(
        !description.trim().is_empty() && description.chars().count() <= MAX_DESCRIPTION_CHARACTERS,
        "description must contain 1 to {MAX_DESCRIPTION_CHARACTERS} characters"
    );
    ensure!(!body.trim().is_empty(), "skill body must not be empty");
    Ok(Metadata {
        name: name.into(),
        description: description.into(),
    })
}

/// The YAML frontmatter and the body of a Markdown document.
///
/// # Errors
///
/// Returns an error when `text` doesn't start with a `---` line or the frontmatter has no closing `---` line.
pub fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let content = text
        .strip_prefix("---\n")
        .context("missing YAML frontmatter")?;
    let (header, body) = content
        .split_once("\n---\n")
        .context("unterminated YAML frontmatter")?;
    Ok((header, body))
}

/// Refuses a relative link or image in `file` that points nowhere or resolves outside `root`.
///
/// # Errors
///
/// Returns an error when resolving `root` fails, a relative link target doesn't exist, or it resolves outside `root`.
pub fn validate_links(text: &str, file: &Path, root: &Path) -> Result<()> {
    let root = canonical(root)?;
    for event in Parser::new(text) {
        if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = event {
            let link = dest_url.split(['#', '?']).next().context("missing link")?;
            if link.is_empty()
                || link.starts_with("https://")
                || link.starts_with("http://")
                || link.starts_with("mailto:")
            {
                continue;
            }
            let path = file.parent().context("missing parent")?.join(link);
            let resolved = canonical(&path)
                .with_context(|| format!("broken resource {}: {link}", file.display()))?;
            ensure!(
                resolved.starts_with(&root),
                "resource escapes skill tree: {link}"
            );
        }
    }
    Ok(())
}

/// Each shared skill's `SKILL.md` path and metadata, sorted by name.
///
/// # Errors
///
/// Returns an error when reading the skill tree fails, or it holds no skill or an entry other than a directory with a UTF-8 name.
/// It also returns an error when a skill has invalid metadata or links.
pub fn skills(root: &Path) -> Result<Vec<(std::path::PathBuf, Metadata)>> {
    let tree = root.join("home/dot_agents/skills");
    let mut result = Vec::new();
    for entry in fs::read_dir(&tree)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_dir(),
            "canonical skill must be a directory"
        );
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF-8 skill name"))?;
        let path = entry.path().join("SKILL.md");
        let text = fs::read_to_string(&path)?;
        let parsed = metadata(&text, &name).with_context(|| path.display().to_string())?;
        validate_links(&text, &path, &tree)?;
        result.push((path, parsed));
    }
    result.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    ensure!(!result.is_empty(), "no skills found");
    Ok(result)
}

/// Checks the shared skills and their Claude symlinks, and returns the number of skills.
///
/// # Errors
///
/// Returns an error when a skill fails validation, a skill's symlink goes missing or points elsewhere, or a symlink names no shared skill.
pub fn validate_tree(root: &Path) -> Result<usize> {
    let entries = skills(root)?;
    for (_, skill) in &entries {
        let symlink = root
            .join("home/dot_claude/skills")
            .join(format!("symlink_{}", skill.name));
        let target = fs::read_to_string(&symlink)
            .with_context(|| format!("missing symlink {}", symlink.display()))?;
        ensure!(
            target.trim() == format!("../../.agents/skills/{}", skill.name),
            "incorrect symlink {}",
            symlink.display()
        );
    }
    for entry in fs::read_dir(root.join("home/dot_claude/skills"))? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF-8 symlink"))?;
        let name = name
            .strip_prefix("symlink_")
            .context("independent Claude skill must move to canonical tree")?;
        ensure!(
            entries.iter().any(|(_, skill)| skill.name == name),
            "symlink has no canonical skill: {name}"
        );
    }
    Ok(entries.len())
}

/// Copies the files the skill checks read into the new directory `output` and validates the copy.
///
/// # Errors
///
/// Returns an error when either skill tree fails validation, an input names a symlink or anything other than a regular file, or `output` already exists or creating it fails.
/// Also returns an error when reading or writing a file fails, or a source file changes during the copy.
pub fn export_checks(root: &Path, output: &Path) -> Result<()> {
    validate_tree(root)?;
    let mut files = Vec::new();
    for relative in [
        "home/dot_agents/skills",
        "home/dot_claude/skills",
        "xtask",
        "crates",
        "Cargo.toml",
        "Cargo.lock",
        "clippy.toml",
        "mise.toml",
        ".chezmoiroot",
        "home/.chezmoiignore",
        "lefthook.yml",
        ".github/workflows/ci.yml",
        "home/dot_config/skill-ops/policy.json",
        "home/dot_config/opencode/plugins/skill-ops.ts",
        "home/dot_config/opencode/plugins/line-endings.ts",
        "docs/skills/decisions",
        "package.json",
        "bun.lock",
        "tsconfig.json",
    ] {
        collect(root, Path::new(relative), &mut files)?;
    }
    fs::create_dir(output).context("create fresh check snapshot directory")?;
    for (relative, bytes) in files {
        let destination = output.join(&relative);
        fs::create_dir_all(destination.parent().context("missing destination parent")?)?;
        fs::write(&destination, &bytes)?;
        ensure!(
            fs::read(root.join(&relative))? == bytes,
            "source changed during snapshot: {}",
            relative.display()
        );
    }
    validate_tree(output)?;
    Ok(())
}

/// Every file of the shared skill tree and its Claude symlinks.
///
/// # Errors
///
/// Returns an error when the skill tree fails validation, holds a symlink or an irregular file, or reading it fails.
pub fn skill_source_paths(root: &Path) -> Result<Vec<PathBuf>> {
    validate_tree(root)?;
    let mut files = Vec::new();
    for relative in ["home/dot_agents/skills", "home/dot_claude/skills"] {
        collect(root, Path::new(relative), &mut files)?;
    }
    Ok(files.into_iter().map(|(path, _)| root.join(path)).collect())
}

pub(crate) fn collect(
    root: &Path,
    relative: &Path,
    files: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<()> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        !metadata.is_symlink(),
        "snapshot cannot contain a symlink: {}",
        path.display()
    );
    if metadata.is_dir() {
        let mut entries = fs::read_dir(&path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if entry.file_name() != "target" {
                collect(root, &relative.join(entry.file_name()), files)?;
            }
        }
    } else {
        ensure!(metadata.is_file(), "snapshot requires regular files");
        files.push((relative.to_path_buf(), fs::read(path)?));
    }
    Ok(())
}
