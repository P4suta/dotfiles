//! Reading a source file the way a validator should see it.
//!
//! The render-then-validate rule: a `.tmpl` is pushed through `chezmoi execute-template` first, so a template parse error and a syntax error in the rendered output are caught in the same pass.

use std::path::Path;

use anyhow::Result;

use crate::{env, proc};

/// True for the bootstrap config, whose template uses init-only functions (`promptStringOnce`, `stateGet`) that plain `execute-template` rejects.
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
/// Unlike the PowerShell wrapper this replaces, the rendered text keeps its trailing newline.
/// That wrapper collected `execute-template` output as an array of lines and re-joined it, which dropped the final newline and made yamllint's `new-line-at-end-of-file` fire on every template.
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
        // --init supplies the init context without answers, so template defaults apply; --no-tty keeps a prompt from hanging a hook.
        cmd.args(["--init", "--no-tty"]);
    }
    cmd.arg("--file").arg(path);
    proc::capture_ok(&mut cmd)
}
