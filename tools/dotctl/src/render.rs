//! Reads a source file as a validator should see it.
//!
//! `chezmoi execute-template` renders a `.tmpl` first, so one pass catches template parse errors and syntax errors in the output.

use std::path::Path;

use anyhow::Result;

use crate::{env, proc};

/// True for the bootstrap config, whose template calls the init-only `promptStringOnce` and `stateGet`, which plain `execute-template` rejects.
pub fn is_chezmoi_config(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".chezmoi.toml.tmpl" | ".chezmoi.yaml.tmpl" | ".chezmoi.json.tmpl"
            )
        })
}

pub fn is_template(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "tmpl")
}

/// Contents of `path`: rendered for a template, read from disk otherwise.
///
/// The rendered text keeps its trailing newline, which yamllint's `new-line-at-end-of-file` rule checks.
pub fn content_of(path: &Path) -> Result<String> {
    if is_template(path) {
        render(path)
    } else {
        let bytes = std::fs::read(path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Renders one template through chezmoi.
pub fn render(path: &Path) -> Result<String> {
    let mut cmd = env::command("chezmoi");
    cmd.arg("execute-template");
    if is_chezmoi_config(path) {
        // `--init` supplies an init context without answers, so template defaults apply, and `--no-tty` keeps a prompt from hanging a hook.
        cmd.args(["--init", "--no-tty"]);
    }
    cmd.arg("--file").arg(path);
    proc::capture_ok(&mut cmd)
}
