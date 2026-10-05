//! Render-and-check gates for the files of this repository.
//!
//! A missing validator downgrades to a note and exit 0, so a missing tool never blocks a commit.

use crate::locate;
use crate::realgit;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub fn dispatch(args: &[String]) -> i32 {
    let Some(kind) = args.first().map(String::as_str) else {
        eprintln!(
            "usage: dotguard lint <toml|plist|json|yaml|nu|herdr|gitconfig|template> <files…>"
        );
        return 2;
    };
    let files = &args[1..];
    match kind {
        "toml" => toml(files),
        "plist" => plist(files),
        "json" => json(files),
        "yaml" => yaml(files),
        "nu" => nu(files),
        "herdr" => herdr(files),
        "gitconfig" => gitconfig(files),
        "template" => template(files),
        _ => {
            eprintln!("dotguard lint: unknown kind '{kind}'");
            2
        }
    }
}

fn err(file: &str, msg: &str) {
    eprintln!("::error file={file}:: {msg}");
}

fn note(msg: &str) {
    eprintln!("lint: {msg}");
}

fn existing(files: &[String]) -> Vec<&String> {
    files
        .iter()
        .filter(|f| Path::new(f.as_str()).is_file())
        .collect()
}

fn has_ext(path: &str, ext: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.to_string_lossy() == ext)
}

/// Renders `f` with chezmoi and returns the bytes, passing other files through.
fn render(f: &str) -> Result<Vec<u8>, ()> {
    if !has_ext(f, "tmpl") {
        return std::fs::read(f).map_err(|_| ());
    }
    let init = is_init_config(Path::new(f));
    let mut args: Vec<String> = vec!["execute-template".into()];
    if init {
        args.push("--init".into());
        args.push("--no-tty".into());
    }
    args.push("--file".into());
    args.push(f.into());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = locate::child("chezmoi")
        .args(&refs)
        .output()
        .map_err(|_| ())?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(())
    }
}

/// A private temp file with the suffix that validators dispatch on, because BSD `mktemp` lacks `--suffix`.
fn temp_file(ext: &str, bytes: &[u8]) -> Option<PathBuf> {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dotguard-lint.{}.{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("rendered.{ext}"));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

fn temp_dir_of(p: &Path) -> PathBuf {
    p.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The init configs may call `promptString` and friends, so they render in an `--init --no-tty` context: prompts fall back to the template default instead of hanging waiting for input.
fn is_init_config(p: &Path) -> bool {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
    matches!(
        name.as_deref(),
        Some(".chezmoi.toml.tmpl" | ".chezmoi.yaml.tmpl" | ".chezmoi.json.tmpl")
    )
}

fn run_quiet(cmd: &mut std::process::Command, file: &str, what: &str) -> bool {
    match cmd.output() {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            err(file, what);
            eprint!("{}", String::from_utf8_lossy(&o.stderr));
            false
        }
        Err(e) => {
            err(file, &format!("{what}: could not run the validator: {e}"));
            false
        }
    }
}

// -- toml ------------------------------------------------------------------

fn toml(files: &[String]) -> i32 {
    let mut rc = 0;
    for f in existing(files) {
        if has_ext(f, "tmpl") {
            let Ok(bytes) = render(f) else {
                err(f, "chezmoi execute-template failed");
                rc = 1;
                continue;
            };
            let Some(tmp) = temp_file("toml", &bytes) else {
                err(f, "temp file creation failed");
                rc = 1;
                continue;
            };
            // A Go template never matches the canonical formatting of taplo, so rendered files get lint only.
            if !run_quiet(
                locate::child("taplo").arg("lint").arg(&tmp),
                f,
                "taplo lint failed",
            ) {
                rc = 1;
            }
            let _ = std::fs::remove_dir_all(temp_dir_of(&tmp));
        } else {
            if !run_quiet(
                locate::child("taplo").arg("lint").arg(f),
                f,
                "taplo lint failed",
            ) {
                rc = 1;
            }
            if !run_quiet(
                locate::child("taplo").args(["fmt", "--check", f]),
                f,
                "taplo fmt --check failed (run: taplo fmt)",
            ) {
                rc = 1;
            }
        }
    }
    rc
}

// -- plist -----------------------------------------------------------------

fn plist(files: &[String]) -> i32 {
    let mut rc = 0;
    for f in existing(files) {
        let Ok(bytes) = render(f) else {
            err(f, "chezmoi execute-template failed");
            rc = 1;
            continue;
        };
        // launchd uses the plutil parser, and a plist that fails here would fail `launchctl bootstrap` with a generic error and no line number.
        let Some(tmp) = temp_file("plist", &bytes) else {
            err(f, "temp file creation failed");
            rc = 1;
            continue;
        };
        if !run_quiet(
            locate::child("plutil").arg("-lint").arg(&tmp),
            f,
            "plutil -lint failed",
        ) {
            rc = 1;
        }
        // `launchctl bootout` needs the `Label` to remove a job by name.
        if !locate::child("plutil")
            .args(["-extract", "Label", "raw", "-o", "/dev/null"])
            .arg(&tmp)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            err(f, "plist has no Label key");
            rc = 1;
        }
        let _ = std::fs::remove_dir_all(temp_dir_of(&tmp));
    }
    rc
}

// -- json ------------------------------------------------------------------

fn json(files: &[String]) -> i32 {
    let mut rc = 0;
    for f in existing(files) {
        let Ok(bytes) = render(f) else {
            err(f, "chezmoi execute-template failed");
            rc = 1;
            continue;
        };
        let mut spawned = locate::child("jq")
            .arg("empty")
            .stdin(Stdio::piped())
            .spawn();
        let ok = if let Ok(c) = spawned.as_mut() {
            if let Some(mut si) = c.stdin.take() {
                use std::io::Write;
                let _ = si.write_all(&bytes);
            }
            c.wait().is_ok_and(|s| s.success())
        } else {
            err(f, "could not run jq");
            false
        };
        if !ok {
            err(f, "JSON parse failed");
            rc = 1;
        }
    }
    rc
}

// -- yaml ------------------------------------------------------------------

fn yaml(files: &[String]) -> i32 {
    let files = existing(files);
    if files.is_empty() {
        return 0;
    }
    if locate::which("uvx").is_none() {
        note("uvx is not installed; skipping YAML validation");
        return 0;
    }
    // yamllint runs through uvx to stay off `PATH` as a gate-only dependency.
    let config = realgit::capture(&["rev-parse", "--show-toplevel"]).map_or_else(
        || ".yamllint".to_owned(),
        |root| {
            Path::new(root.trim())
                .join(".yamllint")
                .to_string_lossy()
                .into_owned()
        },
    );
    let mut cmd = locate::child("uvx");
    cmd.args([
        "--quiet", "--from", "yamllint", "yamllint", "--strict", "-c", &config,
    ]);
    for f in &files {
        cmd.arg(f);
    }
    let mut rc = 0;
    match cmd.output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprint!("{}", String::from_utf8_lossy(&o.stderr));
            rc = 1;
        }
        Err(e) => {
            note(&format!("yamllint could not run: {e}"));
        }
    }
    rc
}

// -- nushell ---------------------------------------------------------------

fn nu(files: &[String]) -> i32 {
    if locate::which("nu").is_none() {
        note("nu is not installed; skipping semantic validation");
        return 0;
    }
    let mut rc = 0;
    for f in existing(files).into_iter().filter(|f| has_ext(f, "nu")) {
        let script = "if not (nu-check --debug ($env.NU_CHECK_FILE | path expand)) { exit 1 }";
        let ok = locate::child("nu")
            .env("NU_CHECK_FILE", f.as_str())
            .args(["--no-config-file", "-c", script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            err(f, "nu-check failed");
            rc = 1;
        }
    }
    rc
}

// -- herdr -----------------------------------------------------------------

fn herdr(files: &[String]) -> i32 {
    if locate::which("herdr").is_none() {
        note("herdr is not installed; skipping semantic validation");
        return 0;
    }
    let mut rc = 0;
    for f in existing(files)
        .into_iter()
        .filter(|f| f.ends_with("/herdr/config.toml") || f.as_str() == "herdr/config.toml")
    {
        if !run_quiet(
            locate::child("herdr")
                .arg("config")
                .arg("check")
                .env("HERDR_CONFIG_PATH", f.as_str()),
            f,
            "herdr config check failed",
        ) {
            rc = 1;
        }
    }
    rc
}

// -- gitconfig -------------------------------------------------------------

fn gitconfig(files: &[String]) -> i32 {
    let mut rc = 0;
    for f in existing(files) {
        let Ok(bytes) = render(f) else {
            err(f, "chezmoi execute-template failed");
            rc = 1;
            continue;
        };
        let Some(tmp) = temp_file("gitconfig", &bytes) else {
            err(f, "temp file creation failed");
            rc = 1;
            continue;
        };

        // Through the real git, not the ~/.local/bin/git wrapper.
        let parsed = realgit::command().and_then(|mut c| {
            c.args(["config", "--list", "--file"])
                .arg(&tmp)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        });
        let Some(parsed) = parsed else {
            err(f, "git config parse failed");
            let _ = std::fs::remove_dir_all(temp_dir_of(&tmp));
            rc = 1;
            continue;
        };

        // A `#` opens a comment, so an unquoted color literal vanishes and drops the theme without a syntax error.
        for colour in colours(&String::from_utf8_lossy(&bytes)) {
            if !parsed.contains(&colour) {
                err(
                    f,
                    &format!(
                        "colour {colour} is lost when git parses the file — quote the value, '#' starts a comment"
                    ),
                );
                rc = 1;
            }
        }
        let _ = std::fs::remove_dir_all(temp_dir_of(&tmp));
    }
    rc
}

/// The distinct `#rrggbb` literals in a git-config file.
fn colours(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let b = text.as_bytes();
    let mut i = 0usize;
    while i + 7 <= b.len() {
        if b[i] == b'#' && b[i + 1..i + 7].iter().all(u8::is_ascii_hexdigit) {
            let c = text[i..i + 7].to_ascii_lowercase();
            if !out.contains(&c) {
                out.push(c);
            }
            i += 7;
        } else {
            i += 1;
        }
    }
    out.sort_unstable();
    out
}

// -- template --------------------------------------------------------------

fn template(files: &[String]) -> i32 {
    let mut rc = 0;
    for f in existing(files).into_iter().filter(|f| has_ext(f, "tmpl")) {
        // The init configs render with prompt functions, and the toml gate and chezmoi doctor cover them.
        if is_init_config(Path::new(f)) {
            continue;
        }
        if render(f).is_err() {
            err(f, "chezmoi execute-template failed");
            rc = 1;
        }
    }
    rc
}

#[cfg(test)]
mod tests {
    use super::{colours, is_init_config};
    use std::path::Path;

    #[test]
    fn init_configs_are_detected_by_name() {
        assert!(is_init_config(Path::new(".chezmoi.toml.tmpl")));
        assert!(is_init_config(Path::new("x/.chezmoi.yaml.tmpl")));
        assert!(!is_init_config(Path::new("dot_gitconfig.tmpl")));
        assert!(!is_init_config(Path::new(".chezmoiignore")));
    }

    #[test]
    fn colours_are_distinct_and_lowercased() {
        let text = "minus-style = \"normal #3F2D3D\"\nline = \"#9ece6a\"\nagain = \"#3f2d3d\"\n";
        assert_eq!(
            colours(text),
            vec!["#3f2d3d".to_owned(), "#9ece6a".to_owned()]
        );
        assert_eq!(colours("#12345 no hex tail"), Vec::<String>::new());
        assert_eq!(colours("#12345g"), Vec::<String>::new());
    }
}
