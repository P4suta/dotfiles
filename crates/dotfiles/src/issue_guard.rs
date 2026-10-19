//! Before-edit issue context for supported agent mutation tools.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

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
    let root = edit_checkout(&cwd, &paths)?;
    let mut mutations = Vec::new();
    for path in paths {
        let path = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        let absolute = platform::issue_context::edit_path(&root, &path)?;
        let relative = absolute
            .strip_prefix(&root)
            .context("edit path lies outside the repository")?;
        let preparation = relative
            .extension()
            .is_some_and(|extension| extension == "md")
            && platform::issue_context::git(
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
