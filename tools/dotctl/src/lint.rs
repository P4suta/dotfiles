//! The lint layer: one validator per file kind, all render-then-validate.
//!
//! lefthook decides which kind gets which files through its `glob:` filters,
//! so a kind here trusts the paths it is handed and only skips ones that no longer exist (a staged file can be deleted in the same commit).

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::ValueEnum;

use crate::proc::write_temp;
use crate::{env, proc, render};

/// PowerShell has to check PowerShell: PSScriptAnalyzer is a pwsh module with no other interface.
/// Embedded so the repo keeps no loose wrapper scripts.
const PSSA_HELPER: &str = include_str!("../assets/psscriptanalyzer.ps1");
/// The repository's rules are compiled in, so a missing file fails the build instead of every lint run.
const YAMLLINT_CONFIG: &str = include_str!("../../../.yamllint");
const PSSA_SETTINGS: &str = include_str!("../../../PSScriptAnalyzerSettings.psd1");

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    /// Spell checker over any text file.
    Typos,
    /// bash syntax plus style findings at every severity.
    Shellcheck,
    /// PowerShell, via PSScriptAnalyzer.
    Ps1,
    /// TOML syntax, via taplo.
    Toml,
    /// JSON syntax.
    Json,
    /// YAML syntax, via yamllint.
    Yaml,
    /// git config syntax, which catches bad escapes in section headers.
    Gitconfig,
    /// The prose gate: one sentence per line, via ocomment.
    Prose,
    /// Catch-all: every template must render, including the extensions no validator above claims.
    Template,
}

/// Runs one kind over `files`, returning the exit code to hand back to lefthook: 0 when everything passed.
pub fn run(kind: Kind, files: &[PathBuf]) -> Result<i32> {
    let present: Vec<&Path> = files
        .iter()
        .map(PathBuf::as_path)
        .filter(|path| path.exists())
        .collect();
    if present.is_empty() {
        return Ok(0);
    }

    match kind {
        // These take the paths themselves, and report per file already.
        Kind::Typos => whole_batch("typos", &present),
        // Discovers its own configuration (.ocomment.toml) and skips languages it cannot lex.
        Kind::Prose => prose(&present),
        Kind::Shellcheck => whole_batch("shellcheck", &present),
        // Renders every file first, then checks them in one pwsh process.
        Kind::Ps1 => powershell(&present),
        _ => {
            let mut code = 0;
            for path in present {
                if !check_one(kind, path)? {
                    code = 1;
                }
            }
            Ok(code)
        }
    }
}

/// ocomment check: exit 1 carries findings; exit 2 only says some files were unwritable, which the gate does not judge.
fn prose(files: &[&Path]) -> Result<i32> {
    let mut cmd = env::command("ocomment");
    cmd.arg("check").args(files);
    let code = proc::status(&mut cmd)?.code();
    if code == 2 { Ok(0) } else { Ok(code) }
}

/// Validators that accept every path in one invocation.
fn whole_batch(program: &str, files: &[&Path]) -> Result<i32> {
    let mut cmd = env::command(program);
    cmd.args(files);
    Ok(proc::status(&mut cmd)?.code())
}

/// Returns false when the file failed its validator.
fn check_one(kind: Kind, path: &Path) -> Result<bool> {
    if kind == Kind::Template {
        return template(path);
    }

    let content = match render::content_of(path) {
        Ok(content) => content,
        Err(err) => {
            report(path, &format!("chezmoi execute-template failed\n{err}"));
            return Ok(false);
        }
    };

    match kind {
        Kind::Toml => external(path, &content, ".toml", "taplo", &["lint"]),
        Kind::Json => json(path, &content),
        Kind::Yaml => yaml(path, &content),
        Kind::Gitconfig => external(path, &content, "", "git", &["config", "--list", "--file"]),
        Kind::Typos | Kind::Shellcheck | Kind::Ps1 | Kind::Prose | Kind::Template => {
            unreachable!("handled above")
        }
    }
}

/// `::error file=...` keeps the findings greppable in a lefthook run, which interleaves the output of parallel commands.
fn report(path: &Path, message: &str) {
    println!("::error file={}:: {message}", path.display());
}

/// Writes rendered content to a temp file and hands it to an external validator, which is the only way to check text that only exists after a template renders.
fn external(
    path: &Path,
    content: &str,
    suffix: &str,
    program: &str,
    args: &[&str],
) -> Result<bool> {
    let temp = write_temp(content, suffix)?;
    let mut cmd = env::command(program);
    cmd.args(args).arg(temp.as_os_str());
    let out = proc::capture(&mut cmd)?;
    if out.ok() {
        return Ok(true);
    }
    report(path, &format!("{program} failed"));
    out.echo();
    Ok(false)
}

/// yamllint is a global mise tool (.chezmoidata.toml), so the shim answers it from any directory.
/// Going through `uvx` instead made every run re-resolve a Python interpreter, which a stale `uv` earlier on PATH could break.
fn yaml(path: &Path, content: &str) -> Result<bool> {
    let config = write_temp(YAMLLINT_CONFIG, ".yaml")?;
    let config = config.to_string_lossy().into_owned();
    external(path, content, ".yaml", "yamllint", &["-c", &config])
}

fn json(path: &Path, content: &str) -> Result<bool> {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(_) => Ok(true),
        Err(err) => {
            report(path, &format!("JSON parse failed - {err}"));
            Ok(false)
        }
    }
}

/// Importing the analyzer module costs over a second, so every file goes to one pwsh process.
/// The manifest pairs each rendered temp file with the source path the report should name.
fn powershell(files: &[&Path]) -> Result<i32> {
    let mut code = 0;
    let mut rendered: Vec<&Path> = Vec::new();
    let mut entries = Vec::new();

    for path in files {
        match render::content_of(path) {
            Ok(content) => {
                entries.push(serde_json::json!({
                    "content": content,
                    "label": path.display().to_string(),
                }));
                rendered.push(path);
            }
            Err(err) => {
                report(
                    path,
                    &format!(
                        "chezmoi execute-template failed
{err}"
                    ),
                );
                code = 1;
            }
        }
    }
    if entries.is_empty() {
        return Ok(code);
    }

    let manifest = write_temp(&serde_json::to_string(&entries)?, ".json")?;
    let helper = write_temp(PSSA_HELPER, ".ps1")?;
    let settings = write_temp(PSSA_SETTINGS, ".psd1")?;

    let mut cmd = env::command("pwsh");
    cmd.args(["-NoLogo", "-NoProfile", "-File"])
        .arg(helper.as_os_str())
        .arg("-Manifest")
        .arg(manifest.as_os_str())
        .arg("-Settings")
        .arg(settings.as_os_str());
    let analyzer = proc::status(&mut cmd)?.code();
    Ok(if analyzer == 0 { code } else { analyzer })
}

fn template(path: &Path) -> Result<bool> {
    // The bootstrap config's syntax is covered by taplo and chezmoi doctor;
    // rendering it here would need an init context it cannot have.
    if render::is_chezmoi_config(path) {
        return Ok(true);
    }
    match render::render(path) {
        Ok(_) => Ok(true),
        Err(err) => {
            report(path, &format!("chezmoi execute-template failed\n{err}"));
            Ok(false)
        }
    }
}
