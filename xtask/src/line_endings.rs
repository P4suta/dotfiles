use crate::eol_rules::{Action, Attributes, Eol, Text, action, content, to_lf};
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The command agent clients run after every tool call.
pub const HOOK_COMMAND: &str = "dotfiles-xtask line-endings hook";

#[derive(Debug, clap::Subcommand)]
pub enum Request {
    /// Normalize files after an agent tool call, reading the client's hook input from standard input.
    Hook,
    /// Normalize the named files under the same attribute rule.
    Normalize { paths: Vec<PathBuf> },
}

fn git(directory: &Path) -> Command {
    let mut command = Tool::Git.command();
    command.arg("-C").arg(directory);
    command
}

/// The top of the work tree containing `directory`, or `None` outside every repository.
pub fn work_tree(directory: &Path) -> Result<Option<PathBuf>> {
    let output = git(directory)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("start git")?;
    if !output.status.success() {
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        ensure!(
            diagnostics.contains("not a git repository")
                || diagnostics.contains("must be run in a work tree"),
            "git cannot inspect {}: {}",
            directory.display(),
            diagnostics.trim()
        );
        return Ok(None);
    }
    let top = String::from_utf8(output.stdout).context("work tree path is not UTF-8")?;
    Ok(Some(PathBuf::from(top.trim_end_matches(['\n', '\r']))))
}

/// A path from Git's `-z` output, which is UTF-8 on Windows and raw bytes elsewhere.
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    #[cfg(unix)]
    {
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        Ok(PathBuf::from(
            std::str::from_utf8(bytes).context("listed path is not UTF-8")?,
        ))
    }
}

fn path_bytes(path: &Path) -> Result<&[u8]> {
    #[cfg(unix)]
    {
        Ok(path.as_os_str().as_bytes())
    }
    #[cfg(not(unix))]
    {
        Ok(path.to_str().context("path is not UTF-8")?.as_bytes())
    }
}

fn listed(directory: &Path, arguments: &[&str]) -> Result<Vec<PathBuf>> {
    let output = git(directory)
        .args(["ls-files", "-z"])
        .args(arguments)
        .stderr(Stdio::inherit())
        .output()
        .context("start git")?;
    ensure!(
        output.status.success(),
        "git ls-files failed in {}",
        directory.display()
    );
    let mut paths = output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|path| !path.is_empty())
        .map(path_from_bytes)
        .collect::<Result<Vec<_>>>()?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn text(value: &str) -> Text {
    match value {
        "set" => Text::Set,
        "unset" => Text::Unset,
        "auto" => Text::Auto,
        _ => Text::Unspecified,
    }
}

fn eol(value: &str) -> Eol {
    match value {
        "lf" => Eol::Lf,
        "crlf" => Eol::Crlf,
        _ => Eol::Unspecified,
    }
}

/// Resolves `text`, `eol`, and `binary` for paths relative to `directory`.
pub fn attributes(directory: &Path, paths: &[PathBuf]) -> Result<Vec<Attributes>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path_bytes(path)?);
        input.push(0);
    }
    let mut child = git(directory)
        .args(["check-attr", "-z", "--stdin", "text", "eol", "binary"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start git")?;
    let mut stdin = child.stdin.take().context("git input is unavailable")?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .context("git output is unavailable")?
        .read_to_end(&mut output)?;
    let status = child.wait()?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("git input writer panicked"))??;
    ensure!(
        status.success(),
        "git check-attr failed in {}",
        directory.display()
    );
    let fields: Vec<&[u8]> = output.split(|&byte| byte == 0).collect();
    ensure!(
        fields.len() == paths.len() * 9 + 1 && fields.last() == Some(&&b""[..]),
        "git check-attr returned an unexpected record count"
    );
    let mut resolved = Vec::with_capacity(paths.len());
    for record in fields.as_chunks::<9>().0 {
        let mut found = Attributes {
            text: Text::Unspecified,
            eol: Eol::Unspecified,
            binary: false,
        };
        for triple in record.as_chunks::<3>().0 {
            let value = std::str::from_utf8(triple[2])?;
            match triple[1] {
                b"text" => found.text = text(value),
                b"eol" => found.eol = eol(value),
                b"binary" => found.binary = value == "set",
                other => bail!(
                    "git check-attr returned an unrequested attribute {}",
                    String::from_utf8_lossy(other)
                ),
            }
        }
        resolved.push(found);
    }
    Ok(resolved)
}

fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// Replaces `path` only while it is writable and still holds `expected`, keeping its permissions.
pub fn replace(path: &Path, expected: &[u8], next: &[u8]) -> Result<bool> {
    let parent = path.parent().context("file has no parent directory")?;
    let permissions = fs::metadata(path)?.permissions();
    if permissions.readonly() {
        eprintln!(
            "Kept CRLF in the read-only file {}; make it writable and run `dotfiles-xtask line-endings normalize {}`",
            path.display(),
            path.display()
        );
        return Ok(false);
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(next)?;
    temporary.as_file().sync_all()?;
    fs::set_permissions(temporary.path(), permissions)?;
    if fs::read(path)? != expected {
        return Ok(false);
    }
    temporary
        .persist(path)
        .with_context(|| format!("replace {}", path.display()))?;
    Ok(true)
}

/// Files under `directory` that the attribute rule would rewrite, paired with their current bytes.
fn pending(directory: &Path, paths: &[PathBuf]) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let files: Vec<PathBuf> = paths
        .iter()
        .filter(|path| regular(&directory.join(path)))
        .cloned()
        .collect();
    let resolved = attributes(directory, &files)?;
    let mut found = Vec::new();
    for (path, attributes) in files.into_iter().zip(resolved) {
        let bytes = fs::read(directory.join(&path))?;
        if action(attributes, content(&bytes)) == Action::Normalize {
            found.push((path, bytes));
        }
    }
    Ok(found)
}

/// Converts CRLF to LF in `paths`, relative to `directory`, unless their attributes exempt them.
pub fn normalize(directory: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut changed = Vec::new();
    for (path, bytes) in pending(directory, paths)? {
        let target = directory.join(&path);
        if replace(&target, &bytes, &to_lf(&bytes))? {
            changed.push(target);
        }
    }
    Ok(changed)
}

/// Tracked files whose working-tree bytes contain CRLF that their attributes do not declare.
pub fn offending(root: &Path) -> Result<Vec<PathBuf>> {
    let tracked = listed(root, &[])?;
    Ok(pending(root, &tracked)?
        .into_iter()
        .map(|(path, _)| path)
        .collect())
}

pub fn check(root: &Path) -> Result<()> {
    let found = offending(root)?;
    ensure!(
        found.is_empty(),
        "tracked text files contain CRLF line endings: {found:?}\nConvert them with `cargo run --locked --manifest-path xtask/Cargo.toml -- line-endings normalize PATH...`, or declare eol=crlf, -text, or binary for them in .gitattributes"
    );
    println!("Validated LF line endings in tracked text files");
    Ok(())
}

fn changed_files(directory: &Path) -> Result<Vec<PathBuf>> {
    match work_tree(directory)? {
        Some(top) => {
            let paths = listed(&top, &["--modified", "--others", "--exclude-standard"])?;
            normalize(&top, &paths)
        }
        None => Ok(Vec::new()),
    }
}

fn named_file(path: &Path) -> Result<Vec<PathBuf>> {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if parent.is_dir() && work_tree(parent)?.is_some() => {
            normalize(parent, &[PathBuf::from(name)])
        }
        _ => Ok(Vec::new()),
    }
}

/// Normalizes the changed files of the work tree at the hook's directory and the file the tool names.
/// A directory Git cannot inspect is reported on standard error and skipped, so one failure does not fail every tool call.
pub fn hook(input: &Value) -> Result<Vec<PathBuf>> {
    let directory = match input["cwd"].as_str() {
        Some(directory) => PathBuf::from(directory),
        None => std::env::current_dir()?,
    };
    let mut scopes = vec![(directory.clone(), changed_files(&directory))];
    for key in ["file_path", "filePath", "notebook_path"] {
        if let Some(path) = input["tool_input"][key].as_str() {
            let path = directory.join(path);
            let result = named_file(&path);
            scopes.push((path, result));
        }
    }
    let mut changed = Vec::new();
    for (scope, result) in scopes {
        match result {
            Ok(paths) => changed.extend(paths),
            Err(error) => eprintln!(
                "Warning: skipped line-ending normalization for {}: {error:#}\nRun `git -C {} status` to see why Git cannot inspect it.",
                scope.display(),
                scope
                    .parent()
                    .filter(|_| scope != directory)
                    .unwrap_or(&scope)
                    .display()
            ),
        }
    }
    Ok(changed)
}

pub fn run(request: Request) -> Result<()> {
    let changed = match request {
        Request::Hook => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            hook(&serde_json::from_str(&text).context("hook input must be JSON")?)?
        }
        Request::Normalize { paths } => {
            let directory = std::env::current_dir()?;
            normalize(&directory, &paths)?
        }
    };
    for path in changed {
        println!("Converted CRLF to LF in {}", path.display());
    }
    Ok(())
}

/// Adds the post-tool hook to a Claude Code or Codex settings document without reordering existing groups.
pub fn merge_hooks(mut value: Value) -> Result<Value> {
    let expected = json!({"hooks":[{"type":"command","command":HOOK_COMMAND,"timeout":30}]});
    let entries = value
        .as_object_mut()
        .context("client settings must be a JSON object")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("native hooks must be an object")?
        .entry("PostToolUse")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .context("native hook event must be an array")?;
    if entries.contains(&expected) {
        return Ok(value);
    }
    let mut retained = Vec::new();
    for mut entry in std::mem::take(entries) {
        let handlers = entry["hooks"]
            .as_array_mut()
            .context("native hook group must contain handlers")?;
        let before = handlers.len();
        handlers.retain(|handler| handler["command"] != HOOK_COMMAND);
        if !handlers.is_empty() || handlers.len() == before {
            retained.push(entry);
        }
    }
    retained.push(expected);
    *entries = retained;
    Ok(value)
}

/// Registers the hook in the machine-local Claude Code and Codex settings under `home`.
pub fn install(home: &Path) -> Result<()> {
    for path in [
        home.join(".codex/hooks.json"),
        home.join(".claude/settings.json"),
    ] {
        let value: Value = if path.try_exists()? {
            crate::skill_ops::read_json(&path)?
        } else {
            json!({})
        };
        let next = merge_hooks(value.clone())?;
        if next != value {
            crate::skill_ops::write_json(&path, &next, true)?;
        }
    }
    println!("Registered the line-ending hook for Claude Code and Codex");
    Ok(())
}
