//! Before-edit issue context for supported agent mutation tools.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const BLOCK: i32 = 2;
const PATCH_PATHS: &[&str] = &[
    "*** Add File: ",
    "*** Update File: ",
    "*** Delete File: ",
    "*** Move to: ",
];

#[derive(Deserialize)]
struct Event {
    tool_name: String,
    cwd: Option<PathBuf>,
    #[serde(default)]
    tool_input: Value,
}

fn mutation_paths(event: &Event) -> Result<Option<Vec<PathBuf>>> {
    match event
        .tool_name
        .rsplit(['.', ':'])
        .next()
        .unwrap_or_default()
    {
        "Edit" | "Write" | "MultiEdit" | "edit" | "write" => {
            let paths: Vec<_> = ["file_path", "filePath"]
                .into_iter()
                .filter_map(|key| event.tool_input.get(key))
                .collect();
            ensure!(
                paths.len() == 1,
                "edit tool needs one unambiguous file path"
            );
            let path = paths
                .first()
                .and_then(|path| path.as_str())
                .context("edit tool has no file path")?;
            Ok(Some(vec![PathBuf::from(path)]))
        }
        "apply_patch" => {
            let fields: Vec<_> = ["command", "input", "patch", "patchText"]
                .into_iter()
                .filter_map(|key| event.tool_input.get(key))
                .collect();
            ensure!(
                fields.len() <= 1,
                "apply_patch needs one unambiguous patch input"
            );
            let patch = event
                .tool_input
                .as_str()
                .or_else(|| fields.first().and_then(|value| value.as_str()))
                .context("apply_patch has no patch text")?;
            let paths: Vec<_> = patch
                .lines()
                .filter_map(|line| {
                    PATCH_PATHS
                        .iter()
                        .find_map(|prefix| line.strip_prefix(prefix))
                })
                .map(PathBuf::from)
                .collect();
            ensure!(
                !paths.is_empty(),
                "apply_patch has no supported file mutation"
            );
            Ok(Some(paths))
        }
        _ => Ok(None),
    }
}

/// The components that name a client's persistent memory store under the home directory.
const CLAUDE: &str = ".claude";
const PROJECTS: &str = "projects";
const MEMORY: &str = "memory";
const CODEX: &str = ".codex";
const MEMORIES: &str = "memories";

fn memory(home: &Path, path: &Path) -> bool {
    use crate::agent_memory_rules::Part;
    let Ok(relative) = path.strip_prefix(home) else {
        return false;
    };
    let mut components = relative.components();
    crate::agent_memory_rules::memory(std::array::from_fn(|_| match components.next() {
        None => Part::End,
        Some(Component::Normal(name)) => match name.to_str() {
            Some(CLAUDE) => Part::Claude,
            Some(PROJECTS) => Part::Projects,
            Some(MEMORY) => Part::Memory,
            Some(CODEX) => Part::Codex,
            Some(name) if name.starts_with(MEMORIES) => Part::Memories,
            _ => Part::Other,
        },
        Some(_) => Part::Other,
    }))
}

fn memory_refusal(path: &Path) -> anyhow::Error {
    platform::refusal::Refusal::without_next(
        "agent.memory",
        "persistent agent memory stays off on every managed client, so no session reads this file; keep the lesson as a check, default, or command in the dotfiles repository, or as a document in the repository it concerns, and propose it to the owner",
    )
    .evidence(format!("edit path: {}", path.display()))
    .into()
}

/// Whether the nearest existing directory of `path` lies in a Git checkout.
fn in_checkout(path: &Path) -> Result<bool> {
    let parent = path.parent().context("edit has no parent directory")?;
    let existing = parent
        .ancestors()
        .find(|directory| directory.is_dir())
        .context("edit has no existing parent directory")?;
    Ok(crate::canonical(existing)?
        .ancestors()
        .any(|directory| directory.join(".git").exists()))
}

fn edit_checkout(cwd: &Path, paths: &[PathBuf]) -> Result<PathBuf> {
    if cwd
        .ancestors()
        .any(|directory| directory.join(".git").exists())
    {
        return platform::issue_context::checkout(cwd);
    }
    let path = paths.first().context("edit has no target")?;
    let absolute = if path.is_absolute() {
        path.clone()
    } else {
        cwd.join(path)
    };
    let parent = absolute.parent().context("edit has no parent directory")?;
    let existing = parent
        .ancestors()
        .find(|directory| directory.is_dir())
        .context("edit has no existing parent directory")?;
    platform::issue_context::checkout(existing)
}

/// # Errors
/// Returns an error for an unsupported mutation packet or missing issue context.
pub fn check(input: &str) -> Result<()> {
    let event: Event = serde_json::from_str(input)?;
    let Some(paths) = mutation_paths(&event)? else {
        return Ok(());
    };
    let cwd = crate::canonical(
        event
            .cwd
            .as_deref()
            .context("edit event has no working directory")?,
    )?;
    let paths: Vec<PathBuf> = paths
        .into_iter()
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .collect();
    if let Some(home) = platform::home::home() {
        let homes = [crate::canonical(&home).ok(), Some(home)];
        if let Some(path) = paths
            .iter()
            .find(|path| homes.iter().flatten().any(|home| memory(home, path)))
        {
            return Err(memory_refusal(path));
        }
    }
    let mut checked = Vec::new();
    for path in paths {
        if in_checkout(&path)? {
            checked.push(path);
        }
    }
    if checked.is_empty() {
        return Ok(());
    }
    let root = edit_checkout(&cwd, &checked)?;
    let mut mutations = Vec::new();
    for path in checked {
        let absolute = platform::issue_context::edit_path(&root, &path)?;
        let relative = absolute
            .strip_prefix(&root)
            .context("edit path lies outside the repository")?;
        let preparation = platform::issue_context::git(
            &root,
            &[
                "check-ignore",
                "--",
                relative.to_str().context("non-UTF-8 edit path")?,
            ],
            None,
        )
        .is_ok();
        if !preparation {
            mutations.push(absolute);
        }
    }
    if mutations.is_empty() {
        return Ok(());
    }
    platform::issue_context::guard(&root, &mutations).map_err(|error| {
        platform::refusal::Refusal::new(
            "pr.issue-context",
            "implementation edits require a checked destination-bound issue context",
            "pr-workflow start --repo OWNER/REPO --issue NUMBER --path PATH",
        )
        .evidence(format!("{error:#}"))
        .into()
    })
}

/// # Errors
/// Returns an error if reading hook input fails.
pub fn hook() -> Result<i32> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    match check(&input) {
        Ok(()) => Ok(0),
        Err(error) => {
            platform::report(&error);
            if error.downcast_ref::<platform::refusal::Refusal>().is_none() {
                eprintln!("{}", platform::refusal::DEFECT);
            }
            Ok(BLOCK)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_existing_checkout_keeps_its_boundary_and_outside_sessions_find_new_paths() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let repository = scope.path().join("repository");
        std::fs::create_dir(&repository)?;
        let initialized = platform::process::command("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .output()?;
        ensure!(initialized.status.success(), "{initialized:?}");
        let root = crate::canonical(&repository)?;
        let new = repository.join("new/directory/file.rs");
        assert_eq!(edit_checkout(scope.path(), &[new])?, root);
        let outside = scope.path().join("outside.rs");
        assert_eq!(
            edit_checkout(&repository, std::slice::from_ref(&outside))?,
            root
        );
        assert!(platform::issue_context::edit_path(&root, &outside).is_err());
        assert!(edit_checkout(scope.path(), &[outside]).is_err());
        Ok(())
    }

    #[test]
    fn edits_from_outside_the_checkout_still_require_its_issue_authority() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let repository = scope.path().join("repository");
        std::fs::create_dir(&repository)?;
        let initialized = platform::process::command("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .output()?;
        ensure!(initialized.status.success(), "{initialized:?}");
        let packet = serde_json::json!({
            "cwd": scope.path(),
            "tool_name": "apply_patch",
            "tool_input": {"command": format!("*** Add File: {}\n", repository.join("new.rs").display())}
        });
        let error = check(&packet.to_string()).unwrap_err();
        assert!(
            format!("{error:#}")
                .contains("implementation edits require a checked destination-bound issue context"),
            "{error:#}"
        );
        Ok(())
    }

    #[test]
    fn an_edit_outside_every_checkout_needs_no_issue_context() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let repository = scope.path().join("repository");
        std::fs::create_dir(&repository)?;
        let initialized = platform::process::command("git")
            .args(["init", "--quiet"])
            .current_dir(&repository)
            .output()?;
        ensure!(initialized.status.success(), "{initialized:?}");
        let outside = scope.path().join("config/new.yaml");
        for cwd in [scope.path(), repository.as_path()] {
            let packet = serde_json::json!({
                "cwd": cwd,
                "tool_name": "Write",
                "tool_input": {"file_path": outside}
            });
            check(&packet.to_string())?;
        }
        let mixed = serde_json::json!({
            "cwd": scope.path(),
            "tool_name": "apply_patch",
            "tool_input": {"command": format!(
                "*** Add File: {}\n*** Add File: {}\n",
                outside.display(),
                repository.join("new.rs").display()
            )}
        });
        assert!(check(&mixed.to_string()).is_err());
        std::fs::write(repository.join(".gitignore"), "target/\n")?;
        let ignored = serde_json::json!({
            "cwd": repository,
            "tool_name": "Write",
            "tool_input": {"file_path": repository.join("target/scratch/probe.rs")}
        });
        check(&ignored.to_string())?;
        Ok(())
    }

    #[test]
    fn a_client_memory_store_is_memory_and_its_neighbors_are_not() {
        let home = Path::new("/home/owner");
        for path in [
            ".claude/projects/-home-owner-repo/memory/MEMORY.md",
            ".claude/projects/p/memory",
            ".codex/memories_1.sqlite",
            ".codex/memories/entry.md",
        ] {
            assert!(memory(home, &home.join(path)), "{path}");
        }
        for path in [
            ".claude/projects/p/session.jsonl",
            ".claude/memory/note.md",
            ".codex/config.toml",
            "projects/repo/memory/note.md",
        ] {
            assert!(!memory(home, &home.join(path)), "{path}");
        }
        assert!(!memory(
            home,
            Path::new("/elsewhere/.claude/projects/p/memory/a.md")
        ));
        let refusal = format!(
            "{:#}",
            memory_refusal(&home.join(".codex/memories_1.sqlite"))
        );
        assert!(
            refusal.contains("persistent agent memory stays off"),
            "{refusal}"
        );
    }

    #[test]
    fn supported_mutations_include_moves_and_read_tools_remain_available() -> Result<()> {
        let event = |tool: &str, input: Value| Event {
            tool_name: tool.to_owned(),
            cwd: Some(PathBuf::from(".")),
            tool_input: input,
        };
        assert!(mutation_paths(&event("Read", Value::Null))?.is_none());
        assert!(
            mutation_paths(&event("Bash", serde_json::json!({"command":"rg --files"})))?.is_none()
        );
        check(r#"{"tool_name":"Read"}"#)?;
        assert!(mutation_paths(&event("apply_patch", Value::Null)).is_err());
        assert!(
            mutation_paths(&event(
                "Write",
                serde_json::json!({"file_path":"good.rs","filePath":"other.rs"})
            ))
            .is_err()
        );
        assert!(mutation_paths(&event("apply_patch", serde_json::json!({"input":"*** Add File: good.rs","patchText":"*** Add File: other.rs"}))).is_err());
        assert!(mutation_paths(&event("apply_patch", serde_json::json!({"command":"*** Add File: good.rs","input":"*** Add File: other.rs"}))).is_err());
        assert!(mutation_paths(&event("apply_patch", serde_json::json!({"command":17}))).is_err());
        assert_eq!(
            mutation_paths(&event(
                "apply_patch",
                serde_json::json!({"command":"*** Add File: canonical.rs\n"})
            ))?,
            Some(vec![PathBuf::from("canonical.rs")])
        );
        assert_eq!(
            mutation_paths(&event(
                "Edit",
                serde_json::json!({"file_path":"crates/a.rs"})
            ))?,
            Some(vec![PathBuf::from("crates/a.rs")])
        );
        assert_eq!(
            mutation_paths(&event(
                "apply_patch",
                Value::String("*** Update File: old.rs\n*** Move to: new.rs\n".into())
            ))?,
            Some(vec![PathBuf::from("old.rs"), PathBuf::from("new.rs")])
        );
        assert_eq!(
            mutation_paths(&event(
                "functions.apply_patch",
                serde_json::json!({"patchText":"*** Add File: new.rs\n"})
            ))?,
            Some(vec![PathBuf::from("new.rs")])
        );
        Ok(())
    }
}
