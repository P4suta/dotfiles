//! Request-bound execution evidence for changed implementation contracts.

pub mod mutation;
mod plan;
pub mod proof;
mod reports;
pub mod source;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::assure::Tree;
use plan::{Change, Check, Method, Plan, Scope};
use platform::refusal::Refusal;
use platform::tool::Tool;
use rules::verification_rules::{self as core, Verdict};

/// The persisted verification protocol version.
const VERSION: u8 = 1;
/// Evidence retained beside the managed answer request.
const STATE_FILE: &str = "verification.json";
/// A process lock serializes attempts and edits of one session.
const LOCK_FILE: &str = "verification.lock";
/// The retained byte limit for one check stream.
const MAX_OUTPUT_BYTES: u64 = 16 * 1024 * 1024;
/// The largest owner-supplied verification contract.
const MAX_PLAN_BYTES: usize = 256 * 1024;

#[derive(Debug, clap::Subcommand)]
pub enum Action {
    /// Freeze an affected-contract plan before implementation edits.
    Begin {
        #[arg(long)]
        session: String,
        #[arg(long)]
        plan: PathBuf,
    },
    /// Execute a declared check and record its native results.
    Run {
        #[arg(long)]
        session: String,
        #[arg(long)]
        check: String,
        #[arg(long)]
        red: bool,
        #[arg(long, conflicts_with = "red")]
        baseline: bool,
    },
    /// Refuse missing or stale evidence for affected contracts or the full release inventory.
    Check {
        #[arg(long)]
        session: String,
        #[arg(long)]
        release: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Red,
    Baseline,
    Green,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Session {
    version: u8,
    request: String,
    sequence: u64,
    roots: BTreeMap<PathBuf, Root>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Root {
    baseline: String,
    plan: Option<Plan>,
    red: BTreeMap<String, Run>,
    #[serde(default)]
    before: BTreeMap<String, Run>,
    green: BTreeMap<String, Run>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Run {
    sequence: u64,
    source: String,
    tools: String,
    plan: String,
    check: String,
    production: BTreeMap<String, String>,
    exit: Option<i32>,
    stdout: String,
    stderr: String,
    completed: bool,
    retained: PathBuf,
    #[serde(default)]
    native: BTreeMap<String, String>,
}

struct State {
    directory: PathBuf,
    session: Session,
    lock: File,
}

impl Drop for State {
    fn drop(&mut self) {
        platform::cleanup::report(self.lock.unlock(), "release the verification session lock");
    }
}

fn refuse(rule: &str, cause: impl Into<String>, session: &str) -> anyhow::Error {
    Refusal::new(
        rule,
        cause,
        format!("dotfiles verify begin --session {session} --plan <contract.json>"),
    )
    .into()
}

fn load(home: &Path, session: &str) -> Result<State> {
    load_locked(home, session, Access::Write)
}

#[derive(Clone, Copy)]
enum Access {
    Read,
    Write,
}

fn load_locked(home: &Path, session: &str, access: Access) -> Result<State> {
    let directory = crate::answer::directory(home, session)?;
    let request = crate::answer::request(&directory)?.identity()?;
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(LOCK_FILE))?;
    match access {
        Access::Read => lock.try_lock_shared(),
        Access::Write => lock.try_lock(),
    }
    .context("another verification attempt or implementation edit holds this session")?;
    let mut state = match fs::read(directory.join(STATE_FILE)) {
        Ok(bytes) => serde_json::from_slice::<Session>(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Session {
            version: VERSION,
            request: request.clone(),
            sequence: 0,
            roots: BTreeMap::new(),
        },
        Err(error) => return Err(error.into()),
    };
    ensure!(
        state.version == VERSION,
        "unsupported verification evidence version"
    );
    if state.request != request {
        state.roots.retain(|root, record| {
            root.exists() && check_root(root, record, session, false).is_err()
        });
        state = Session {
            version: VERSION,
            request,
            sequence: state.sequence,
            roots: state.roots,
        };
    }
    Ok(State {
        directory,
        session: state,
        lock,
    })
}

impl State {
    fn save(&self) -> Result<()> {
        ensure!(
            crate::answer::request(&self.directory)?.identity()? == self.session.request,
            "managed request changed during verification"
        );
        crate::answer::write_json(&self.directory.join(STATE_FILE), &self.session)
    }

    fn start_attempt(
        &mut self,
        root: &Path,
        definition: (&Plan, &Check),
        phase: Phase,
        source: String,
        tool_identity: String,
        current_production: BTreeMap<String, String>,
    ) -> Result<PathBuf> {
        let (plan, check) = definition;
        self.session.sequence = self
            .session
            .sequence
            .checked_add(1)
            .context("verification sequence overflow")?;
        let sequence = self.session.sequence;
        let retained = self
            .directory
            .join("verification-runs")
            .join(sequence.to_string());
        fs::create_dir_all(&retained)?;
        let attempt = Run {
            sequence,
            source,
            tools: tool_identity,
            plan: plan.identity()?,
            check: check.identity()?,
            production: current_production,
            exit: None,
            stdout: String::new(),
            stderr: String::new(),
            completed: false,
            retained: retained.clone(),
            native: BTreeMap::new(),
        };
        let record = self
            .session
            .roots
            .get_mut(root)
            .context("missing verification root")?;
        let attempts = match phase {
            Phase::Red => &mut record.red,
            Phase::Baseline => &mut record.before,
            Phase::Green => &mut record.green,
        };
        attempts.insert(check.id.clone(), attempt);
        self.save()?;
        Ok(retained)
    }

    fn register(&mut self, root: &Path) -> Result<()> {
        if !self.session.roots.contains_key(root) {
            self.register_baseline(crate::assure::RequestBaseline {
                root: root.to_path_buf(),
                commit: crate::assure::worktree_snapshot(root)?.commit,
            })?;
        }
        Ok(())
    }

    fn register_baseline(&mut self, baseline: crate::assure::RequestBaseline) -> Result<()> {
        if let std::collections::btree_map::Entry::Vacant(entry) =
            self.session.roots.entry(baseline.root)
        {
            entry.insert(Root {
                baseline: baseline.commit,
                plan: None,
                red: BTreeMap::new(),
                before: BTreeMap::new(),
                green: BTreeMap::new(),
            });
            self.save()?;
        }
        Ok(())
    }
}

fn root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = platform::issue_context::git(&cwd, &["rev-parse", "--show-toplevel"], None)?;
    Ok(crate::canonical(Path::new(root.trim()))?)
}

fn begin(home: &Path, session: &str, file: &Path) -> Result<()> {
    let plan = parse_plan(&fs::read(file)?)?;
    let root = root()?;
    let mut state = load(home, session)?;
    state.register(&root)?;
    let record = state
        .session
        .roots
        .get_mut(&root)
        .context("missing verification root")?;
    if let Some(previous) = &record.plan {
        ensure!(
            plan.extends(previous)?,
            "verification contract may grow, but existing commands and obligations cannot be narrowed or replaced"
        );
        record.plan = Some(plan);
        state.save()?;
    } else {
        let current = source::snapshot(&root)?.0;
        let changed = crate::assure::changed(
            &root,
            &Tree::Commit(Some(record.baseline.clone())),
            &Tree::Commit(Some(current)),
        )?;
        ensure!(
            !changed.iter().any(|path| source::relevant(Path::new(path))),
            "implementation changed before the verification contract was registered"
        );
        record.plan = Some(plan);
        state.save()?;
    }
    println!("Registered verification contract for {}.", root.display());
    Ok(())
}

fn parse_plan(bytes: &[u8]) -> Result<Plan> {
    ensure!(
        bytes.len() <= MAX_PLAN_BYTES,
        "verification contract exceeds its byte bound"
    );
    let plan: Plan = serde_json::from_slice(bytes)?;
    plan.validate()?;
    Ok(plan)
}

/// Parse and normalize the same contract the managed runner admits.
///
/// # Errors
/// Refuses invalid contracts and enforces the input byte limit.
pub fn inspect_plan(text: &str) -> Result<String> {
    Ok(serde_json::to_string(&parse_plan(text.as_bytes())?)?)
}

/// Exercise the native structured-result parser without files or command execution.
///
/// # Errors
/// Refuses an incomplete or mismatched fixture contract result.
pub fn inspect_producer_report(text: &str, red: bool) -> Result<()> {
    let check = Check {
        id: "fixture".into(),
        method: Method::Test,
        tool: plan::Tool::Cargo,
        arguments: vec!["test".into()],
        report: plan::Report::Structured,
        expected: vec!["contract".into()],
        rejecting: vec![],
        report_path: None,
        verifier_version: None,
        environment: vec![],
        scope: Scope::Affected,
    };
    reports::validate(
        &check,
        i32::from(red),
        text,
        "fixture",
        "fixture",
        Path::new("."),
        red,
    )?;
    Ok(())
}

fn program(check: &Check) -> Result<std::ffi::OsString> {
    if matches!(check.tool, plan::Tool::Dotfiles) {
        Ok(std::env::current_exe()?.into_os_string())
    } else {
        Ok(check.tool.program().into())
    }
}

fn tools(root: &Path, check: &Check) -> Result<String> {
    let executable = if matches!(check.tool, plan::Tool::Dotfiles) {
        std::env::current_exe()?
    } else {
        let output = Tool::Mise
            .command()
            .current_dir(root)
            .args(["which", check.tool.program()])
            .output()?;
        ensure!(
            output.status.success(),
            "cannot resolve the project's pinned verification tool: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        PathBuf::from(String::from_utf8(output.stdout)?.trim())
    };
    ensure!(
        executable.is_absolute(),
        "verification executable is not an absolute path"
    );
    let version = Tool::Mise
        .command()
        .current_dir(root)
        .args(["x", "--"])
        .arg(program(check)?)
        .arg("--version")
        .output()?;
    ensure!(
        version.status.success() && !version.stdout.is_empty(),
        "verification tool version is unavailable: {}",
        String::from_utf8_lossy(&version.stderr)
    );
    let environment: BTreeMap<_, _> = [
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTUP_TOOLCHAIN",
        "CARGO_HOME",
        "RUSTUP_HOME",
    ]
    .into_iter()
    .chain(check.environment.iter().map(String::as_str))
    .map(|name| {
        (
            name,
            std::env::var_os(name).map(|value| value.to_string_lossy().into_owned()),
        )
    })
    .collect();
    let identity = [
        fs::read(executable)?,
        version.stdout,
        version.stderr,
        serde_json::to_vec(&environment)?,
    ]
    .concat();
    Ok(source::digest(&identity))
}

fn production(root: &Path, tree: &Tree, plan: &Plan) -> Result<BTreeMap<String, String>> {
    plan.contracts
        .iter()
        .map(|contract| {
            Ok((
                contract.path.clone(),
                source::production_id(root, tree, &contract.path)?,
            ))
        })
        .collect()
}

fn retain_native(
    root: &Path,
    retained: &Path,
    check: &Check,
    stdout: &str,
) -> Result<BTreeMap<String, String>> {
    let reports = if matches!(check.report, plan::Report::Kani) {
        proof::references(stdout)?
            .into_iter()
            .map(|reference| reference.path)
            .collect::<Vec<_>>()
    } else if matches!(check.report, plan::Report::Mutants) {
        let path = check
            .report_path
            .as_deref()
            .context("missing native report path")?;
        vec![
            PathBuf::from(path),
            Path::new(path).with_file_name("mutants.json"),
        ]
    } else {
        Vec::new()
    };
    let mut hashes = BTreeMap::new();
    for report in reports {
        let destination = retained.join(&report);
        fs::create_dir_all(
            destination
                .parent()
                .context("native report has no parent")?,
        )?;
        fs::copy(source::evidence_path(root, &report)?, &destination)?;
        hashes.insert(
            report
                .to_str()
                .context("non-UTF-8 native report path")?
                .to_owned(),
            source::digest(&fs::read(destination)?),
        );
    }
    Ok(hashes)
}

fn clear_native(root: &Path, check: &Check) -> Result<()> {
    if !matches!(check.report, plan::Report::Mutants) {
        return Ok(());
    }
    let path = check
        .report_path
        .as_deref()
        .context("missing native report path")?;
    for report in [
        PathBuf::from(path),
        Path::new(path).with_file_name("mutants.json"),
    ] {
        let path = platform::issue_context::edit_path(root, &root.join(report))?;
        ensure!(
            platform::issue_context::git(
                root,
                &[
                    "check-ignore",
                    "--",
                    path.to_str().context("non-UTF-8 report path")?
                ],
                None
            )
            .is_ok(),
            "native report must be an ignored build artifact"
        );
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn read_output(path: &Path) -> Result<String> {
    ensure!(
        fs::metadata(path)?.len() <= MAX_OUTPUT_BYTES,
        "verification output exceeds its retained byte bound"
    );
    Ok(fs::read_to_string(path)?)
}

#[derive(Debug)]
struct Execution {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn execute_check(
    root: &Path,
    check: &Check,
    source: &str,
    plan: &Plan,
    retained: &Path,
) -> Result<Execution> {
    let stdout = retained.join("stdout");
    let stderr = retained.join("stderr");
    let status = Tool::Mise
        .command()
        .current_dir(root)
        .args(["x", "--"])
        .arg(program(check)?)
        .args(&check.arguments)
        .env("DOTFILES_VERIFICATION_SOURCE", source)
        .env("DOTFILES_VERIFICATION_PLAN", plan.identity()?)
        .stdin(Stdio::null())
        .stdout(File::create(&stdout)?)
        .stderr(File::create(&stderr)?)
        .status()
        .context("execute the declared verification command")?;
    let stdout = read_output(&stdout)?;
    let stderr = read_output(&stderr)?;
    print!("{stdout}");
    eprint!("{stderr}");
    Ok(Execution {
        status,
        stdout,
        stderr,
    })
}

fn run(home: &Path, session: &str, id: &str, phase: Phase) -> Result<()> {
    let root = root()?;
    let mut state = load(home, session)?;
    let record = state
        .session
        .roots
        .get(&root)
        .context("verification contract is missing")?;
    let plan = record
        .plan
        .clone()
        .context("verification contract is missing")?;
    plan.validate()?;
    let check = plan
        .checks
        .iter()
        .find(|check| check.id == id)
        .context("unknown verification check")?;
    ensure!(
        phase == Phase::Green || check.method == Method::Test,
        "before-change phases apply only to contract tests"
    );
    let (commit, source) = source::snapshot(&root)?;
    let before = Tree::Commit(Some(commit));
    let current_production = production(&root, &before, &plan)?;
    if phase != Phase::Green {
        let original = production(&root, &Tree::Commit(Some(record.baseline.clone())), &plan)?;
        ensure!(
            plan.contracts
                .iter()
                .filter(|contract| matches!(contract.change, Change::Behavior | Change::Refactor))
                .all(|contract| current_production.get(&contract.path)
                    == original.get(&contract.path)),
            "required test phase ran after production changed; keep the missing chronology and restore the baseline before starting a new test cycle"
        );
    }
    let tool_identity = tools(&root, check)?;
    let retained = state.start_attempt(
        &root,
        (&plan, check),
        phase,
        source.clone(),
        tool_identity.clone(),
        current_production,
    )?;
    clear_native(&root, check)?;
    let output = execute_check(&root, check, &source, &plan, &retained)?;
    let (after, after_source) = source::snapshot(&root)?;
    ensure!(
        after_source == source,
        "source changed while verification was running: {:?}",
        crate::assure::changed(&root, &before, &Tree::Commit(Some(after)))?
    );
    ensure!(
        tools(&root, check)? == tool_identity,
        "verification tool changed during execution"
    );
    let native = retain_native(&root, &retained, check, &output.stdout)?;
    let record = state
        .session
        .roots
        .get_mut(&root)
        .context("missing verification root")?;
    let attempt = match phase {
        Phase::Red => record.red.get_mut(id),
        Phase::Baseline => record.before.get_mut(id),
        Phase::Green => record.green.get_mut(id),
    }
    .context("missing verification attempt")?;
    attempt.exit = output.status.code();
    attempt.stdout = output.stdout;
    attempt.stderr = output.stderr;
    attempt.native = native;
    let result = reports::validate(
        check,
        output
            .status
            .code()
            .context("verification was interrupted")?,
        &attempt.stdout,
        &source,
        &attempt.plan,
        &retained,
        phase == Phase::Red,
    );
    attempt.completed = result.is_ok();
    state.save()?;
    result?;
    println!(
        "Retained {} verification result {id} at source {source}.",
        match phase {
            Phase::Red => "red",
            Phase::Baseline => "passing baseline",
            Phase::Green => "passing",
        }
    );
    Ok(())
}

fn validate_native(retained: &Path, native: &BTreeMap<String, String>) -> Result<()> {
    for (path, digest) in native {
        ensure!(
            source::safe_path(path)
                && source::digest(&fs::read(source::evidence_path(
                    retained,
                    Path::new(path)
                )?)?)
                    == *digest,
            "retained native evidence changed"
        );
    }
    Ok(())
}

fn validate_run(root: &Path, check: &Check, run: &Run, source: &str, phase: Phase) -> Result<()> {
    validate_native(&run.retained, &run.native)?;
    ensure!(
        run.completed && run.exit.is_some(),
        "verification attempt is incomplete"
    );
    ensure!(
        phase != Phase::Green || run.source == source,
        "verification result is stale"
    );
    ensure!(
        run.check == check.identity()?,
        "verification command or expected coverage changed"
    );
    ensure!(
        run.tools == tools(root, check)?,
        "verification tool identity is stale"
    );
    let result = reports::validate(
        check,
        run.exit.context("missing verification exit")?,
        &run.stdout,
        &run.source,
        &run.plan,
        &run.retained,
        phase == Phase::Red,
    )?;
    ensure!(
        result.passed != (phase == Phase::Red) && result.subjects == check.expected,
        "verification result differs from its native evidence"
    );
    Ok(())
}

fn before_green(
    root: &Path,
    record: &Root,
    plan: &Plan,
    contract: &plan::Contract,
    check: &Check,
    green: &Run,
) -> Result<bool> {
    let phase = match contract.change {
        Change::Behavior => Phase::Red,
        Change::Refactor => Phase::Baseline,
        Change::Configuration | Change::Verification => return Ok(true),
    };
    let original = source::production_id(
        root,
        &Tree::Commit(Some(record.baseline.clone())),
        &contract.path,
    )?;
    let previous = if phase == Phase::Red {
        &record.red
    } else {
        &record.before
    };
    for id in &contract.checks {
        let Some(prior_check) = plan
            .checks
            .iter()
            .find(|candidate| candidate.id == *id && candidate.method == Method::Test)
        else {
            continue;
        };
        let Some(prior) = previous.get(id) else {
            continue;
        };
        let history = core::TestHistory {
            expected: if contract.change == Change::Behavior {
                core::TestOutcome::Failed
            } else {
                core::TestOutcome::Passed
            },
            actual: if phase == Phase::Red {
                core::TestOutcome::Failed
            } else {
                core::TestOutcome::Passed
            },
            completed: prior.completed,
            matching_source: prior.production.get(&contract.path) == Some(&original),
            matching_test: prior_check
                .expected
                .iter()
                .all(|subject| check.expected.contains(subject)),
            before: prior.sequence,
            after: green.sequence,
        };
        if !core::ordered(history) {
            continue;
        }
        validate_run(root, prior_check, prior, &green.source, phase)?;
        return Ok(true);
    }
    Ok(false)
}

fn validate_scope(
    contract: &plan::Contract,
    affected: &[String],
    changed_production: bool,
    root: &Path,
    baseline: &Tree,
    current: &Tree,
) -> Result<()> {
    ensure!(
        contract.change != Change::Verification || !changed_production,
        "verification-only contract changed production: {}",
        contract.path
    );
    if contract.change == Change::Configuration {
        let paths: Vec<_> = affected
            .iter()
            .filter(|path| source::within(path, &contract.path))
            .cloned()
            .collect();
        let before = crate::assure::contents(root, baseline, &paths)?;
        let after = crate::assure::contents(root, current, &paths)?;
        ensure!(
            paths
                .iter()
                .map(|path| source::configuration_change(
                    path,
                    before.get(path).map(Vec::as_slice),
                    after.get(path).map(Vec::as_slice)
                ))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .all(|value| value),
            "configuration contract covers production code: {}",
            contract.path
        );
    }
    Ok(())
}

fn check_root(root: &Path, record: &Root, session: &str, release: bool) -> Result<()> {
    let (commit, source) = source::snapshot(root)?;
    let current = Tree::Commit(Some(commit));
    let baseline = Tree::Commit(Some(record.baseline.clone()));
    let changed = crate::assure::changed(root, &baseline, &current)?;
    let affected: Vec<_> = if release {
        crate::assure::paths(root, &current)?
    } else {
        changed.into_iter().collect()
    };
    let affected: Vec<_> = affected
        .into_iter()
        .filter(|path| source::relevant(Path::new(path)))
        .collect();
    if !release && affected.is_empty() {
        return Ok(());
    }
    ensure!(
        !affected.is_empty(),
        "release verification has no implementation inventory"
    );
    let Some(plan) = &record.plan else {
        return Err(refuse(
            "verification.plan",
            "changed implementation has no verification contract",
            session,
        ));
    };
    plan.validate()?;
    for path in &affected {
        ensure!(
            plan.contracts
                .iter()
                .any(|contract| source::within(path, &contract.path)),
            "verification contract omits changed source {path}"
        );
    }
    for contract in &plan.contracts {
        if !affected
            .iter()
            .any(|path| source::within(path, &contract.path))
        {
            continue;
        }
        let changed_production = source::production_id(root, &baseline, &contract.path)?
            != source::production_id(root, &current, &contract.path)?;
        validate_scope(
            contract,
            &affected,
            changed_production,
            root,
            &baseline,
            &current,
        )?;
        let mut required = contract.required();
        if affected
            .iter()
            .any(|path| source::within(path, &contract.path) && path.ends_with("_rules.rs"))
        {
            required |= core::Method::Proof.bit();
        }
        let mut verified = 0;
        let mut ordered =
            !changed_production || !matches!(contract.change, Change::Behavior | Change::Refactor);
        for id in &contract.checks {
            let check = plan
                .checks
                .iter()
                .find(|check| &check.id == id)
                .context("missing planned check")?;
            if release && check.scope != Scope::Full {
                continue;
            }
            let Some(green) = record.green.get(id) else {
                continue;
            };
            validate_run(root, check, green, &source, Phase::Green).map_err(|error| {
                refuse("verification.stale", format!("{id}: {error:#}"), session)
            })?;
            verified |= check.method.core().bit();
            if check.method == Method::Test {
                ordered |= before_green(root, record, plan, contract, check, green)?;
            }
        }
        match core::completion(required, verified, true, ordered) {
            Verdict::Admit => {}
            Verdict::Missing | Verdict::Tdd => {
                return Err(refuse(
                    "verification.missing",
                    format!(
                        "{} needs completed applicable checks and observed red-before-green evidence (required={required:#x}, verified={verified:#x}, TDD={ordered})",
                        contract.path
                    ),
                    session,
                ));
            }
            Verdict::Stale => unreachable!("identity is checked before the pure gate"),
        }
    }
    Ok(())
}

/// Check every checkout a managed session edited, including checkouts outside its initial directory.
///
/// # Errors
/// Refuses incomplete, stale, or unsupported verification evidence.
#[cfg(test)]
fn check_session(home: &Path, session: &str) -> Result<()> {
    let state = load_locked(home, session, Access::Read)?;
    for (root, record) in &state.session.roots {
        check_root(root, record, session, false)?;
    }
    Ok(())
}

/// # Errors
/// Returns an error when the managed request, contract, execution, or retained evidence is invalid.
pub fn execute(action: Action) -> Result<()> {
    let home = platform::home::home().context("native home is unavailable")?;
    match action {
        Action::Begin { session, plan } => begin(&home, &session, &plan),
        Action::Run {
            session,
            check,
            red,
            baseline,
        } => run(
            &home,
            &session,
            &check,
            if red {
                Phase::Red
            } else if baseline {
                Phase::Baseline
            } else {
                Phase::Green
            },
        ),
        Action::Check { session, release } => {
            let state = load_locked(&home, &session, Access::Read)?;
            let root = root()?;
            let record = state.session.roots.get(&root).ok_or_else(|| {
                refuse(
                    "verification.plan",
                    "verification contract is missing",
                    &session,
                )
            })?;
            check_root(&root, record, &session, release)?;
            println!(
                "Verified {} implementation contracts at the current source.",
                if release {
                    "full-inventory"
                } else {
                    "affected"
                }
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn retained_native_files_are_complete_and_reject_tampering_or_traversal() -> Result<()> {
        let root = tempfile::tempdir()?;
        let retained = tempfile::tempdir()?;
        let check: Check = serde_json::from_value(
            serde_json::json!({"id":"proofs","method":"proof","tool":"just","arguments":["proofs"],"report":"kani","expected":["bounds"],"rejecting":["false"]}),
        )?;
        let directory = root.path().join("verification/results");
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("one.json"), b"one")?;
        fs::write(directory.join("two.json"), b"two")?;
        let mut output = String::new();
        for name in ["one", "two"] {
            writeln!(
                output,
                "phase=verification name={name} args=[\"kani\"] source_sha256={}",
                "a".repeat(64)
            )?;
        }
        let hashes = retain_native(root.path(), retained.path(), &check, &output)?;
        assert_eq!(hashes.len(), 2);
        for (name, bytes) in [("one", b"one"), ("two", b"two")] {
            let relative = format!("verification/results/{name}.json");
            assert_eq!(hashes[&relative], source::digest(bytes));
            assert_eq!(fs::read(retained.path().join(relative))?, bytes);
        }
        validate_native(retained.path(), &hashes)?;
        fs::write(directory.join("one.json"), b"source changed after capture")?;
        validate_native(retained.path(), &hashes)?;
        let path = retained.path().join("verification/results/one.json");
        fs::write(&path, b"changed")?;
        assert!(validate_native(retained.path(), &hashes).is_err());
        fs::remove_file(path)?;
        assert!(validate_native(retained.path(), &hashes).is_err());
        let traversal = BTreeMap::from([("../outside.json".into(), source::digest(b"one"))]);
        assert!(validate_native(retained.path(), &traversal).is_err());
        assert!(source::evidence_path(root.path(), Path::new("/absolute.json")).is_err());
        Ok(())
    }

    #[test]
    fn a_duplicated_handle_cannot_keep_a_completed_session_locked() -> Result<()> {
        let home = tempfile::tempdir()?;
        let directory = crate::answer::directory(home.path(), "session")?;
        fs::create_dir_all(&directory)?;
        crate::answer::write_json(
            &directory.join("request.json"),
            &serde_json::json!({"version": 1, "generation": ".tmpfixture", "prompt": "Verify the requested change."}),
        )?;
        let state = load(home.path(), "session")?;
        let inherited = state.lock.try_clone()?;
        drop(state);
        let successor = load(home.path(), "session")?;
        drop(successor);
        drop(inherited);
        Ok(())
    }

    #[test]
    fn native_file_lock_modes_admit_shared_reads_and_exclude_writes() -> Result<()> {
        let home = tempfile::tempdir()?;
        let directory = crate::answer::directory(home.path(), "session")?;
        fs::create_dir_all(&directory)?;
        crate::answer::write_json(
            &directory.join("request.json"),
            &serde_json::json!({"version": 1, "generation": ".tmpfixture", "prompt": "Verify the requested change."}),
        )?;
        let missing_plan = execute(Action::Begin {
            session: "session".to_owned(),
            plan: directory.join("missing-plan.json"),
        })
        .unwrap_err();
        assert_eq!(
            missing_plan
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::NotFound)
        );
        let first = load_locked(home.path(), "session", Access::Read)?;
        let second = load_locked(home.path(), "session", Access::Read)?;
        check_session(home.path(), "session")?;
        assert!(load(home.path(), "session").is_err());
        drop(second);
        drop(first);
        let writer = load(home.path(), "session")?;
        assert!(load(home.path(), "session").is_err());
        assert!(load_locked(home.path(), "session", Access::Read).is_err());
        assert!(check_session(home.path(), "session").is_err());
        drop(writer);
        check_session(home.path(), "session")?;
        Ok(())
    }
}
