use anyhow::Result;
use dotfiles_xtask::{Metadata, export_checks, metadata, validate_links, validate_tree};
use std::fs;

#[test]
fn folded_and_quoted_yaml_descriptions_are_discovered() -> Result<()> {
    let folded = "---\nname: example-skill\ndescription: >-\n  Inspect a project.\n  Use for a concrete task.\n---\n\n# Example\n";
    assert_eq!(
        metadata(folded, "example-skill")?,
        Metadata {
            name: "example-skill".into(),
            description: "Inspect a project. Use for a concrete task.".into(),
        }
    );
    let quoted =
        "---\nname: example-skill\ndescription: \"Inspect: a project.\"\n---\n\n# Example\n";
    assert_eq!(
        metadata(quoted, "example-skill")?.description,
        "Inspect: a project."
    );
    Ok(())
}

#[test]
fn malformed_or_incompatible_metadata_is_rejected() {
    for text in [
        "name: example-skill\ndescription: Task\n",
        "---\nname: example-skill\ndescription: [unterminated\n---\n",
        "---\nname: example-skill\ndescription: true\n---\n# Body\n",
        "---\nname: example-skill\ndescription: Task\nclient-only-field: true\n---\n# Body\n",
        "---\nname: other\ndescription: Task\n---\n# Body\n",
        "---\nname: example-skill\ndescription: Task\n---\n",
    ] {
        assert!(metadata(text, "example-skill").is_err(), "accepted {text}");
    }
}

#[test]
fn invalid_discovery_names_and_description_lengths_are_rejected() {
    for name in [
        "Example",
        "-example",
        "example--skill",
        "example-",
        "synced",
    ] {
        let text = format!("---\nname: {name}\ndescription: Task\n---\n# Body\n");
        assert!(metadata(&text, name).is_err());
    }
    for description in [String::new(), "x".repeat(1025)] {
        let text = format!("---\nname: example\ndescription: '{description}'\n---\n# Body\n");
        assert!(metadata(&text, "example").is_err());
    }
}

#[test]
fn relative_resources_resolve_and_fenced_examples_are_not_links() -> Result<()> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("SKILL.md");
    fs::write(root.path().join("reference.md"), "# Reference\n")?;
    let text = "[Reference](reference.md#subject)\n\n```md\n[Sample](missing.md)\n```\n[Official](https://example.com/manual)\n";
    validate_links(text, &file, root.path())
}

#[test]
fn broken_or_escaping_local_resources_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("reference.md"), "# External\n")?;
    let file = root.path().join("SKILL.md");
    assert!(validate_links("[Broken](missing.md)", &file, root.path()).is_err());
    let escaped = format!(
        "[Escape]({})",
        outside.path().join("reference.md").display()
    );
    assert!(validate_links(&escaped, &file, root.path()).is_err());
    Ok(())
}

fn tree() -> Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("dot_agents/skills/example"))?;
    fs::create_dir_all(root.path().join("dot_claude/skills"))?;
    fs::write(
        root.path().join("dot_agents/skills/example/SKILL.md"),
        "---\nname: example\ndescription: Task\n---\n# Body\n",
    )?;
    fs::write(
        root.path().join("dot_claude/skills/symlink_example"),
        "../../.agents/skills/example\n",
    )?;
    Ok(root)
}

#[test]
fn discovery_aliases_are_complete_and_cannot_drift() -> Result<()> {
    let root = tree()?;
    assert_eq!(validate_tree(root.path())?, 1);
    let alias = root.path().join("dot_claude/skills/symlink_example");
    fs::write(&alias, "/tmp/unrelated-source\n")?;
    assert!(validate_tree(root.path()).is_err());
    fs::remove_file(&alias)?;
    assert!(validate_tree(root.path()).is_err());
    fs::write(&alias, "../../.agents/skills/example\n")?;
    fs::create_dir(root.path().join("dot_claude/skills/independent-copy"))?;
    assert!(validate_tree(root.path()).is_err());
    Ok(())
}

#[test]
fn check_snapshots_preserve_sources_and_exclude_builds_and_unrelated_state() -> Result<()> {
    let root = tree()?;
    fs::create_dir_all(root.path().join("xtask/src"))?;
    fs::create_dir_all(root.path().join("xtask/target"))?;
    fs::write(
        root.path().join("xtask/src/lib.rs"),
        "pub fn example() {}\n",
    )?;
    fs::write(root.path().join("xtask/target/build"), "build bytes")?;
    fs::write(root.path().join("mise.toml"), "[tools]\n")?;
    fs::create_dir_all(root.path().join("guard/src"))?;
    fs::create_dir_all(root.path().join("guard/target"))?;
    fs::write(root.path().join("guard/src/lib.rs"), "pub fn guard() {}\n")?;
    fs::write(
        root.path().join("guard/target/build"),
        "private build bytes",
    )?;
    for relative in [
        ".chezmoiignore",
        "lefthook.yml",
        ".github/workflows/required.yml",
        "dot_config/skill-ops/policy.json",
        "dot_config/opencode/plugins/skill-ops.ts",
        "docs/skills/decisions/alpha.json",
        "package.json",
        "bun.lock",
        "tsconfig.json",
    ] {
        let path = root.path().join(relative);
        fs::create_dir_all(path.parent().expect("check artifact parent"))?;
        fs::write(path, "check configuration")?;
    }
    fs::write(root.path().join("unrelated-state"), "private")?;
    let parent = tempfile::tempdir()?;
    let output = parent.path().join("snapshot");
    export_checks(root.path(), &output)?;
    assert_eq!(
        fs::read(output.join("xtask/src/lib.rs"))?,
        fs::read(root.path().join("xtask/src/lib.rs"))?
    );
    assert!(!output.join("xtask/target").try_exists()?);
    assert!(!output.join("guard/target").try_exists()?);
    assert_eq!(
        fs::read(output.join("guard/src/lib.rs"))?,
        b"pub fn guard() {}\n"
    );
    assert!(!output.join("unrelated-state").try_exists()?);
    assert_eq!(
        fs::read(output.join("docs/skills/decisions/alpha.json"))?,
        fs::read(root.path().join("docs/skills/decisions/alpha.json"))?
    );
    assert!(export_checks(root.path(), &output).is_err());
    assert_eq!(validate_tree(&output)?, 1);
    Ok(())
}

#[test]
fn child_checks_cannot_overwrite_the_running_task_executable() -> Result<()> {
    let root = tempfile::tempdir()?;
    let cache = tempfile::tempdir()?;
    assert_eq!(
        dotfiles_xtask::check_build_dir(root.path(), Some(cache.path())),
        cache.path().join("checks")
    );
    assert_eq!(
        dotfiles_xtask::check_build_dir(root.path(), Some(std::path::Path::new("relative-target"))),
        root.path().join("relative-target/checks")
    );
    assert_eq!(
        dotfiles_xtask::check_build_dir(root.path(), None),
        root.path().join("target/checks")
    );
    Ok(())
}
