use super::{Candidate, GateRun, Reused, Stage, Target, git_text, input_hashes};
use crate::hosts_rules::{Family, PendingAction, PendingState, pending_action};
use crate::runtime::Real;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const DIRECTORY: &str = "hosts-pending";
const BUNDLE: &str = "head.bundle";
const SCHEMA: u32 = 1;
const SUBMISSION_LENGTH: usize = 32;
const SEQUENCE_HEX_LENGTH: usize = SUBMISSION_LENGTH / 2;

#[derive(Debug, Deserialize, Serialize)]
#[serde(remote = "PendingState", rename_all = "lowercase")]
enum State {
    Prepared,
    Submitted,
    Unverified,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Execution {
    schema: u32,
    commit: String,
    tree: String,
    host: String,
    os: String,
    gate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input_schema: Option<u32>,
    inputs: [String; 2],
    reused: [Option<Reused>; 2],
    submission: String,
    bundle_hash: String,
    program: Vec<String>,
    #[serde(default)]
    local: bool,
    #[serde(with = "State")]
    state: PendingState,
}

impl Execution {
    fn validate(&self, root: &Path, family: Family) -> Result<()> {
        ensure!(
            self.schema == SCHEMA && self.os == family.name(),
            "execution has another schema or family"
        );
        ensure!(
            self.submission.len() == SUBMISSION_LENGTH
                && self.submission.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid submission identity"
        );
        ensure!(
            !self.host.is_empty()
                && self
                    .host
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()
                        || matches!(character, '-' | '_' | '.')),
            "invalid host identity"
        );
        ensure!(
            !self.program.is_empty()
                && self.program.iter().all(|argument| !argument.contains('\0')),
            "invalid frozen submission program"
        );
        ensure!(
            git_text(
                root,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("{}^{{commit}}", self.commit)
                ]
            )? == self.commit,
            "execution has another commit identity"
        );
        ensure!(
            git_text(root, &["rev-parse", &format!("{}^{{tree}}", self.commit)])? == self.tree,
            "execution has another committed tree"
        );
        match self.input_schema {
            Some(super::INPUT_SCHEMA) => ensure!(
                input_hashes(root, &self.commit, &self.gate)? == self.inputs,
                "execution has other committed inputs"
            ),
            None => ensure!(
                self.state == PendingState::Unverified
                    && self.inputs.iter().all(|input| valid_digest(input)),
                "legacy execution must be terminal with valid input digests"
            ),
            Some(_) => anyhow::bail!("execution has an unsupported input schema"),
        }
        ensure!(
            valid_digest(&self.bundle_hash),
            "invalid source bundle digest"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnverifiedTerminal {
    execution: Execution,
    cutoff: u64,
    notes_revision: Option<String>,
    status: Option<i32>,
    output_hash: String,
}

fn history_path(root: &Path, family: Family) -> Result<PathBuf> {
    Ok(common(root)?
        .join(DIRECTORY)
        .join(format!("{}.history.json", family.name())))
}

fn append_history(path: &Path, record: &UnverifiedTerminal, empty: bool) -> Result<()> {
    let mut bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => b"[]".to_vec(),
        Err(error) => return Err(error.into()),
    };
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .context("empty unverified execution history")?;
    ensure!(
        bytes[end] == b']',
        "unverified execution history is not an array"
    );
    let mut addition = Vec::new();
    if !empty {
        addition.push(b',');
    }
    addition.extend(serde_json::to_vec(record)?);
    bytes.splice(end..end, addition);
    crate::runtime::replace(path, &bytes, false)
}

fn history(root: &Path, family: Family) -> Result<Vec<UnverifiedTerminal>> {
    let records: Vec<UnverifiedTerminal> = match fs::read(history_path(root, family)?) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).context("malformed unverified execution history")?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    for record in &records {
        record.execution.validate(root, family)?;
        ensure!(
            record.execution.state == PendingState::Unverified && valid_digest(&record.output_hash),
            "invalid terminal execution history"
        );
        let count = record
            .notes_revision
            .as_deref()
            .map(|revision| -> Result<u64> {
                ensure!(
                    git_text(
                        root,
                        &["rev-parse", "--verify", &format!("{revision}^{{commit}}")]
                    )? == revision,
                    "history has another notes revision"
                );
                super::git(
                    root,
                    &["merge-base", "--is-ancestor", revision, super::NOTES],
                )
                .context("history ordering revision left the hosts notes history")?;
                Ok(git_text(root, &["rev-list", "--count", revision])?.parse()?)
            })
            .transpose()?
            .unwrap_or(0);
        ensure!(
            record.cutoff == count,
            "history cutoff differs from its notes revision"
        );
    }
    Ok(records)
}

pub(super) fn cutoff(
    root: &Path,
    family: Family,
    stage: Stage,
    inputs: &str,
    gate: &str,
) -> Result<Option<u64>> {
    let history = history(root, family)?;
    Ok(history
        .iter()
        .filter(|entry| {
            entry.execution.input_schema.is_none()
                || (entry.execution.inputs[stage.index()] == inputs
                    && (stage == Stage::Real || entry.execution.gate == gate)
                    && (!entry.execution.local || entry.execution.reused[stage.index()].is_none()))
        })
        .map(|entry| entry.cutoff)
        .max())
}

fn notes_barrier(root: &Path) -> Result<(Option<String>, u64)> {
    let revision = git_text(
        root,
        &["for-each-ref", "--format=%(objectname)", super::NOTES],
    )?;
    if revision.is_empty() {
        Ok((None, 0))
    } else {
        let count = git_text(root, &["rev-list", "--count", &revision])?.parse()?;
        Ok((Some(revision), count))
    }
}

pub(super) struct Lease {
    directory: PathBuf,
    family: Family,
    _lock: fs::File,
}

fn digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes))
}

fn valid_digest(value: &str) -> bool {
    const SHA256_LENGTH: usize = 64;
    value.len() == SHA256_LENGTH && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn common(root: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git_text(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?))
}

pub(super) fn exists(root: &Path, family: Family) -> Result<bool> {
    match fs::symlink_metadata(
        common(root)?
            .join(DIRECTORY)
            .join(format!("{}.json", family.name())),
    ) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

impl Lease {
    pub(super) fn acquire(root: &Path, family: Family) -> Result<Self> {
        let common = common(root)?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(common.join(format!("hosts-pending-{}.lock", family.name())))?;
        lock.lock().context("lock the family's pending execution")?;
        Ok(Self {
            directory: common.join(DIRECTORY),
            family,
            _lock: lock,
        })
    }

    fn path(&self) -> PathBuf {
        self.directory.join(format!("{}.json", self.family.name()))
    }

    fn snapshot(&self) -> PathBuf {
        self.directory.join(self.family.name())
    }

    fn read(&self, root: &Path) -> Result<Option<Execution>> {
        let metadata = match fs::symlink_metadata(self.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.file_type().is_file(),
            "pending execution must be a regular file"
        );
        let execution: Execution = serde_json::from_slice(&fs::read(self.path())?)
            .context("malformed pending execution")?;
        execution.validate(root, self.family)?;
        if execution.state == PendingState::Unverified {
            let expected = serde_json::to_value(&execution)?;
            let records = history(root, self.family)?
                .iter()
                .map(|record| serde_json::to_value(&record.execution))
                .collect::<serde_json::Result<Vec<_>>>()?;
            ensure!(
                records.contains(&expected),
                "unverified execution has no matching terminal history"
            );
            return Ok(Some(execution));
        }
        let bundle = self.snapshot().join(BUNDLE);
        ensure!(
            fs::symlink_metadata(&bundle)?.file_type().is_file(),
            "pending source must be a regular bundle"
        );
        ensure!(
            digest(&fs::read(&bundle)?) == execution.bundle_hash,
            "pending source bundle changed"
        );
        ensure!(
            git_text(
                root,
                &["bundle", "list-heads", &bundle.to_string_lossy(), "HEAD"]
            )? == format!("{} HEAD", execution.commit),
            "pending bundle holds another candidate"
        );
        Ok(Some(execution))
    }

    fn write(&self, execution: &Execution) -> Result<()> {
        crate::runtime::replace(&self.path(), &serde_json::to_vec(execution)?, false)
    }

    fn prepare(
        &self,
        candidate: &Candidate<'_>,
        snapshot: &Path,
        host: &str,
        reused: [Option<Reused>; 2],
    ) -> Result<Execution> {
        ensure!(
            self.read(candidate.root)?
                .is_none_or(|previous| previous.state == PendingState::Unverified),
            "a pending execution already owns this family"
        );
        fs::create_dir_all(self.snapshot())?;
        let bundle = self.snapshot().join(BUNDLE);
        crate::runtime::replace(&bundle, &fs::read(snapshot.join(BUNDLE))?, false)?;
        ensure!(
            git_text(
                candidate.root,
                &["bundle", "list-heads", &bundle.to_string_lossy(), "HEAD"]
            )? == format!("{} HEAD", candidate.commit),
            "candidate changed before submission"
        );
        let counter = self
            .directory
            .join(format!("{}.sequence", self.family.name()));
        let previous: u64 = match fs::read(&counter) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("malformed execution sequence")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        let sequence = crate::hosts_rules::next_generation(previous)
            .context("execution sequence exhausted")?;
        crate::runtime::replace(&counter, &serde_json::to_vec(&sequence)?, false)?;
        let identity =
            serde_json::to_vec(&(self.directory.to_string_lossy(), self.family.name(), host))?;
        let submission = format!(
            "{}{:0width$x}",
            &digest(&identity)[..SEQUENCE_HEX_LENGTH],
            sequence,
            width = SEQUENCE_HEX_LENGTH
        );
        let skipped = reused.each_ref().map(Option::is_some);
        let program = super::wrap(
            self.family,
            &super::remote_gate_stages(self.family, candidate.gate, skipped),
        );
        let execution = Execution {
            schema: SCHEMA,
            commit: candidate.commit.to_owned(),
            tree: candidate.tree.to_owned(),
            host: host.to_owned(),
            os: self.family.name().to_owned(),
            gate: candidate.gate.to_owned(),
            input_schema: Some(super::INPUT_SCHEMA),
            inputs: candidate.inputs.clone(),
            reused,
            submission,
            bundle_hash: digest(&fs::read(&bundle)?),
            program,
            local: false,
            state: PendingState::Prepared,
        };
        Ok(execution)
    }

    fn run(&self, native: &Real, mut execution: Execution) -> Result<(Execution, GateRun)> {
        let reference = format!("{}:{}", execution.host, execution.submission);
        if pending_action(execution.state, false, false) == PendingAction::Submit {
            let mut submission = super::command(native, Tool::Domyjob);
            submission
                .current_dir(self.snapshot())
                .args([
                    "run",
                    &execution.host,
                    "--submission",
                    &execution.submission,
                    "--",
                ])
                .args(&execution.program);
            let (status, output) = super::job_output(submission, Some(super::REQUEST_LIMIT))?;
            ensure!(
                status.success(),
                "{reference}: submit pending remote job: {output}; resume with `dotfiles hosts check`"
            );
            ensure!(
                output.lines().any(|line| line.trim() == reference),
                "submission returned another job identity: {output}"
            );
            execution.state = PendingState::Submitted;
            self.write(&execution)?;
            println!("{reference}: submitted");
        } else {
            println!("{reference}: resuming");
        }
        let (status, output) = super::wait_job(
            native,
            &execution.host,
            &reference,
            format!("{reference}\n"),
        )?;
        let run = GateRun {
            family: self.family,
            target: Target::Remote(execution.host.clone()),
            status,
            output,
            reused: execution.reused.clone(),
            capture: None,
        };
        Ok((execution, run))
    }

    fn finish(
        &self,
        native: &Real,
        root: &Path,
        execution: &Execution,
        run: &GateRun,
    ) -> Result<Vec<String>> {
        if !run
            .output
            .lines()
            .any(|line| line.starts_with(super::STAGE_REPORT))
        {
            return self.unverified(root, execution, run);
        }
        let (report, workspace) = super::stage_packet(run)?;
        if crate::hosts_rules::report_evidence(run.status, report.code == 0 && report.real == 0)
            == crate::hosts_rules::ReportEvidence::Unverified
        {
            return self.unverified(root, execution, run);
        }
        ensure!(
            workspace.is_some(),
            "pending execution returned no workspace identity"
        );
        let result = super::record_run(
            native,
            root,
            &execution.commit,
            &execution.tree,
            &execution.gate,
            &execution.inputs,
            run,
        )?;
        if pending_action(execution.state, true, result.recorded) == PendingAction::Forget {
            self.retire()?;
        }
        Ok(result.failures)
    }

    fn unverified(&self, root: &Path, execution: &Execution, run: &GateRun) -> Result<Vec<String>> {
        self.mark_unverified(root, execution, run.status, &run.output)?;
        anyhow::bail!(
            "{}:{} terminated without verified stage evidence; unverified history blocks old credit; run `dotfiles hosts check` for a fresh attempt",
            execution.host,
            execution.submission
        );
    }

    fn mark_unverified(
        &self,
        root: &Path,
        execution: &Execution,
        status: Option<i32>,
        output: &str,
    ) -> Result<()> {
        let _notes = super::notes_lock(root)?;
        let records = history(root, self.family)?;
        let mut execution = execution.clone();
        execution.state = PendingState::Unverified;
        if !records
            .iter()
            .any(|entry| entry.execution.submission == execution.submission)
        {
            let (notes_revision, cutoff) = notes_barrier(root)?;
            let record = UnverifiedTerminal {
                execution: execution.clone(),
                cutoff,
                notes_revision,
                status,
                output_hash: digest(output.as_bytes()),
            };
            append_history(
                &history_path(root, self.family)?,
                &record,
                records.is_empty(),
            )?;
        }
        self.write(&execution)?;
        Ok(())
    }

    pub(super) fn begin_local(
        &self,
        candidate: &Candidate<'_>,
        snapshot: &Path,
        host: &str,
        reused: [Option<Reused>; 2],
    ) -> Result<()> {
        let mut execution = self.prepare(candidate, snapshot, host, reused)?;
        execution.local = true;
        self.mark_unverified(
            candidate.root,
            &execution,
            None,
            "local protected candidate capture started",
        )
    }

    fn retire(&self) -> Result<()> {
        fs::remove_file(self.path())?;
        platform::cleanup::report(
            fs::remove_file(self.snapshot().join(BUNDLE)),
            "remove the completed execution's source bundle",
        );
        platform::cleanup::report(
            fs::remove_dir(self.snapshot()),
            "remove the completed execution's source directory",
        );
        Ok(())
    }

    pub(super) fn retire_unverified(&self, root: &Path) -> Result<()> {
        if self
            .read(root)?
            .is_some_and(|execution| execution.state == PendingState::Unverified)
        {
            self.retire()?;
        }
        Ok(())
    }

    pub(super) fn recover(&self, native: &Real, root: &Path) -> Result<Option<Vec<String>>> {
        let Some(execution) = self.read(root)? else {
            return Ok(None);
        };
        if pending_action(execution.state, false, false) == PendingAction::Restart {
            return Ok(None);
        }
        let (execution, run) = self.run(native, execution)?;
        Ok(Some(self.finish(native, root, &execution, &run)?))
    }

    pub(super) fn execute(
        &self,
        native: &Real,
        candidate: &Candidate<'_>,
        snapshot: &Path,
        host: &str,
        reused: [Option<Reused>; 2],
    ) -> Result<Vec<String>> {
        let execution = self.prepare(candidate, snapshot, host, reused)?;
        self.write(&execution)?;
        let (execution, run) = self.run(native, execution)?;
        self.finish(native, candidate.root, &execution, &run)
    }
}
