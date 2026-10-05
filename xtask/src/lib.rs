use anyhow::{Context, Result, ensure};
use pulldown_cmark::{Event, Parser, Tag};
use std::fs;
use std::path::{Path, PathBuf};
use yaml_rust2::YamlLoader;

pub mod agent_memory;
pub mod desktop;
pub mod hooks;
pub mod pr_rules;
pub mod pr_workflow;
pub mod profile_rules;
pub mod profiles;
pub mod quality;
pub mod rehearsal;
pub mod review_guard;
pub mod review_rules;
pub mod runtime;
pub mod secrets;
pub mod setup;
pub mod skill_install;
pub mod skill_ops;
pub mod skill_proofs;
pub mod skill_rules;
pub mod terminal;
pub mod tool;
pub mod transaction;
pub mod windows_setup;
pub mod wsl;

#[derive(Debug, PartialEq)]
pub struct Metadata {
    pub name: String,
    pub description: String,
}

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
            && name.len() <= 64
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
        !description.trim().is_empty() && description.chars().count() <= 1024,
        "description must contain 1 to 1024 characters"
    );
    ensure!(!body.trim().is_empty(), "skill body must not be empty");
    Ok(Metadata {
        name: name.into(),
        description: description.into(),
    })
}

pub fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let content = text
        .strip_prefix("---\n")
        .context("missing YAML frontmatter")?;
    let (header, body) = content
        .split_once("\n---\n")
        .context("unterminated YAML frontmatter")?;
    Ok((header, body))
}

/// Windows `canonicalize` returns verbatim drive paths (`\\?\C:\...`) that cargo rejects and chezmoi rewrites into `//?/` forms; the same drive path without the prefix is equivalent.
pub fn without_verbatim_prefix(path: &Path) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest)
            if rest.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && rest.as_bytes().get(1) == Some(&b':') =>
        {
            PathBuf::from(rest)
        }
        _ => path.to_path_buf(),
    }
}

/// `canonicalize` without the Windows verbatim drive prefix, so every comparison and every path handed to another tool uses one spelling.
#[allow(clippy::disallowed_methods)]
pub fn canonical(path: &Path) -> Result<PathBuf> {
    Ok(without_verbatim_prefix(&path.canonicalize()?))
}

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

pub fn skills(root: &Path) -> Result<Vec<(std::path::PathBuf, Metadata)>> {
    let tree = root.join("dot_agents/skills");
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

pub fn validate_tree(root: &Path) -> Result<usize> {
    let entries = skills(root)?;
    for (_, skill) in &entries {
        let alias = root
            .join("dot_claude/skills")
            .join(format!("symlink_{}", skill.name));
        let target = fs::read_to_string(&alias)
            .with_context(|| format!("missing alias {}", alias.display()))?;
        ensure!(
            target.trim() == format!("../../.agents/skills/{}", skill.name),
            "incorrect alias {}",
            alias.display()
        );
    }
    for entry in fs::read_dir(root.join("dot_claude/skills"))? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("non-UTF-8 alias"))?;
        let name = name
            .strip_prefix("symlink_")
            .context("independent Claude skill must move to canonical tree")?;
        ensure!(
            entries.iter().any(|(_, skill)| skill.name == name),
            "alias has no canonical skill: {name}"
        );
    }
    Ok(entries.len())
}

pub fn export_checks(root: &Path, output: &Path) -> Result<()> {
    validate_tree(root)?;
    let mut files = Vec::new();
    for relative in [
        "dot_agents/skills",
        "dot_claude/skills",
        "xtask",
        "guard",
        "mise.toml",
        ".chezmoiignore",
        "lefthook.yml",
        ".github/workflows/required.yml",
        "dot_config/skill-ops/policy.json",
        "dot_config/opencode/plugins/skill-ops.ts",
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

pub fn skill_source_paths(root: &Path) -> Result<Vec<PathBuf>> {
    validate_tree(root)?;
    let mut files = Vec::new();
    for relative in ["dot_agents/skills", "dot_claude/skills"] {
        collect(root, Path::new(relative), &mut files)?;
    }
    Ok(files.into_iter().map(|(path, _)| root.join(path)).collect())
}

fn collect(root: &Path, relative: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
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

pub fn check_build_dir(root: &Path, configured: Option<&Path>) -> PathBuf {
    let target = match configured {
        Some(target) => root.join(target),
        None => root.join("target"),
    };
    target.join("checks")
}
