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
//! A `uses-with` input that holds a `${{ matrix.* }}` expression names no version, so it needs no lookup.
//!
//! GitHub lookups need a token: `RENOVATE_GITHUB_COM_TOKEN` when the environment sets it, or else the same secret from the Doppler project and config that this machine's chezmoi data names.
//!
//! Opt out with `git config renovate.localSkip true`, adding `--global` for the whole machine, or once with `RENOVATE_LOCAL_SKIP=1 git push`.

use crate::locate;
use crate::real_git;
use crate::refusal::Refusal;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

mod cargo;
pub mod ci_pin;
pub mod mise;
pub mod report;
pub mod tarball;

const DEFAULT_UPDATE_TYPES: &str = "minor patch pin pinDigest";
const LOOKUP_REQUEST_TIMEOUT: &str = "5s";
const LOOKUP_TMPFS: &str = "/tmp:rw,nodev,nosuid,size=2g";
const LOOKUP_CPUS: &str = "2";
const LOOKUP_MEMORY: &str = "3g";
const INSPECT_ARGUMENT_COUNT: usize = 2;
const INSPECT_REVISION_ARGUMENT_COUNT: usize = 4;
const INSPECT_REVISION_INDEX: usize = INSPECT_REVISION_ARGUMENT_COUNT - 1;

/// The exit status of a subcommand the gate doesn't serve.
const USAGE_ERROR: i32 = 2;
/// The digest digits an update shows when it names no version.
const SHORT_DIGEST: usize = 12;
/// The image's tool directory, which Renovate writes inside its read-only container.
const CONTAINERBASE: &str = "/tmp/containerbase";
/// The first user's and the `staff` group's identifiers on macOS, when `id` can't tell.
const DEFAULT_UID: u32 = 501;
const DEFAULT_GID: u32 = 20;

/// One dependency with a blocking update in a validated report.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Update {
    package_file: String,
    dep_name: String,
    current_value: String,
    resolved_version: String,
    current_digest: String,
    replace_string: String,
    kind: String,
    new_value: String,
    new_digest: String,
    breaking: bool,
    inherited_touched: bool,
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct LookupIdentity(Update);

impl Update {
    fn lookup_identity(&self) -> LookupIdentity {
        LookupIdentity(Self {
            inherited_touched: false,
            ..self.clone()
        })
    }
}

pub fn dispatch(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("run") => run_local(&args[1..]),
        Some("inspect") if args.len() == INSPECT_ARGUMENT_COUNT => {
            inspect_report(Path::new(&args[1]), None)
        }
        Some("inspect")
            if args.len() == INSPECT_REVISION_ARGUMENT_COUNT && args[2] == "--revision" =>
        {
            inspect_report(Path::new(&args[1]), Some(&args[INSPECT_REVISION_INDEX]))
        }
        _ => {
            eprintln!(
                "usage: dotguard renovate run [arguments] | inspect <report.json> [--revision <commit>]"
            );
            USAGE_ERROR
        }
    }
}

fn update_types() -> String {
    std::env::var("RENOVATE_LOCAL_UPDATE_TYPES")
        .or_else(|_| std::env::var("RENOVATE_LOCAL_GATE_TYPES"))
        .unwrap_or_else(|_| DEFAULT_UPDATE_TYPES.to_owned())
}

fn read_report(path: &Path, found: Option<&Found>) -> Result<report::CheckedReport, String> {
    std::fs::read(path)
        .map_err(|_| "the required report could not be read".to_owned())
        .and_then(|bytes| {
            match found {
                Some(found) => report::parse_for_push(&bytes, &update_types(), found),
                None => report::parse(&bytes, &update_types()),
            }
            .map_err(|error| error.to_string())
        })
}

fn report_refusal(evidence: &str) -> i32 {
    Refusal::new(
        "push.evidence",
        "the required report could not be validated",
        "dotguard renovate run",
    )
    .evidence(evidence)
    .emit();
    1
}

fn inspection_source(revision: &str) -> Result<Found, String> {
    let repository = real_git::capture(&["rev-parse", "--show-toplevel"])
        .ok_or("source inspection requires a Git repository")?
        .trim()
        .to_owned();
    let offered = real_git::capture(&[
        "-C",
        &repository,
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{revision}^{{commit}}"),
    ])
    .ok_or("source inspection requires an existing commit")?
    .trim()
    .to_owned();
    Ok(Found {
        repository,
        changed_files: Vec::new(),
        added_by_file: BTreeMap::new(),
        baselines_by_file: BTreeMap::new(),
        offered: Some(offered),
        scope_unknown: true,
    })
}

fn inspect_report(path: &Path, revision: Option<&str>) -> i32 {
    let source = match revision.map(inspection_source).transpose() {
        Ok(source) => source,
        Err(reason) => return report_refusal(&reason),
    };
    let report = match read_report(path, source.as_ref()) {
        Ok(report) => report,
        Err(evidence) => return report_refusal(&evidence),
    };
    if let Some(offered) = source.as_ref().and_then(|source| source.offered.as_deref()) {
        println!("Source revision: {offered}");
    }
    println!(
        "Validated lookup evidence: {} nonfatal notices, {} selected update candidates",
        report.notices,
        report.updates.len()
    );
    for update in report.updates {
        println!(
            "{}: {} {} -> {} ({})",
            update.package_file,
            update.dep_name,
            display_current(&update),
            display_new(&update),
            update.kind
        );
    }
    0
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
    let integrated = match integrated_scope(remote, input, &found) {
        Ok(integrated) => integrated,
        Err(cause) => return unavailable(&cause),
    };

    if locate::which("docker").is_none() {
        return unavailable("required tool docker is missing");
    }
    eprintln!(
        ">>> renovate gate: dependency manifests changed — checking the touched dependencies..."
    );

    let report = renovate_report(found.offered.as_deref());
    let Some(report) = report else {
        return unavailable("Renovate did not produce verified evidence for the offered commit");
    };

    let checked = std::fs::read(&report)
        .map_err(|_| "the required report could not be read".to_owned())
        .and_then(|bytes| {
            let checked = report::parse_for_push(&bytes, &update_types(), &found)
                .map_err(|error| error.to_string())?;
            let integrated = integrated
                .as_ref()
                .map(|scope| report::parse_for_push(&bytes, &update_types(), scope))
                .transpose()
                .map_err(|error| error.to_string())?;
            Ok((checked, integrated))
        });
    if let Some(dir) = report.parent() {
        platform::cleanup::remove_dir_all(dir);
    }
    let (checked, integrated_report) = match checked {
        Ok(checked) => checked,
        Err(evidence) => return report_refusal(&evidence),
    };
    let authored = match introduced_updates(integrated.as_ref().zip(integrated_report.as_ref())) {
        Ok(authored) => authored,
        Err(cause) => return unavailable(cause),
    };
    if checked.notices != 0 {
        eprintln!("renovate gate: {} nonfatal report notices", checked.notices);
    }
    let updates = checked.updates;
    let contexts = match replacement_contexts(&updates, &found) {
        Ok(contexts) => contexts,
        Err(cause) => return unavailable(cause),
    };
    let (blocked, outdated) = classify(&updates, &found, &contexts, authored.as_ref());

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
    repository: String,
    changed_files: Vec<String>,
    added_by_file: BTreeMap<String, String>,
    baselines_by_file: BTreeMap<String, String>,
    offered: Option<String>,
    scope_unknown: bool,
}

struct SourceContext<'a> {
    package_file: &'a str,
    before: String,
    after: String,
}

/// The manifest gate over one push.
/// `None` when the push touches nothing Renovate manages, the common case, which must cost nothing.
fn scope(remote: &str, input: &str) -> Result<Option<Found>, String> {
    let mut changed_files: Vec<String> = Vec::new();
    let mut added_by_file: BTreeMap<String, String> = BTreeMap::new();
    let mut baselines_by_file = BTreeMap::new();
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
            for file in &files {
                baselines_by_file.insert(file.clone(), b.clone());
            }
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
        repository: real_git::capture(&["rev-parse", "--show-toplevel"])
            .ok_or("the repository root is unavailable")?
            .trim()
            .to_owned(),
        changed_files,
        added_by_file,
        baselines_by_file,
        offered,
        scope_unknown,
    }))
}

/// Splits the updates into blocking ones, which this push touched, and outdated ones, which stay with Renovate.
fn classify(
    updates: &[Update],
    found: &Found,
    contexts: &[SourceContext<'_>],
    authored: Option<&BTreeSet<LookupIdentity>>,
) -> (Vec<String>, Vec<String>) {
    let mut blocked = Vec::new();
    let mut outdated = Vec::new();
    for c in updates {
        if !c.inherited_touched && !found.changed_files.iter().any(|f| f == &c.package_file) {
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
            touched: rules::freshness_rules::introduced(
                source_touched(c, found, contexts),
                authored.is_none_or(|updates| updates.contains(&c.lookup_identity())),
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

fn source_touched(c: &Update, found: &Found, contexts: &[SourceContext<'_>]) -> bool {
    if !c.inherited_touched && !found.changed_files.contains(&c.package_file) {
        return false;
    }
    c.inherited_touched
        || touched(
            c,
            found.added_by_file.get(&c.package_file).map(String::as_str),
            found.scope_unknown,
        )
        || contexts
            .iter()
            .find(|context| context.package_file == c.package_file)
            .is_some_and(|context| {
                let replacement = protocol_text(&c.replace_string);
                !replacement.is_empty()
                    && context.after.matches(&replacement).count()
                        > context.before.matches(&replacement).count()
            })
}

fn introduced_updates(
    integrated: Option<(&Found, &report::CheckedReport)>,
) -> Result<Option<BTreeSet<LookupIdentity>>, &'static str> {
    let Some((found, checked)) = integrated else {
        return Ok(None);
    };
    let contexts = replacement_contexts(&checked.updates, found)?;
    Ok(Some(
        checked
            .updates
            .iter()
            .filter(|update| source_touched(update, found, &contexts))
            .map(Update::lookup_identity)
            .collect(),
    ))
}

fn replacement_contexts<'a>(
    updates: &'a [Update],
    found: &Found,
) -> Result<Vec<SourceContext<'a>>, &'static str> {
    let mut contexts: Vec<SourceContext<'a>> = Vec::new();
    for candidate in updates {
        if candidate.breaking
            || candidate.replace_string.is_empty()
            || !found.changed_files.contains(&candidate.package_file)
            || touched(
                candidate,
                found
                    .added_by_file
                    .get(&candidate.package_file)
                    .map(String::as_str),
                found.scope_unknown,
            )
            || contexts
                .iter()
                .any(|context| context.package_file == candidate.package_file)
        {
            continue;
        }
        let base = found
            .baselines_by_file
            .get(&candidate.package_file)
            .ok_or("the dependency source baseline is unavailable")?;
        let offered = found
            .offered
            .as_deref()
            .ok_or("the offered source is unavailable")?;
        let before = real_git::capture(&["show", &format!("{base}:{}", candidate.package_file)])
            .ok_or("Git could not inspect the original dependency source")?;
        let after = real_git::capture(&["show", &format!("{offered}:{}", candidate.package_file)])
            .ok_or("Git could not inspect the offered dependency source")?;
        contexts.push(SourceContext {
            package_file: &candidate.package_file,
            before: protocol_text(&before),
            after: protocol_text(&after),
        });
    }
    Ok(contexts)
}

fn protocol_text(source: &str) -> String {
    let mut text = String::with_capacity(source.len());
    for character in source.chars() {
        match character {
            '\r' => (),
            '\u{1f}' | '\n' => text.push(' '),
            _ => text.push(character),
        }
    }
    text
}

/// The diff base for one pushed ref: the remote tip it updates, or the fork point from the default branch for a new branch.
/// Ask the remote which branch `HEAD` names for a new branch.
/// A cached `refs/remotes/<remote>/HEAD` can name a branch the remote no longer uses.
fn range_base(remote: &str, local_sha: &str, remote_sha: &str) -> Result<Option<String>, String> {
    if !remote_sha.is_empty() && !remote_sha.chars().all(|c| c == '0') {
        return Ok(Some(remote_sha.to_owned()));
    }
    default_base(remote, local_sha)
}

fn default_base(remote: &str, local_sha: &str) -> Result<Option<String>, String> {
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

fn integrated_scope(remote: &str, input: &str, found: &Found) -> Result<Option<Found>, String> {
    let fields: Vec<_> = input.split_whitespace().collect();
    let [_, offered, _, published] = fields[..] else {
        return Err("the offered revision range is malformed".to_owned());
    };
    if published.chars().all(|character| character == '0') {
        return Ok(None);
    }
    let merges = real_git::capture(&[
        "rev-list",
        "--merges",
        "--max-count=1",
        &format!("{published}..{offered}"),
    ])
    .ok_or("Git could not inspect the incoming merge history")?;
    if merges.trim().is_empty() {
        return Ok(None);
    }
    let base = default_base(remote, offered)?
        .ok_or("the integrated default branch baseline is unavailable")?;
    let changed_files = diff_names(&base, offered)?;
    let added_by_file = diff_added_lines(&base, offered, &changed_files)?
        .into_iter()
        .collect();
    let baselines_by_file = changed_files
        .iter()
        .map(|file| (file.clone(), base.clone()))
        .collect();
    Ok(Some(Found {
        repository: found.repository.clone(),
        changed_files,
        added_by_file,
        baselines_by_file,
        offered: Some(offered.to_owned()),
        scope_unknown: false,
    }))
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
    real_git::capture(&["diff", "--name-only", "--diff-filter=d", base, tip])
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
    let replacement = protocol_text(&c.replace_string);
    if !replacement.is_empty() && protocol_text(added_lines).contains(&replacement) {
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
    if !c.resolved_version.is_empty() {
        c.resolved_version.clone()
    } else if !c.current_value.is_empty() {
        c.current_value.clone()
    } else {
        c.current_digest.chars().take(SHORT_DIGEST).collect()
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

    let mut cfg = prepare(offered, &[])?;
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

/// Reports whether Renovate manages this path.
pub fn is_manifest(path: &str) -> bool {
    let lowered = path.to_lowercase();
    if matches!(
        lowered.as_str(),
        "home/.chezmoidata/versions.toml" | "home/.chezmoidata/modern.toml"
    ) {
        return true;
    }
    let comps = lowered.split('/');
    let name = comps.clone().next_back().unwrap_or("");

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
        "bun.lock",
        "bun.lockb",
        "action.yml",
        "action.yaml",
    ];
    if exact.contains(&name) {
        return true;
    }
    let file = Path::new(name);
    let ext = file.extension().unwrap_or_default().to_string_lossy();
    let stem = file.file_stem().unwrap_or_default().to_string_lossy();
    if name.starts_with("requirements") && ext == "txt" {
        return true;
    }
    if ext == "gradle"
        || (ext == "kts" && stem.ends_with(".gradle") && stem.len() > ".gradle".len())
    {
        return true;
    }
    // `Dockerfile` and suffixed variants such as `Dockerfile.prod`, but no other word that starts with it.
    if comps.clone().any(|c| {
        c == "dockerfile"
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
        && comps.clone().rev().nth(1) == Some("workflows")
        && comps
            .clone()
            .any(|component| matches!(component.as_bytes(), b".github"))
    {
        return true;
    }
    if ext == "toml"
        && comps
            .clone()
            .zip(comps.skip(1))
            .any(|pair| pair == (".config", "mise"))
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

const DEFAULT_IMAGE: &str = "renovate/renovate:44.169.0@sha256:99c305024372fe8fc928a187dccfa276ebfd3e6911b967cb7b8667376ed6c046";

/// The image reference must name an exact `renovate/renovate` version with a sha256 digest, so no caller can fall back to `:latest`.
fn image_is_pinned(image: &str) -> bool {
    crate::guard_rules::image_is_pinned(image.as_bytes())
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
        "--cpus".into(),
        LOOKUP_CPUS.into(),
        "--memory".into(),
        LOOKUP_MEMORY.into(),
        "--memory-swap".into(),
        LOOKUP_MEMORY.into(),
        "--tmpfs".into(),
        LOOKUP_TMPFS.into(),
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

    let managers = serde_json::json!([{
        "customType": "regex",
        "managerFilePatterns": [r"/(^|/)(\.mise|mise)\.toml$/"],
        "matchStrings": [r#"(?:^|\r?\n)[ \t]*dotnet[ \t]*=[ \t]*[\"'](?<currentValue>\d+\.\d+\.\d+)[\"']"#],
        "depNameTemplate": "dotnet",
        "packageNameTemplate": "dotnet-sdk",
        "datasourceTemplate": "dotnet-version",
        "versioningTemplate": "semver"
    }]);
    args.extend([
        "--env".into(),
        format!("RENOVATE_CUSTOM_MANAGERS={managers}"),
    ]);

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
fn prepare(offered: Option<&str>, passthrough: &[String]) -> Option<RunConfig> {
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
            .filter(|s| !s.is_empty())
            .or_else(doppler_token),
        passthrough: passthrough.to_vec(),
    })
}

/// The secret this machine's Doppler project and config hold for Renovate's GitHub lookups.
///
/// `chezmoi` names the project and config, and `doppler` prints the value into this process alone.
/// A missing tool, an unnamed project, a failed fetch, or an unresolved reference yields `None`, and the run then lacks GitHub lookups as before.
fn doppler_token() -> Option<String> {
    let scope = locate::child("chezmoi")
        .args([
            "execute-template",
            "{{ .doppler.project }}/{{ .doppler.config }}",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let scope = String::from_utf8(scope.stdout).ok()?;
    let (project, config) = doppler_scope(&scope)?;
    let fetched = locate::doppler()?
        .args([
            "secrets",
            "get",
            "RENOVATE_GITHUB_COM_TOKEN",
            "--plain",
            "--project",
            project,
            "--config",
            config,
            "--timeout",
            LOOKUP_REQUEST_TIMEOUT,
            "--no-check-version",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let token = String::from_utf8(fetched.stdout).ok()?.trim().to_owned();
    (!token.is_empty() && !token.starts_with("${")).then_some(token)
}

/// The project and config of `project/config`, when the text names both.
fn doppler_scope(text: &str) -> Option<(&str, &str)> {
    let (project, config) = text.trim().split_once('/')?;
    (!project.is_empty() && !config.is_empty() && !config.contains('/'))
        .then_some((project, config))
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
    let offered_index = snapshot.with_extension("index");
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
        && snapshot_git(
            &snapshot,
            &["init", "--quiet", "--initial-branch=main", "--template="],
        )
        && snapshot_git(&snapshot, &["add", "--all"]);
    if !ok {
        platform::cleanup::remove_dir_all(&snapshot);
        return None;
    }
    Some(snapshot)
}

fn snapshot_git(snapshot: &Path, arguments: &[&str]) -> bool {
    real_git::find()
        .and_then(|git| {
            platform::process::command(git)
                .arg("-C")
                .arg(snapshot)
                .args(arguments)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .ok()
        })
        .is_some_and(|status| status.success())
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

pub fn run_local(passthrough: &[String]) -> i32 {
    let Some(cfg) = prepare(None, passthrough) else {
        return 1;
    };
    let code = run_prepared(&cfg, false).unwrap_or(1);
    platform::cleanup::remove_dir_all(&cfg.snapshot);
    code
}

/// Spawn docker per the plan.
fn run_prepared(cfg: &RunConfig, quiet_stdout: bool) -> Option<i32> {
    let (args, removed) = docker_plan(cfg);
    let mut cmd = locate::child("docker");
    cmd.args(&args);
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
    use super::{RunConfig, docker_plan, doppler_scope, image_is_pinned};
    use std::path::PathBuf;

    #[test]
    fn the_doppler_scope_names_both_a_project_and_a_config() {
        assert_eq!(doppler_scope("dotfiles/prd\n"), Some(("dotfiles", "prd")));
        for unnamed in ["", "/", "dotfiles/", "/prd", "dotfiles", "a/b/c"] {
            assert_eq!(doppler_scope(unnamed), None, "{unnamed:?}");
        }
    }

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
    fn the_local_lookup_maps_mise_dotnet_to_the_official_sdk_datasource() {
        let args = argv(&cfg(None, None));
        let managers = args
            .iter()
            .filter_map(|arg| arg.strip_prefix("RENOVATE_CUSTOM_MANAGERS="))
            .flat_map(|value| serde_json::from_str::<Vec<serde_json::Value>>(value).unwrap())
            .collect::<Vec<_>>();
        assert!(managers.iter().any(|manager| {
            manager["depNameTemplate"] == "dotnet"
                && manager["packageNameTemplate"] == "dotnet-sdk"
                && manager["datasourceTemplate"] == "dotnet-version"
                && manager["customType"] == "regex"
                && manager["matchStrings"]
                    .as_array()
                    .is_some_and(|patterns| !patterns.is_empty())
        }));
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
        assert!(a.contains(&"/tmp:rw,nodev,nosuid,size=2g".to_owned()));
        for (flag, value) in [("--cpus", "2"), ("--memory", "3g"), ("--memory-swap", "3g")] {
            let actual = a
                .iter()
                .position(|arg| arg == flag)
                .map(|i| a[i + 1].as_str());
            assert_eq!(actual, Some(value), "{flag}");
        }
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
    use super::{Update, is_manifest, touched};
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
            "settings.gradle.kts",
            "..gradle.kts",
            "Build.GRADLE.KTS",
            "İ.gradle.kts",
            "Dockerfile",
            "apps/api/Dockerfile",
            "apps/api/Dockerfile.prod",
            ".github/workflows/ci.yml",
            ".github/workflows/release.yaml",
            ".GITHUB/WORKFLOWS/CI.YML",
            "nested/.github/workflows/ci.yml",
            "action.yml",
            "action.yaml",
            ".github/actions/rust-toolchain/action.yml",
            "tools/native-action/action.yaml",
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
            "docs/action.yml.md",
            "tools/native-action/not-action.yml",
            ".githubx/workflows/ci.yml",
            "x.github/workflows/ci.yml",
            "commise.toml",
            "miseo.toml/backups",
            "app.devcontainer.jsonx",
            ".gradle.kts",
            "gradle.kts",
            "build.gradle..kts",
            "build.gradle.kts.old",
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

    fn cand(rep: &str, cur: &str, dep: &str) -> Update {
        Update {
            package_file: "Cargo.toml".into(),
            dep_name: dep.into(),
            current_value: cur.into(),
            resolved_version: String::new(),
            current_digest: String::new(),
            replace_string: rep.into(),
            kind: "minor".into(),
            new_value: "9.9".into(),
            new_digest: String::new(),
            breaking: false,
            inherited_touched: false,
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
