//! The dependency-freshness gate — the counterpart to the language gate for versions: a push that touches a dependency must not leave it on a stale non-major release.
//!
//! Since `core.hooksPath` routes every repository through the same hooks, the check runs just before a push in any repo whose pushed range touches a manifest Renovate understands, with zero per-repo config.
//! The staleness facts themselves come from the pinned read-only renovate container this module runs; the same run is reachable by hand as `dotguard renovate run`.
//!
//! The rule: refuse the push when its range adds or changes a manifest line for a dependency that has a non-major update available (minor / patch / pin / pinDigest by default; override with `RENOVATE_LOCAL_GATE_TYPES`).
//! Everything else is informational:
//!
//! * major updates never block — deferring them is a legitimate decision, and "you touched it, so use the current release" is the whole point;
//! * stale dependencies the push did not touch remain Renovate's job: listed for the touched manifests, never blocking.
//!
//! It runs only when all of the following hold, otherwise it returns 0 — quietly for opt-outs, with a `::warning::` when a prerequisite is missing, so a missing dev tool never blocks a push.
//! A container failure (network, image pull, …)
//! is also fail-open: the gate blocks on evidence, not on the absence of it.
//!
//! Opt-outs: `git config renovate.localSkip true` (add `--global` for a machine-wide kill switch), or a one-shot `RENOVATE_LOCAL_SKIP=1 git push`.

use crate::locate;
use crate::realgit;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const DEFAULT_GATE_TYPES: &str = "minor patch pin pinDigest";

/// One dependency with a blocking update available, as parsed from the jq report.
/// Fields are positional; the unit separator keeps empty ones alive.
struct Candidate {
    package_file: String,
    dep_name: String,
    current_value: String,
    current_digest: String,
    replace_string: String,
    update_type: String,
    new_value: String,
    new_digest: String,
}

/// Route `dotguard renovate <run>`.
pub fn dispatch(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("run") {
        run_local()
    } else {
        eprintln!("usage: dotguard renovate run");
        2
    }
}

/// `dotguard renovate-gate <remote>` — read the ref list from stdin.
pub fn run(remote: &str) -> i32 {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return 0;
    }
    gate(remote, &input)
}

fn gate(remote: &str, input: &str) -> i32 {
    if realgit::capture(&["rev-parse", "--show-toplevel"]).is_none() {
        return 0;
    }

    // Opt-outs first: nobody pays for a container start they opted out of.
    if realgit::capture(&["config", "--type=bool", "--get", "renovate.localSkip"])
        .is_some_and(|v| v.trim() == "true")
    {
        return 0;
    }
    if std::env::var_os("RENOVATE_LOCAL_SKIP").is_some_and(|v| !v.is_empty()) {
        return 0;
    }

    let Some((changed_files, added_lines, scope_unknown)) = scope(remote, input) else {
        return 0;
    };

    // Prerequisites, fail-open but loudly.
    for tool in ["docker", "jq"] {
        if locate::which(tool).is_none() {
            eprintln!("::warning:: renovate gate skipped: {tool} not found");
            return 0;
        }
    }
    eprintln!(
        ">>> renovate gate: dependency manifests changed — checking the touched dependencies..."
    );

    let report = renovate_report();
    let Some(report) = report else {
        eprintln!("::warning:: renovate gate skipped: renovate-local did not produce a report");
        return 0;
    };

    let types = std::env::var("RENOVATE_LOCAL_GATE_TYPES")
        .unwrap_or_else(|_| DEFAULT_GATE_TYPES.to_owned());
    let Some(out) = jq_candidates(&types, &report) else {
        return 0; // jq failed: fail-open, same contract as a missing report
    };
    let (blocked, stale) = classify(
        &parse_candidates(&out),
        &changed_files,
        &added_lines,
        scope_unknown,
    );

    if !stale.is_empty() {
        eprintln!(
            "::notice:: stale but untouched in this push (left for Renovate):\n{}",
            stale.join("\n")
        );
    }
    if blocked.is_empty() {
        eprintln!("renovate gate: touched dependencies are current");
        return 0;
    }

    eprintln!(
        "::error:: push refused: this push changes dependencies that are not on their latest non-major release:\n{}\n\n\
         Bump them to the versions above (SHA-pinned actions: update both the hash and\n\
         the version comment), then push again. Escape hatches:\n\n  \
         RENOVATE_LOCAL_SKIP=1 git push          # this push only\n  \
         git config renovate.localSkip true      # this repo (add --global for the machine)\n  \
         RENOVATE_LOCAL_GATE_TYPES=\"patch\" ...   # narrow what blocks\n\n\
         Skipping pre-push hooks entirely is not an option here: the ~/.local/bin/git\n\
         wrapper refuses the hook-bypass flag without a waiver.",
        blocked.join("\n")
    );
    1
}

/// The manifest gate: which manifests the pushed range changes, the lines it adds, and whether the scope had to be assumed total.
/// `None` when the push touches nothing Renovate manages — the common case, and the one that must cost nothing.
fn scope(remote: &str, input: &str) -> Option<(Vec<String>, String, bool)> {
    let mut changed_files: Vec<String> = Vec::new();
    let mut added_lines = String::new();
    let mut scope_unknown = false;

    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [_local_ref, local_sha, _remote_ref, remote_sha] = f[..] else {
            continue;
        };
        if local_sha.is_empty() || local_sha.chars().all(|c| c == '0') {
            continue; // nothing pushed, or a ref deletion
        }
        let base = range_base(remote, local_sha, remote_sha);
        let files: Vec<String> = if let Some(b) = &base {
            diff_names(b, local_sha)
        } else {
            // Nothing to diff against: every manifest in the tree is in scope and every dependency in it counts as touched.
            scope_unknown = true;
            ls_tree_manifests(local_sha)
        };
        if files.is_empty() {
            continue;
        }
        if let Some(b) = &base {
            added_lines.push_str(&diff_added_lines(b, local_sha, &files));
        }
        changed_files.extend(files);
    }
    if changed_files.is_empty() {
        return None;
    }
    changed_files.sort_unstable();
    changed_files.dedup();
    Some((changed_files, added_lines, scope_unknown))
}

/// Split the candidates into blocking (touched by this push) and stale (Renovate's department).
fn classify(
    candidates: &[Candidate],
    changed_files: &[String],
    added_lines: &str,
    scope_unknown: bool,
) -> (Vec<String>, Vec<String>) {
    let mut blocked = Vec::new();
    let mut stale = Vec::new();
    for c in candidates {
        if !changed_files.iter().any(|f| f == &c.package_file) {
            continue;
        }
        let line = format!(
            "  {}: {} {} -> {} ({})",
            c.package_file,
            c.dep_name,
            display_current(c),
            display_new(c),
            c.update_type
        );
        if touched(c, added_lines, scope_unknown) {
            blocked.push(line);
        } else {
            stale.push(line);
        }
    }
    (blocked, stale)
}

/// The commit to diff one pushed ref against: the remote tip it updates, or the fork point from the remote's default branch for a brand-new branch.
fn range_base(remote: &str, local_sha: &str, remote_sha: &str) -> Option<String> {
    if !remote_sha.is_empty() && !remote_sha.chars().all(|c| c == '0') {
        return Some(remote_sha.to_owned());
    }
    for r in [
        format!("{remote}/HEAD"),
        format!("{remote}/main"),
        format!("{remote}/master"),
    ] {
        if realgit::succeeds(&["rev-parse", "--verify", "-q", &format!("{r}^{{commit}}")]) {
            return realgit::capture(&["merge-base", local_sha, &r]).map(|s| s.trim().to_owned());
        }
    }
    None
}

fn diff_names(base: &str, tip: &str) -> Vec<String> {
    realgit::capture(&["diff", "--name-only", base, tip])
        .map(|s| {
            s.lines()
                .filter(|l| is_manifest(l))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn ls_tree_manifests(tip: &str) -> Vec<String> {
    realgit::capture(&["ls-tree", "-r", "--name-only", tip])
        .map(|s| {
            s.lines()
                .filter(|l| is_manifest(l))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The `+` side of a zero-context diff over the given files.
fn diff_added_lines(base: &str, tip: &str, files: &[String]) -> String {
    let mut args: Vec<String> = vec![
        "diff".into(),
        "-U0".into(),
        base.into(),
        tip.into(),
        "--".into(),
    ];
    args.extend(files.iter().cloned());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    realgit::capture(&refs)
        .map(|s| {
            s.lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .map(|l| l.strip_prefix('+').unwrap_or(l))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// A dependency is "touched" when an added line carries its exact replaceString, or names it together with its current value or digest.
fn touched(c: &Candidate, added_lines: &str, scope_unknown: bool) -> bool {
    if scope_unknown {
        return true;
    }
    if !c.replace_string.is_empty() && added_lines.contains(&c.replace_string) {
        return true;
    }
    let needle = if c.current_value.is_empty() {
        &c.current_digest
    } else {
        &c.current_value
    };
    if needle.is_empty() {
        return false;
    }
    added_lines
        .lines()
        .any(|l| l.contains(&c.dep_name) && l.contains(needle.as_str()))
}

fn display_current(c: &Candidate) -> String {
    if c.current_value.is_empty() {
        c.current_digest.chars().take(12).collect()
    } else {
        c.current_value.clone()
    }
}

fn display_new(c: &Candidate) -> String {
    if c.new_value.is_empty() {
        c.new_digest.chars().take(12).collect()
    } else {
        c.new_value.clone()
    }
}

/// Run the container lookup for the gate and return the report it wrote.
fn renovate_report() -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!("renovate-gate.{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let report = dir.join("report.json");

    let mut cfg = prepare()?;
    cfg.report_file = Some(report.clone());
    cfg.log_level =
        Some(std::env::var("RENOVATE_LOCAL_LOG_LEVEL").unwrap_or_else(|_| "warn".into()));
    let ok = run_prepared(&cfg, true).is_some_and(|code| code == 0)
        && report.metadata().is_ok_and(|m| m.len() > 0);
    if ok {
        Some(report)
    } else {
        let _ = std::fs::remove_dir_all(&dir);
        None
    }
}

/// The jq program that flattens the report into candidate lines joined by the unit separator (U+001F): unlike a tab it survives empty fields.
const JQ_CANDIDATES: &str = r#"
    ($types | split(" ") | map(select(length > 0))) as $blocking
    | (.repositories.local.packageFiles // {}) | to_entries[] | .value[]
    | .packageFile as $file
    | (.deps // [])[]
    | .depName as $dep | (.currentValue // "") as $cur | (.currentDigest // "") as $dig
    | (.replaceString // "") as $rep
    | (.updates // [])[]
    | (.updateType // .bucket // "") as $type
    | select(($blocking | index($type)) != null)
    | [$file, $dep, $cur, $dig, $rep, $type, (.newValue // ""), (.newDigest // "")]
    | map(tostring | gsub("[\u001f\n]"; " ")) | join("\u001f")
"#;

fn jq_candidates(types: &str, report: &Path) -> Option<String> {
    let out = locate::child("jq")
        .args(["-r", "--arg", "types", types, JQ_CANDIDATES])
        .arg(report)
        .env("PATH", locate::hook_path())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_candidates(out: &str) -> Vec<Candidate> {
    let mut v = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\u{1f}').collect();
        if f.len() != 8 || f[0].is_empty() {
            continue;
        }
        let mk = |i: usize| f.get(i).unwrap_or(&"").to_string();
        v.push(Candidate {
            package_file: mk(0),
            dep_name: mk(1),
            current_value: mk(2),
            current_digest: mk(3),
            replace_string: mk(4),
            update_type: mk(5),
            new_value: mk(6),
            new_digest: mk(7),
        });
    }
    v
}

/// Is this path a manifest Renovate manages?
///
/// A hand-written reading of the manifest list the shell gate spelled as one ERE — same intent, now with a test table instead of a regex nobody can read.
/// Judged case-insensitively (macOS and every ecosystem here already are) and component-wise rather than substring, with two deliberate narrowings of the old regex's unanchored slop: `commise.toml` and `notebook/DockerfileReadme.md` no longer count, and nothing real ever depended on their counting.
pub fn is_manifest(path: &str) -> bool {
    let lowered = path.to_lowercase();
    let comps: Vec<&str> = lowered.split('/').collect();
    let name = comps.last().copied().unwrap_or("");
    let owned_name = name.to_owned();
    let file = Path::new(&owned_name);
    let ext = file
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let stem = file
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let stem_ext = Path::new(&stem)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let exact = [
        "cargo.toml",
        "cargo.lock",
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "go.mod",
        "go.sum",
        "pyproject.toml",
        "libs.versions.toml",
        ".tool-versions",
    ];
    if exact.contains(&name) {
        return true;
    }
    if name == "bun.lock" || name == "bun.lockb" {
        return true;
    }
    if name.starts_with("requirements") && ext == "txt" {
        return true;
    }
    if ext == "gradle" || (ext == "kts" && stem_ext == "gradle") {
        return true;
    }
    // `Dockerfile` and its suffixed variants (Dockerfile.prod), but not words that merely begin with the string.
    if comps.iter().any(|c| {
        *c == "dockerfile"
            || c.strip_prefix("dockerfile")
                .is_some_and(|r| r.starts_with('.'))
    }) {
        return true;
    }
    if ext == "json" && stem.ends_with("devcontainer") {
        return true;
    }
    if ext == "toml"
        && (name == "mise.toml" || name.starts_with(".mise") || name.starts_with("mise"))
    {
        return true;
    }
    if ext == "yml" || ext == "yaml" {
        let n = comps.len();
        if n >= 2 && comps[n - 2] == "workflows" && comps.contains(&".github") {
            return true;
        }
    }
    if ext == "toml" && comps.windows(2).any(|w| w == [".config", "mise"]) {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// The container run itself: `dotguard renovate run`
// ---------------------------------------------------------------------------

/// Everything one `docker run` needs, gathered so the argv is built by a pure function the tests can inspect — this is the security boundary, and a flag nobody can test is a flag that quietly disappears.
pub struct RunConfig {
    pub image: String,
    pub snapshot: PathBuf,
    pub config_present: bool,
    pub report_file: Option<PathBuf>,
    pub log_level: Option<String>,
    pub uid: u32,
    pub gid: u32,
    pub dedicated_token: Option<String>,
    pub passthrough: Vec<String>,
}

const DEFAULT_IMAGE: &str = "renovate/renovate:44.52.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd";

/// The image reference must be an exact renovate/renovate version with a sha256 digest: a caller cannot silently downgrade the immutable boundary to `:latest`.
fn image_is_pinned(image: &str) -> bool {
    let Some(rest) = image.strip_prefix("renovate/renovate:") else {
        return false;
    };
    let Some((version, digest)) = rest.split_once("@sha256:") else {
        return false;
    };
    let mut parts = version.split('.');
    let ok_version = [parts.next(), parts.next(), parts.next()]
        .iter()
        .all(|p| p.is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())))
        && parts.next().is_none();
    ok_version && digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

/// The `docker` argv and the environment removals for one run.
///
/// The process gets no Linux capabilities, cannot gain privileges, sees a read-only root and repository, and gets only isolated tmpfs for writable state.
/// Ambient `GITHUB_TOKEN` / `GITHUB_COM_TOKEN` are stripped; a caller may opt in to a dedicated, least-privilege `RENOVATE_GITHUB_COM_TOKEN`, handed to the container through the environment — never as a command-line argument, where `ps` could read it.
fn docker_plan(c: &RunConfig) -> (Vec<String>, Vec<&'static str>) {
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--user".into(),
        format!("{}:{}", c.uid, c.gid),
        "--read-only".into(),
        "--cap-drop=ALL".into(),
        "--security-opt=no-new-privileges".into(),
        "--tmpfs".into(),
        "/tmp:rw,nodev,nosuid,size=512m".into(),
        "--tmpfs".into(),
        format!(
            "/tmp/containerbase:rw,nodev,nosuid,size=64m,uid={},gid={},mode=0700",
            c.uid, c.gid
        ),
        "--env".into(),
        "HOME=/tmp".into(),
        "--env".into(),
        "RENOVATE_PLATFORM=local".into(),
        "--env".into(),
        "RENOVATE_ONBOARDING=false".into(),
        "--env".into(),
        "RENOVATE_REQUIRE_CONFIG=optional".into(),
    ];

    // Report destination: log by default, JSON file when a path is set.
    // The directory is bind-mounted at /report so the container uid can write it.
    if let Some(path) = &c.report_file {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        args.push("--env".into());
        args.push("RENOVATE_REPORT_TYPE=file".into());
        args.push("--env".into());
        args.push(format!("RENOVATE_REPORT_PATH=/report/{name}"));
        args.push("--volume".into());
        args.push(format!("{}:/report:rw", dir.display()));
    } else {
        args.push("--env".into());
        args.push("RENOVATE_REPORT_TYPE=logging".into());
    }

    if let Some(level) = &c.log_level {
        args.push("--env".into());
        args.push(format!("LOG_LEVEL={level}"));
    }
    // Per-repo override; the non-default filename keeps future bots from mistaking it for their own config.
    if c.config_present {
        args.push("--env".into());
        args.push("RENOVATE_CONFIG_FILE=/repo/renovate.local.json5".into());
    }

    args.push("--volume".into());
    args.push(format!("{}:/repo:ro", c.snapshot.display()));
    args.push("--workdir".into());
    args.push("/repo".into());

    let mut removed: Vec<&'static str> = vec!["GITHUB_TOKEN"];
    if c.dedicated_token.is_some() {
        // docker's --env NAME form marks the variable for inheritance; the value travels through this process's own environment, set by prepare(), and never through argv.
        args.push("--env".into());
        args.push("GITHUB_COM_TOKEN".into());
    } else {
        removed.push("GITHUB_COM_TOKEN");
    }

    args.push(c.image.clone());
    args.extend(c.passthrough.iter().cloned());
    (args, removed)
}

/// Gather the config from the environment and build the staged snapshot.
fn prepare() -> Option<RunConfig> {
    if locate::which("docker").is_none() {
        eprintln!("error: docker is required.");
        return None;
    }
    let root = realgit::capture(&["rev-parse", "--show-toplevel"])?
        .trim()
        .to_owned();
    if root.is_empty() {
        eprintln!("error: run this inside a git repository.");
        return None;
    }

    let image = std::env::var("RENOVATE_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_owned());
    if !image_is_pinned(&image) {
        eprintln!(
            "error: RENOVATE_IMAGE must use an exact renovate/renovate version and sha256 digest."
        );
        return None;
    }

    // A staged snapshot of the index keeps the original .git, ignored files, untracked files and the original worktree outside the container boundary.
    // checkout-index writes what would be committed — exactly what a push-based freshness gate should be judging.
    let snapshot = std::env::temp_dir().join(format!(
        "renovate-local.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&snapshot).ok()?;
    let prefix = format!("{}/", snapshot.display());
    let ok = realgit::succeeds(&[
        "-C",
        &root,
        "checkout-index",
        "--all",
        "--force",
        &format!("--prefix={prefix}"),
    ]) && realgit::succeeds(&[
        "-C",
        &prefix,
        "init",
        "--quiet",
        "--initial-branch=main",
        "--template=",
    ]) && realgit::succeeds(&["-C", &prefix, "add", "--all"]);
    if !ok {
        let _ = std::fs::remove_dir_all(&snapshot);
        return None;
    }

    let config_present = snapshot.join("renovate.local.json5").is_file();
    let (uid, gid) = uid_gid();
    Some(RunConfig {
        image,
        snapshot,
        config_present,
        report_file: std::env::var_os("RENOVATE_LOCAL_REPORT_FILE").map(PathBuf::from),
        log_level: std::env::var("LOG_LEVEL").ok().filter(|s| !s.is_empty()),
        uid,
        gid,
        dedicated_token: std::env::var("RENOVATE_GITHUB_COM_TOKEN")
            .ok()
            .filter(|s| !s.is_empty()),
        passthrough: std::env::args().skip(3).collect(),
    })
}

/// `id -u` / `id -g`: std has no getuid, and two invocations per container run is nothing.
fn uid_gid() -> (u32, u32) {
    let read = |flag: &str| {
        std::process::Command::new("id")
            .arg(flag)
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(if flag == "-u" { 501 } else { 20 })
    };
    (read("-u"), read("-g"))
}

/// `dotguard renovate run` — the manual entry point, same container, no gate.
pub fn run_local() -> i32 {
    let Some(cfg) = prepare() else {
        return 1;
    };
    let code = run_prepared(&cfg, false).unwrap_or(1);
    let _ = std::fs::remove_dir_all(&cfg.snapshot);
    code
}

/// Spawn docker per the plan.
/// `quiet_stdout` keeps the gate's progress on stderr only.
/// Returns the exit code when docker ran.
fn run_prepared(cfg: &RunConfig, quiet_stdout: bool) -> Option<i32> {
    let (args, removed) = docker_plan(cfg);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut cmd = locate::child("docker");
    cmd.args(&refs);
    for r in removed {
        cmd.env_remove(r);
    }
    // The dedicated token rides the environment, not argv.
    if let Some(token) = &cfg.dedicated_token {
        cmd.env("GITHUB_COM_TOKEN", token);
    }
    if quiet_stdout {
        cmd.stdout(Stdio::null()).stderr(Stdio::inherit());
    }
    let status = cmd.status().ok()?;
    Some(status.code().unwrap_or(1))
}

#[cfg(test)]
mod run_tests {
    use super::{RunConfig, docker_plan, image_is_pinned};
    use std::path::PathBuf;

    fn cfg(dedicated_token: Option<&str>, report_file: Option<&str>) -> RunConfig {
        RunConfig {
            image: "renovate/renovate:44.52.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6dbd".into(),
            snapshot: PathBuf::from("/tmp/snap"),
            config_present: false,
            report_file: report_file.map(PathBuf::from),
            log_level: Some("warn".into()),
            uid: 501,
            gid: 20,
            dedicated_token: dedicated_token.map(str::to_owned),
            passthrough: vec![],
        }
    }

    fn argv(c: &RunConfig) -> Vec<String> {
        docker_plan(c).0
    }

    #[test]
    fn the_image_must_be_exactly_pinned() {
        assert!(image_is_pinned(super::DEFAULT_IMAGE));
        assert!(!image_is_pinned("renovate/renovate:latest"));
        assert!(!image_is_pinned("renovate/renovate:44.52.0"));
        assert!(!image_is_pinned(
            "renovate/renovate:44.52.0@sha256:778ff1b404d79ef7763f0c904ebb279a4138ffc1017181ad076b1648e37f6db"
        ));
        assert!(!image_is_pinned("alpine:3.20"));
    }

    #[test]
    fn the_container_is_a_hardened_boundary() {
        let a = argv(&cfg(None, None));
        for flag in [
            "--read-only",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
        ] {
            assert!(a.contains(&flag.to_owned()), "missing {flag}");
        }
        assert!(a.contains(&"--user 501:20".split(' ').next().unwrap().to_owned()));
        let user = a.iter().position(|x| x == "--user").map(|i| &a[i + 1]);
        assert_eq!(user, Some(&"501:20".to_owned()));
        assert!(a.contains(&"/tmp:rw,nodev,nosuid,size=512m".to_owned()));
        assert!(a.contains(
            &"/tmp/containerbase:rw,nodev,nosuid,size=64m,uid=501,gid=20,mode=0700".to_owned()
        ));
        assert!(a.contains(&"HOME=/tmp".to_owned()));
        assert!(a.contains(&"RENOVATE_PLATFORM=local".to_owned()));
        assert!(a.contains(&"RENOVATE_ONBOARDING=false".to_owned()));
        assert!(a.contains(&"RENOVATE_REQUIRE_CONFIG=optional".to_owned()));
    }

    #[test]
    fn the_repository_is_mounted_read_only_from_a_snapshot() {
        let a = argv(&cfg(None, None));
        assert!(a.contains(&"/tmp/snap:/repo:ro".to_owned()));
        assert!(a.contains(&"/repo".to_owned()));
        assert!(
            !a.iter()
                .any(|x| x.ends_with(":/repo") && !x.ends_with(":/repo:ro"))
        );
    }

    #[test]
    fn a_report_file_switches_the_mount_and_the_mode() {
        let a = argv(&cfg(None, Some("/tmp/report/report.json")));
        assert!(a.contains(&"RENOVATE_REPORT_TYPE=file".to_owned()));
        assert!(a.contains(&"RENOVATE_REPORT_PATH=/report/report.json".to_owned()));
        assert!(a.contains(&"/tmp/report:/report:rw".to_owned()));
        assert!(!a.contains(&"RENOVATE_REPORT_TYPE=logging".to_owned()));
    }

    #[test]
    fn ambient_github_tokens_never_reach_the_container() {
        // No dedicated token: both ambient variables are removed, and neither name appears anywhere in argv.
        let (a, removed) = docker_plan(&cfg(None, None));
        assert!(removed.contains(&"GITHUB_TOKEN"));
        assert!(removed.contains(&"GITHUB_COM_TOKEN"));
        assert!(!a.iter().any(|x| x.contains("GITHUB")));

        // Dedicated token: forwarded by name only, ambient GITHUB_TOKEN still removed, and the value never appears in argv.
        let (a, removed) = docker_plan(&cfg(Some("dedicated-read-token"), None));
        assert!(removed.contains(&"GITHUB_TOKEN"));
        assert!(!removed.contains(&"GITHUB_COM_TOKEN"));
        let pos = a
            .iter()
            .position(|x| x == "GITHUB_COM_TOKEN")
            .map(|i| &a[i - 1]);
        assert_eq!(
            pos,
            Some(&"--env".to_owned()),
            "passed by --env NAME, not NAME=VALUE"
        );
        assert!(!a.iter().any(|x| x.contains("dedicated-read-token")));
    }

    #[test]
    fn a_per_repo_override_is_only_mounted_through_the_repo() {
        let mut c = cfg(None, None);
        c.config_present = true;
        let a = argv(&c);
        assert!(a.contains(&"RENOVATE_CONFIG_FILE=/repo/renovate.local.json5".to_owned()));
        assert!(a.contains(&"/tmp/snap:/repo:ro".to_owned()));
    }
}

/// The credential boundary this repository keeps around the personal gh token, ported from the integration test that used to stub `docker`: nothing that shapes a general shell may read it, and the one script allowed to read it may only do so in a shape that cannot become ambient.
#[cfg(test)]
mod credential_boundary {
    use std::path::PathBuf;

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn read(rel: &str) -> String {
        std::fs::read_to_string(repo().join(rel)).unwrap_or_default()
    }

    #[test]
    fn no_general_shell_exports_the_gh_token() {
        assert!(!repo().join("dot_config/shell/github-token.sh").exists());
        let remove = read(".chezmoiremove");
        assert!(
            remove
                .lines()
                .any(|l| l.trim() == ".config/shell/github-token.sh"),
            "retired GitHub token exporter is not removed from existing hosts"
        );
        for rel in [
            "dot_zshrc.tmpl",
            "dot_bashrc",
            "dot_claude/executable_bashenv.sh",
        ] {
            assert!(
                !read(rel).contains("gh auth token"),
                "personal gh token is still exported by {rel}"
            );
        }
    }

    #[test]
    fn install_tools_reads_the_token_only_as_a_command_prefix() {
        let text = read("run_onchange_after_install-tools.sh.tmpl");
        for line in text.lines().filter(|l| l.contains("gh auth token")) {
            let t = line.trim();
            assert!(
                t.contains("gh auth status") || t.starts_with("github_token="),
                "install-tools reads the gh token in an unapproved shape: {t}"
            );
        }
        for line in text.lines() {
            let t = line.trim_start();
            assert!(
                !(t.starts_with("export ") && t.contains("GITHUB_TOKEN")),
                "install-tools exports a GitHub token into the environment"
            );
        }
        for line in text.lines().filter(|l| l.contains("MISE_GITHUB_TOKEN")) {
            let t = line.trim();
            assert!(
                t.starts_with("MISE_GITHUB_TOKEN=") && t.contains(" mise "),
                "MISE_GITHUB_TOKEN is used outside a mise command prefix: {t}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Candidate, is_manifest, parse_candidates, touched};

    #[test]
    fn manifests_are_recognized_component_wise() {
        let yes = [
            "Cargo.toml",
            "Cargo.lock",
            "crates/foo/Cargo.toml",
            "package.json",
            "package-lock.json",
            "pnpm-lock.yaml",
            "bun.lock",
            "bun.lockb",
            "go.mod",
            "go.sum",
            "pyproject.toml",
            "requirements.txt",
            "requirements-dev.txt",
            "gradle/libs.versions.toml",
            "build.gradle",
            "build.gradle.kts",
            "Dockerfile",
            "apps/api/Dockerfile",
            "apps/api/Dockerfile.prod",
            ".github/workflows/ci.yml",
            ".github/workflows/release.yaml",
            "mise.toml",
            ".mise.toml",
            ".mise.dev.toml",
            ".config/mise/config.toml",
            ".tool-versions",
            ".devcontainer/devcontainer.json",
        ];
        for p in yes {
            assert!(is_manifest(p), "should match: {p}");
        }

        let no = [
            "README.md",
            "src/main.rs",
            "mygo.mod",
            "requirements.d/notes.md",
            "notebook/DockerfileReadme.md",
            "docs/workflows/ci.yml", // no .github component
            "commise.toml",
            "miseo.toml/backups",
            "app.devcontainer.jsonx",
        ];
        for p in no {
            assert!(!is_manifest(p), "should not match: {p}");
        }
    }

    #[test]
    fn candidates_parse_from_unit_separated_lines() {
        let out =
            "Cargo.toml\u{1f}serde\u{1f}1.0\u{1f}\u{1f}serde = \"1.0\"\u{1f}minor\u{1f}1.1\u{1f}\n";
        let c = &parse_candidates(out)[0];
        assert_eq!(c.package_file, "Cargo.toml");
        assert_eq!(c.dep_name, "serde");
        assert_eq!(c.update_type, "minor");
        assert_eq!(c.new_value, "1.1");
        assert!(c.replace_string.contains("serde"));
        assert!(parse_candidates("garbage without separators").is_empty());
    }

    fn cand(rep: &str, cur: &str, dep: &str) -> Candidate {
        Candidate {
            package_file: "Cargo.toml".into(),
            dep_name: dep.into(),
            current_value: cur.into(),
            current_digest: String::new(),
            replace_string: rep.into(),
            update_type: "minor".into(),
            new_value: "9.9".into(),
            new_digest: String::new(),
        }
    }

    #[test]
    fn touched_means_the_added_lines_name_the_dep_with_its_version() {
        let added = "serde = \"1.0\"\n anyhow = \"1\"\n";
        assert!(touched(
            &cand("serde = \"1.0\"", "1.0", "serde"),
            added,
            false
        ));
        assert!(
            !touched(&cand("", "2.0", "serde"), added, false),
            "dep present but old value"
        );
        assert!(
            touched(&cand("", "1", "anyhow"), added, false),
            "name + value on the same line"
        );
        assert!(
            touched(&cand("", "1.0", "serde"), "", true),
            "unknown scope counts all as touched"
        );
    }
}
