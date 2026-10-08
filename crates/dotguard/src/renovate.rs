//! The dependency-freshness gate: a push that touches a dependency must leave it on the current non-major release.
//!
//! The facts come from a pinned read-only Renovate container, which `dotguard renovate run` also starts by hand.
//!
//! The gate refuses a push whose range adds or changes a manifest line of a dependency's own manifest with a non-major update available, reading the offered commit.
//! The default update types cover minor, patch, pin, and `pinDigest`, and `RENOVATE_LOCAL_UPDATE_TYPES` overrides them, with `RENOVATE_LOCAL_GATE_TYPES` read as the name before the rename.
//! Everything else only informs:
//!
//! * Major updates never block, because deferring them stays a valid decision.
//!
//! Missing tools, malformed reports, and failed lookups refuse the push with a structured next action.
//!
//! Opt out with `git config renovate.localSkip true`, adding `--global` for the whole machine, or once with `RENOVATE_LOCAL_SKIP=1 git push`.

use crate::locate;
use crate::real_git;
use crate::refusal::Refusal;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const DEFAULT_UPDATE_TYPES: &str = "minor patch pin pinDigest";

/// The exit status of a subcommand the gate doesn't serve.
const USAGE_ERROR: i32 = 2;
/// The digest digits an update shows when it names no version.
const SHORT_DIGEST: usize = 12;
/// The image's tool directory, which Renovate writes inside its read-only container.
const CONTAINERBASE: &str = "/tmp/containerbase";
/// `dotguard renovate run` precedes the arguments that pass through to Renovate.
const PASSTHROUGH_AFTER: usize = 3;
/// The first user's and the `staff` group's identifiers on macOS, when `id` can't tell.
const DEFAULT_UID: u32 = 501;
const DEFAULT_GID: u32 = 20;

/// One dependency with a blocking update available, as parsed from the jq report.
struct Update {
    package_file: String,
    dep_name: String,
    current_value: String,
    current_digest: String,
    replace_string: String,
    kind: String,
    new_value: String,
    new_digest: String,
    breaking: bool,
}

pub fn dispatch(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("run") {
        run_local()
    } else {
        eprintln!("usage: dotguard renovate run");
        USAGE_ERROR
    }
}

pub fn run(remote: &str) -> i32 {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return unavailable("could not read the offered revisions");
    }
    gate(remote, &input)
}

fn unavailable(cause: &str) -> i32 {
    let verdict = rules::freshness_rules::verdict(rules::freshness_rules::Facts {
        needed: rules::freshness_rules::Need::Required,
        evidence: false,
        breaking: false,
        touched: false,
    });
    if verdict == rules::freshness_rules::Verdict::MissingEvidence {
        Refusal::new("push.evidence", cause, "dotguard renovate run")
            .evidence(cause)
            .emit();
        1
    } else {
        0
    }
}

fn gate(remote: &str, input: &str) -> i32 {
    for line in input.lines() {
        let result = gate_ref(remote, line);
        if result != 0 {
            return result;
        }
    }
    0
}

fn gate_ref(remote: &str, input: &str) -> i32 {
    if real_git::capture(&["rev-parse", "--show-toplevel"]).is_none() {
        return unavailable("Git could not read the repository root");
    }

    if real_git::capture(&["config", "--type=bool", "--get", "renovate.localSkip"])
        .is_some_and(|v| v.trim() == "true")
    {
        return 0;
    }
    if std::env::var_os("RENOVATE_LOCAL_SKIP").is_some_and(|v| !v.is_empty()) {
        return 0;
    }

    let found = match scope(remote, input) {
        Ok(Some(found)) => found,
        Ok(None) => return 0,
        Err(cause) => return unavailable(&cause),
    };

    for tool in ["docker", "jq"] {
        if locate::which(tool).is_none() {
            return unavailable(&format!("required tool {tool} is missing"));
        }
    }
    eprintln!(
        ">>> renovate gate: dependency manifests changed — checking the touched dependencies..."
    );

    let report = renovate_report(found.offered.as_deref());
    let Some(report) = report else {
        return unavailable("Renovate did not produce verified evidence for the offered commit");
    };

    let types = std::env::var("RENOVATE_LOCAL_UPDATE_TYPES")
        .or_else(|_| std::env::var("RENOVATE_LOCAL_GATE_TYPES"))
        .unwrap_or_else(|_| DEFAULT_UPDATE_TYPES.to_owned());
    let out = jq_updates(&types, &report);
    if let Some(dir) = report.parent() {
        platform::cleanup::remove_dir_all(dir);
    }
    let Some(out) = out else {
        return unavailable("the required report could not be validated");
    };
    if out.lines().any(|line| {
        let fields: Vec<_> = line.split('\u{1f}').collect();
        fields.len() != UPDATE_FIELDS
            || fields.first().is_none_or(|file| file.is_empty())
            || !matches!(fields.last(), Some(&"true" | &"false"))
    }) {
        return unavailable("the dependency report contains malformed compatibility evidence");
    }
    let (blocked, outdated) = classify(&parse_updates(&out), &found);

    if !outdated.is_empty() {
        eprintln!(
            "::notice:: outdated but untouched in this push (left for Renovate):\n{}",
            outdated.join("\n")
        );
    }
    if blocked.is_empty() {
        eprintln!("renovate gate: touched dependencies are current");
        return 0;
    }

    outdated_refusal(&blocked).emit();
    1
}

/// The refusal records no opt-out, so it names no bypass.
/// The cause lists them for the owner.
fn outdated_refusal(blocked: &[String]) -> Refusal {
    let mut refusal = Refusal::new(
        "push.dependencies",
        "this push changes dependencies that are not on their latest non-major release.\n\
         Bump them to the listed versions (SHA-pinned actions: update both the hash and the version comment), then push again.\n\
         Unrecorded opt-outs for the owner: RENOVATE_LOCAL_SKIP=1 for one push, git config renovate.localSkip true for this repository, or RENOVATE_LOCAL_UPDATE_TYPES to narrow what blocks.",
        "dotguard renovate run",
    );
    for dependency in blocked {
        refusal = refusal.evidence(dependency.trim());
    }
    refusal
}

struct Found {
    changed_files: Vec<String>,
    added_by_file: BTreeMap<String, String>,
    offered: Option<String>,
    scope_unknown: bool,
}

/// The manifest gate over one push.
/// `None` when the push touches nothing Renovate manages, the common case, which must cost nothing.
fn scope(remote: &str, input: &str) -> Result<Option<Found>, String> {
    let mut changed_files: Vec<String> = Vec::new();
    let mut added_by_file: BTreeMap<String, String> = BTreeMap::new();
    let mut offered: Option<String> = None;
    let mut scope_unknown = false;

    for line in input.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [_local_ref, local_sha, _remote_ref, remote_sha] = f[..] else {
            continue;
        };
        if local_sha.is_empty() || local_sha.chars().all(|c| c == '0') {
            continue; // nothing pushed, or a ref deletion
        }
        offered = offered.or_else(|| Some((*local_sha).to_owned()));
        if !real_git::succeeds(&["cat-file", "-e", &format!("{local_sha}^{{commit}}")]) {
            return Err("the offered commit cannot be read".to_owned());
        }
        let base = range_base(remote, local_sha, remote_sha)?;
        let files: Vec<String> = if let Some(b) = &base {
            diff_names(b, local_sha)?
        } else {
            // Without a base commit, every manifest in the tree counts as in scope and every dependency as touched.
            scope_unknown = true;
            ls_tree_manifests(local_sha)?
        };
        if files.is_empty() {
            continue;
        }
        if let Some(b) = &base {
            for (file, added) in diff_added_lines(b, local_sha, &files)? {
                added_by_file
                    .entry(file)
                    .and_modify(|lines| {
                        lines.push('\n');
                        lines.push_str(&added);
                    })
                    .or_insert(added);
            }
        }
        changed_files.extend(files);
    }
    if changed_files.is_empty() {
        return Ok(None);
    }
    changed_files.sort_unstable();
    changed_files.dedup();
    Ok(Some(Found {
        changed_files,
        added_by_file,
        offered,
        scope_unknown,
    }))
}

/// Splits the updates into blocking ones, which this push touched, and outdated ones, which stay with Renovate.
fn classify(updates: &[Update], found: &Found) -> (Vec<String>, Vec<String>) {
    let mut blocked = Vec::new();
    let mut outdated = Vec::new();
    for c in updates {
        if !found.changed_files.iter().any(|f| f == &c.package_file) {
            continue;
        }
        let line = format!(
            "  {}: {} {} -> {} ({})",
            c.package_file,
            c.dep_name,
            display_current(c),
            display_new(c),
            c.kind
        );
        if rules::freshness_rules::verdict(rules::freshness_rules::Facts {
            needed: rules::freshness_rules::Need::Required,
            evidence: true,
            breaking: c.breaking,
            touched: touched(
                c,
                found.added_by_file.get(&c.package_file).map(String::as_str),
                found.scope_unknown,
            ),
        }) == rules::freshness_rules::Verdict::Outdated
        {
            blocked.push(line);
        } else {
            outdated.push(line);
        }
    }
    (blocked, outdated)
}

/// The diff base for one pushed ref: the remote tip it updates, or the fork point from the default branch for a new branch.
/// Ask the remote which branch `HEAD` names for a new branch.
/// A cached `refs/remotes/<remote>/HEAD` can name a branch the remote no longer uses.
fn range_base(remote: &str, local_sha: &str, remote_sha: &str) -> Result<Option<String>, String> {
    if !remote_sha.is_empty() && !remote_sha.chars().all(|c| c == '0') {
        return Ok(Some(remote_sha.to_owned()));
    }
    let Some(tip) = default_branch_tip(remote)? else {
        return Ok(None);
    };
    if !real_git::succeeds(&["cat-file", "-e", &format!("{tip}^{{commit}}")])
        && !real_git::succeeds(&["fetch", "--no-tags", remote, &tip])
    {
        return Err("the remote default branch commit cannot be read".to_owned());
    }
    Ok(real_git::capture(&["merge-base", local_sha, &tip]).map(|value| value.trim().to_owned()))
}

fn default_branch_tip(remote: &str) -> Result<Option<String>, String> {
    let listing = real_git::capture(&["ls-remote", "--symref", remote, "HEAD"])
        .ok_or_else(|| "the actual remote default branch is unavailable".to_owned())?;
    if listing.trim().is_empty() {
        return Ok(None);
    }
    for line in listing.lines() {
        if let Some((sha, "HEAD")) = line.split_once('\t')
            && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Ok(Some(sha.to_owned()));
        }
    }
    Err("the remote default branch response is malformed".to_owned())
}

fn diff_names(base: &str, tip: &str) -> Result<Vec<String>, String> {
    real_git::capture(&["diff", "--name-only", base, tip])
        .map(|s| {
            s.lines()
                .filter(|l| is_manifest(l))
                .map(str::to_owned)
                .collect()
        })
        .ok_or_else(|| "Git could not inspect the offered revision".to_owned())
}

fn ls_tree_manifests(tip: &str) -> Result<Vec<String>, String> {
    real_git::capture(&["ls-tree", "-r", "--name-only", tip])
        .map(|s| {
            s.lines()
                .filter(|l| is_manifest(l))
                .map(str::to_owned)
                .collect()
        })
        .ok_or_else(|| "Git could not inspect the offered revision".to_owned())
}

/// The `+` side of a zero-context diff, per file it covers.
fn diff_added_lines(
    base: &str,
    tip: &str,
    files: &[String],
) -> Result<Vec<(String, String)>, String> {
    let mut args: Vec<String> = vec![
        "diff".into(),
        "-U0".into(),
        base.into(),
        tip.into(),
        "--".into(),
    ];
    args.extend(files.iter().cloned());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let diff = real_git::capture(&refs)
        .ok_or_else(|| "Git could not inspect the offered revision".to_owned())?;
    let mut out: Vec<(String, String)> = Vec::new();
    let mut file = String::new();
    for line in diff.lines() {
        if let Some(target) = line.strip_prefix("+++ b/") {
            target.clone_into(&mut file);
            continue;
        }
        if let Some(added) = line.strip_prefix('+').filter(|_| !line.starts_with("+++")) {
            if let Some(last) = out.last_mut().filter(|(path, _)| *path == file) {
                last.1.push('\n');
                last.1.push_str(added);
            } else {
                out.push((file.clone(), added.to_owned()));
            }
        }
    }
    Ok(out)
}

/// An added line of the dependency's own manifest touches it when it carries its exact `replaceString`, or names it with its current value or digest.
/// A dependency never borrows change lines from another manifest.
fn touched(c: &Update, added_lines: Option<&str>, scope_unknown: bool) -> bool {
    if scope_unknown {
        return true;
    }
    let Some(added_lines) = added_lines else {
        return false;
    };
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

fn display_current(c: &Update) -> String {
    if c.current_value.is_empty() {
        c.current_digest.chars().take(SHORT_DIGEST).collect()
    } else {
        c.current_value.clone()
    }
}

fn display_new(c: &Update) -> String {
    if c.new_value.is_empty() {
        c.new_digest.chars().take(SHORT_DIGEST).collect()
    } else {
        c.new_value.clone()
    }
}

/// Run the container lookup for the gate and return the report it wrote.
/// `offered` names the first commit the push offers and binds the report to that content.
/// `None` reads the index.
fn renovate_report(offered: Option<&str>) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!("renovate-gate.{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let report = dir.join("report.json");

    let mut cfg = prepare(offered)?;
    cfg.report_file = Some(report.clone());
    cfg.log_level =
        Some(std::env::var("RENOVATE_LOCAL_LOG_LEVEL").unwrap_or_else(|_| "warn".into()));
    let ok = run_prepared(&cfg, true).is_some_and(|code| code == 0)
        && report.metadata().is_ok_and(|m| m.len() > 0);
    platform::cleanup::remove_dir_all(&cfg.snapshot);
    if ok {
        Some(report)
    } else {
        platform::cleanup::remove_dir_all(&dir);
        None
    }
}

/// The jq program that flattens the report into candidate lines joined by the unit separator `U+001F`, which keeps empty fields intact.
const JQ_UPDATES: &str = r#"
    ($types | split(" ") | map(select(length > 0))) as $blocking
    | if (.repositories.local.packageFiles | type) != "object" then error("missing packageFiles") else .repositories.local.packageFiles end | to_entries[] | .value[]
    | .packageFile as $file
    | (.deps // [])[]
    | .depName as $dep | (.currentValue // "") as $cur | (.currentDigest // "") as $dig
    | (.replaceString // "") as $rep
    | (.updates // [])[]
    | (.updateType // .bucket // "") as $type
    | select(($blocking | index($type)) != null)
    | [$file, $dep, $cur, $dig, $rep, $type, (.newValue // ""), (.newDigest // ""), (.isBreaking // false)]
    | map(tostring | gsub("[\u001f\n]"; " ")) | join("\u001f")
"#;

fn jq_updates(types: &str, report: &Path) -> Option<String> {
    let out = locate::child("jq")
        .args(["-r", "--arg", "types", types, JQ_UPDATES])
        .arg(report)
        .env("PATH", locate::hook_path())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The report carries eight dependency fields and its compatibility classification.
const UPDATE_FIELDS: usize = 9;

fn parse_updates(out: &str) -> Vec<Update> {
    let mut v = Vec::new();
    for line in out.lines() {
        let fields: Vec<&str> = line.split('\u{1f}').collect();
        let [
            package_file,
            dep_name,
            current_value,
            current_digest,
            replace_string,
            update_type,
            new_value,
            new_digest,
            breaking,
        ] = fields[..]
        else {
            continue;
        };
        if package_file.is_empty() {
            continue;
        }
        v.push(Update {
            package_file: package_file.to_owned(),
            dep_name: dep_name.to_owned(),
            current_value: current_value.to_owned(),
            current_digest: current_digest.to_owned(),
            replace_string: replace_string.to_owned(),
            kind: update_type.to_owned(),
            new_value: new_value.to_owned(),
            new_digest: new_digest.to_owned(),
            breaking: breaking == "true",
        });
    }
    v
}

/// Reports whether Renovate manages this path.
pub fn is_manifest(path: &str) -> bool {
    let lowered = path.to_lowercase();
    if matches!(
        lowered.as_str(),
        "home/.chezmoidata/versions.toml" | "home/.chezmoidata/modern.toml"
    ) {
        return true;
    }
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
    // `Dockerfile` and suffixed variants such as `Dockerfile.prod`, but no other word that starts with it.
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
    if (ext == "yml" || ext == "yaml")
        && matches!(comps.as_slice(), [.., "workflows", _])
        && comps.contains(&".github")
    {
        return true;
    }
    if ext == "toml"
        && comps
            .iter()
            .zip(comps.iter().skip(1))
            .any(|pair| pair == (&".config", &"mise"))
    {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// The container run itself: `dotguard renovate run`
// ---------------------------------------------------------------------------

/// Everything one `docker run` needs, so a pure function builds the arguments and tests can inspect this security boundary.
#[derive(Debug)]
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

/// The image reference must name an exact `renovate/renovate` version with a sha256 digest, so no caller can fall back to `:latest`.
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
    ok_version
        && digest.len() == rules::skill_rules::DIGEST_LENGTH
        && digest.chars().all(|c| c.is_ascii_hexdigit())
}

/// The `docker` argv and the environment removals for one run.
///
/// The process gets no Linux capabilities and no privilege escalation, a read-only root and repository, and isolated tmpfs mounts for writable state.
/// The run strips ambient `GITHUB_TOKEN` and `GITHUB_COM_TOKEN`.
/// A caller may opt in to a dedicated, least-privilege `RENOVATE_GITHUB_COM_TOKEN`, passed through the environment and never as an argument visible to `ps`.
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
            "{CONTAINERBASE}:rw,nodev,nosuid,size=64m,uid={},gid={},mode=0700",
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

    // Report destination: the log by default, or a JSON file when a path exists.
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
    // Per-repository override, with a filename that no other bot reads as its own config.
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
        // `--env NAME` passes the variable by name, so its value travels through the environment that `prepare()` sets and never through the arguments.
        args.push("--env".into());
        args.push("GITHUB_COM_TOKEN".into());
    } else {
        removed.push("GITHUB_COM_TOKEN");
    }

    args.push(c.image.clone());
    args.extend(c.passthrough.iter().cloned());
    (args, removed)
}

/// Gather the config from the environment and build the snapshot.
/// `offered` names the commit the gate reads instead of the index, so the report describes exactly what the push would publish.
fn prepare(offered: Option<&str>) -> Option<RunConfig> {
    if locate::which("docker").is_none() {
        eprintln!("error: docker is required.");
        return None;
    }
    let root = real_git::capture(&["rev-parse", "--show-toplevel"])?
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

    let snapshot = checkout_snapshot(&root, offered)?;

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
        passthrough: std::env::args().skip(PASSTHROUGH_AFTER).collect(),
    })
}

fn checkout_snapshot(root: &str, offered: Option<&str>) -> Option<PathBuf> {
    // A snapshot of the offered commit, or of the index for a manual run, keeps the original .git, ignored files, untracked files and the original worktree outside the container boundary.
    let snapshot = std::env::temp_dir().join(format!(
        "renovate-local.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&snapshot).ok()?;
    let prefix = format!("{}/", snapshot.display());
    // The offered commit reaches checkout-index through a temporary index, so the repository's own index stays untouched.
    let offered_index = snapshot.join(".gate-index");
    let staged = match offered {
        Some(sha) => {
            let index = offered_index.to_string_lossy().into_owned();
            real_git::succeeds_with_env("GIT_INDEX_FILE", &index, &["-C", root, "read-tree", sha])
                && real_git::succeeds_with_env(
                    "GIT_INDEX_FILE",
                    &index,
                    &[
                        "-C",
                        root,
                        "checkout-index",
                        "--all",
                        "--force",
                        &format!("--prefix={prefix}"),
                    ],
                )
        }
        None => real_git::succeeds(&[
            "-C",
            root,
            "checkout-index",
            "--all",
            "--force",
            &format!("--prefix={prefix}"),
        ]),
    };
    if offered.is_some() {
        platform::cleanup::remove_file(&offered_index);
    }
    let ok = staged
        && real_git::succeeds(&[
            "-C",
            &prefix,
            "init",
            "--quiet",
            "--initial-branch=main",
            "--template=",
        ])
        && real_git::succeeds(&["-C", &prefix, "add", "--all"]);
    if !ok {
        platform::cleanup::remove_dir_all(&snapshot);
        return None;
    }
    Some(snapshot)
}

fn uid_gid() -> (u32, u32) {
    let read = |flag: &str| {
        crate::locate::child("id")
            .arg(flag)
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(if flag == "-u" {
                DEFAULT_UID
            } else {
                DEFAULT_GID
            })
    };
    (read("-u"), read("-g"))
}

pub fn run_local() -> i32 {
    let Some(cfg) = prepare(None) else {
        return 1;
    };
    let code = run_prepared(&cfg, false).unwrap_or(1);
    platform::cleanup::remove_dir_all(&cfg.snapshot);
    code
}

/// Spawn docker per the plan.
fn run_prepared(cfg: &RunConfig, quiet_stdout: bool) -> Option<i32> {
    let (args, removed) = docker_plan(cfg);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut cmd = locate::child("docker");
    cmd.args(&refs);
    for r in removed {
        cmd.env_remove(r);
    }
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

/// The credential boundary around the personal gh token: no general shell setup may read it, and the one allowed script reads it only in a form that never becomes ambient.
#[cfg(test)]
mod credential_boundary {
    use std::path::PathBuf;

    /// The chezmoi source state, which `.chezmoiroot` places under `home/`.
    fn source() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../home")
    }

    fn read(rel: &str) -> String {
        std::fs::read_to_string(source().join(rel)).unwrap_or_default()
    }

    #[test]
    fn no_general_shell_exports_the_gh_token() {
        assert!(!source().join("dot_config/shell/github-token.sh").exists());
        let retired = read(".chezmoidata/retired.toml");
        assert!(
            retired
                .lines()
                .any(|l| l.trim().starts_with("\".config/shell/github-token.sh\" = ")),
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
        let launchers = std::fs::read_dir(source().join(".chezmoiscripts/unix"))
            .expect("the unix launchers")
            .map(|entry| {
                entry
                    .expect("a launcher")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            });
        let tools = launchers
            .into_iter()
            .find(|name| name.ends_with("-tools.sh.tmpl"))
            .expect("the tools launcher");
        let text = read(&format!(".chezmoiscripts/unix/{tools}"));
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
    use super::{Update, is_manifest, parse_updates, touched};
    use std::collections::BTreeMap;

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
            "home/.chezmoidata/versions.toml",
            "home/.chezmoidata/modern.toml",
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
    fn an_outdated_dependency_refusal_lists_each_dependency_and_no_bypass() {
        let refusal = super::outdated_refusal(&[
            "  package.json: lodash 4.17.20 -> 4.17.21 (patch)".to_owned(),
            "  Cargo.toml: serde 1.0.1 -> 1.0.2 (patch)".to_owned(),
        ]);
        assert!(refusal.is_complete(), "{refusal:?}");
        assert_eq!(refusal.rule, "push.dependencies");
        assert_eq!(refusal.evidence.len(), 2);
        assert_eq!(refusal.bypass, None);
    }

    #[test]
    fn updates_parse_from_unit_separated_lines() {
        let out = "Cargo.toml\u{1f}serde\u{1f}1.0\u{1f}\u{1f}serde = \"1.0\"\u{1f}minor\u{1f}1.1\u{1f}\u{1f}false\n";
        let c = &parse_updates(out)[0];
        assert_eq!(c.package_file, "Cargo.toml");
        assert_eq!(c.dep_name, "serde");
        assert_eq!(c.kind, "minor");
        assert_eq!(c.new_value, "1.1");
        assert!(c.replace_string.contains("serde"));
        assert!(parse_updates("garbage without separators").is_empty());
    }

    fn cand(rep: &str, cur: &str, dep: &str) -> Update {
        Update {
            package_file: "Cargo.toml".into(),
            dep_name: dep.into(),
            current_value: cur.into(),
            current_digest: String::new(),
            replace_string: rep.into(),
            kind: "minor".into(),
            new_value: "9.9".into(),
            new_digest: String::new(),
            breaking: false,
        }
    }

    #[test]
    fn touched_means_the_added_lines_name_the_dep_with_its_version() {
        let added = "serde = \"1.0\"\n anyhow = \"1\"\n";
        assert!(touched(
            &cand("serde = \"1.0\"", "1.0", "serde"),
            Some(added),
            false
        ));
        assert!(
            !touched(&cand("", "2.0", "serde"), Some(added), false),
            "dep present but old value"
        );
        assert!(
            touched(&cand("", "1", "anyhow"), Some(added), false),
            "name + value on the same line"
        );
        assert!(
            touched(&cand("", "1.0", "serde"), None, true),
            "unknown scope counts all as touched"
        );
        assert!(
            !touched(&cand("", "1.0", "serde"), None, false),
            "no added lines of its own manifest leaves the dependency untouched"
        );
    }

    #[test]
    fn a_dependency_never_borrows_change_lines_from_another_manifest() {
        let mut added_by_file = BTreeMap::new();
        added_by_file.insert("package.json".to_owned(), "serde = \"1.0\"".to_owned());
        let mut other = cand("", "1.0", "serde");
        other.package_file = "Cargo.toml".into();
        assert!(
            !touched(
                &other,
                added_by_file.get("Cargo.toml").map(String::as_str),
                false
            ),
            "the added line of package.json cannot touch a Cargo.toml dependency"
        );
        assert!(
            touched(
                &cand("", "1.0", "serde"),
                added_by_file.get("package.json").map(String::as_str),
                false
            ),
            "the added line of its own manifest touches the dependency"
        );
    }

    #[test]
    fn a_new_branch_asks_the_remote_which_branch_head_names() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        let git = |arguments: &[&str]| {
            let mut words = vec!["-C", root];
            words.extend(arguments);
            crate::real_git::capture(&words).unwrap().trim().to_owned()
        };
        git(&["init", "--bare", "--initial-branch=main", "--template="]);
        git(&["config", "user.name", "Fixture"]);
        git(&["config", "user.email", "fixture@example.invalid"]);
        git(&["config", "commit.gpgsign", "false"]);
        let tree = git(&["mktree"]);
        let main = git(&["commit-tree", &tree, "-m", "main"]);
        let trunk = git(&["commit-tree", &tree, "-p", &main, "-m", "trunk"]);
        git(&["update-ref", "refs/heads/main", &main]);
        git(&["update-ref", "refs/heads/trunk", &trunk]);
        assert_eq!(super::default_branch_tip(root).unwrap(), Some(main));
        git(&["symbolic-ref", "HEAD", "refs/heads/trunk"]);
        assert_eq!(super::default_branch_tip(root).unwrap(), Some(trunk));
        assert!(super::default_branch_tip(&format!("{root}/missing")).is_err());
    }

    #[test]
    fn freshness_checks_the_offered_tree_and_keeps_the_callers_index() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_str().unwrap();
        let git = |arguments: &[&str]| {
            let mut words = vec!["-C", root];
            words.extend(arguments);
            crate::real_git::capture(&words).unwrap().trim().to_owned()
        };
        git(&["init", "--initial-branch=main", "--template="]);
        git(&["config", "user.name", "Fixture"]);
        git(&["config", "user.email", "fixture@example.invalid"]);
        git(&["config", "commit.gpgsign", "false"]);
        let manifest = directory.path().join("Cargo.toml");
        std::fs::write(&manifest, "offered").unwrap();
        git(&["add", "Cargo.toml"]);
        let tree = git(&["write-tree"]);
        let offered = git(&["commit-tree", &tree, "-m", "offered"]);
        std::fs::write(&manifest, "staged").unwrap();
        git(&["add", "Cargo.toml"]);
        std::fs::write(&manifest, "working").unwrap();
        std::fs::write(directory.path().join("untracked-secret"), "private").unwrap();
        let index_path = directory.path().join(".git/index");
        let index = std::fs::read(&index_path).unwrap();
        let snapshot = super::checkout_snapshot(root, Some(&offered)).unwrap();
        assert_eq!(
            std::fs::read_to_string(snapshot.join("Cargo.toml")).unwrap(),
            "offered"
        );
        assert!(!snapshot.join("untracked-secret").exists());
        assert_eq!(std::fs::read(&index_path).unwrap(), index);
        assert_eq!(std::fs::read_to_string(manifest).unwrap(), "working");
        platform::cleanup::remove_dir_all(&snapshot);
        let staged = super::checkout_snapshot(root, None).unwrap();
        assert_eq!(
            std::fs::read_to_string(staged.join("Cargo.toml")).unwrap(),
            "staged"
        );
        platform::cleanup::remove_dir_all(&staged);
    }
}
