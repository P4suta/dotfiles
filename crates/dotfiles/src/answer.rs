//! Evidence for one exact answer and one request, independent of skill maintenance.

use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use rules::answer_rules::{self, Check, Evidence, Proof};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

mod arguments;
mod lexicon;
mod relations;
mod sources;

use arguments::{Argument, Formula};
use lexicon::Reading;
use sources::Source;

const STATE: &str = ".local/state/dotfiles/answers";
const PROVER_CONFIG: &str = ".config/dotfiles/answer-proof.json";
const VERSION: u8 = 1;
const RETAINED_VERSION: u8 = 2;
const MAX_INPUT_BYTES: u64 = 262_144;
const MAX_CLAIMS: usize = 128;
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const MAX_SESSION_BYTES: usize = 256;
const MAX_GENERATION_BYTES: usize = 128;
const SHA256_HEX_BYTES: usize = 64;
const MAX_PROOF_BYTES: u64 = 1_048_576;
const PLAN_OPEN: &str = "<proposed_plan>";
const PLAN_CLOSE: &str = "</proposed_plan>";
const MIN_FENCE_WIDTH: usize = 3;
const MAX_FENCE_INDENT: usize = 3;
const LEAN_TOOLCHAIN: &str = include_str!("../../../specs/lean/lean-toolchain");

/// Read the same release pin as the proof package.
///
/// # Errors
///
/// Refuses a malformed release pin.
pub fn lean_version() -> Result<&'static str> {
    LEAN_TOOLCHAIN
        .trim()
        .strip_prefix("leanprover/lean4:v")
        .context("invalid Lean toolchain pin")
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Start a new request from a `UserPromptSubmit` hook payload on standard input.
    Begin,
    /// Check an answer draft, then bind its evidence to this request.
    Check {
        #[arg(long)]
        session: String,
        #[arg(long)]
        draft: PathBuf,
        #[arg(long)]
        evidence: PathBuf,
    },
    /// Capture an authorized read or project check as evidence for this request.
    Observe {
        #[arg(long)]
        session: String,
        #[arg(value_enum)]
        tool: ObservationTool,
        #[arg(last = true)]
        arguments: Vec<String>,
    },
    /// Check that the final reply matches this request's completed evidence.
    Stop,
}

#[derive(Debug, Deserialize)]
struct HookInput {
    session_id: String,
    cwd: Option<PathBuf>,
    prompt: Option<String>,
    last_assistant_message: Option<String>,
    #[serde(default)]
    stop_hook_active: bool,
}

/// A bounded, validated managed request, including its optional repository baseline.
#[derive(Debug, Deserialize, Serialize)]
#[serde(try_from = "UncheckedRequest")]
pub struct Request {
    version: u8,
    generation: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    assurance: Option<Assurance>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedRequest {
    version: u8,
    generation: String,
    prompt: String,
    #[serde(default)]
    assurance: Option<Assurance>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Assurance {
    root: PathBuf,
    commit: String,
}

impl TryFrom<UncheckedRequest> for Request {
    type Error = anyhow::Error;

    fn try_from(state: UncheckedRequest) -> Result<Self> {
        ensure!(
            state.version == VERSION,
            "unsupported answer request version"
        );
        ensure!(
            state.generation.starts_with(".tmp")
                && state.generation.len() <= MAX_GENERATION_BYTES
                && state.generation.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
                }),
            "invalid answer request generation"
        );
        ensure!(!state.prompt.trim().is_empty(), "answer request is empty");
        if let Some(context) = &state.assurance {
            ensure!(
                context.root.is_absolute()
                    && context
                        .root
                        .to_str()
                        .is_some_and(|root| !root.contains('\0')),
                "answer assurance root must be an absolute native path"
            );
            ensure!(
                crate::shape_rules::commit_id(context.commit.as_bytes()),
                "invalid answer assurance baseline commit"
            );
        }
        Ok(Self {
            version: state.version,
            generation: state.generation,
            prompt: state.prompt,
            assurance: state.assurance,
        })
    }
}

impl Request {
    fn identity(&self) -> Result<String> {
        Ok(digest(&serde_json::to_vec(self)?))
    }

    fn check_assurance(&self, home: &Path, cwd: &Path) -> Result<()> {
        if let Some(context) = &self.assurance {
            crate::assure::check_request(
                &context.root,
                &context.commit,
                cwd,
                &crate::assure::installed_at(home)?,
            )?;
        }
        Ok(())
    }
}

/// Parse the same bounded request format that observation, checking, and Stop consume.
///
/// # Errors
///
/// Refuses unknown fields, unsupported versions, unsafe generations, and malformed context.
pub fn inspect_request(input: &str) -> Result<Request> {
    ensure!(
        input.len() as u64 <= MAX_INPUT_BYTES,
        "answer input exceeds its byte bound"
    );
    serde_json::from_str(input).context("invalid answer request")
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u8,
    generation: String,
    request_sha256: String,
    draft_sha256: String,
    evidence_sha256: String,
    producer: String,
    sources: BTreeMap<String, String>,
    proof: Option<ProofEvidence>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProofEvidence {
    image: String,
    source_sha256: String,
    report_sha256: String,
    arguments: usize,
}

#[derive(Debug)]
struct ProofRun {
    image: String,
    report: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    atoms: Vec<String>,
    claims: Vec<Claim>,
    readings: Vec<Reading>,
}

/// A parsed draft plan has no retrieved-source or completed-proof authority.
#[derive(Debug)]
pub struct Plan {
    document: Document,
    proof: String,
    arguments: usize,
}

/// Inspect the bounded input and its declared formal and lexical contracts without effects.
///
/// # Errors
///
/// Refuses malformed, unsupported, inconsistent, circular, or uncovered claims.
pub fn inspect(draft: &str, evidence: &str, prompt: &str) -> Result<Plan> {
    ensure!(
        [draft, evidence, prompt]
            .into_iter()
            .all(|input| input.len() as u64 <= MAX_INPUT_BYTES),
        "answer input exceeds its byte bound"
    );
    ensure!(
        !draft.contains('\r'),
        "answer draft must use LF line endings"
    );
    let document: Document = serde_json::from_str(evidence).context("invalid answer evidence")?;
    let (proof, arguments) = coverage(draft, &document, prompt)?;
    for claim in &document.claims {
        if let Basis::Source { source } = &claim.basis {
            sources::validate(source)?;
        }
    }
    for reading in &document.readings {
        lexicon::inspect(reading, draft)?;
    }
    Ok(Plan {
        document,
        proof,
        arguments,
    })
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    id: String,
    text: String,
    formula: Option<Formula>,
    basis: Basis,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Basis {
    Source {
        source: Source,
    },
    Observation {
        record: String,
        excerpt: String,
        exit: i32,
    },
    Comparison {
        comparison: Box<relations::Comparison>,
    },
    Parallel {
        parallel: relations::Parallel,
    },
    User {
        quote: String,
    },
    Hypothesis {
        reason: String,
    },
    Inference {
        argument: Argument,
        conditional: bool,
    },
    Proposal {
        reason: String,
    },
    Unverified {
        reason: String,
    },
    Question,
}

fn digest(bytes: &[u8]) -> String {
    platform::build_inputs::hex(&Sha256::digest(bytes))
}

fn draft_digest(text: &str) -> String {
    digest(text.strip_suffix('\n').unwrap_or(text).as_bytes())
}

fn read(path: &Path) -> Result<String> {
    read_limit(path, MAX_INPUT_BYTES)
}

fn read_limit(path: &Path, limit: u64) -> Result<String> {
    let file = fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "answer input must be a regular file"
    );
    let mut text = String::new();
    file.take(limit + 1).read_to_string(&mut text)?;
    ensure!(
        text.len() as u64 <= limit,
        "answer input exceeds its byte bound"
    );
    Ok(text)
}

fn write_json(path: &Path, document: &impl Serialize) -> Result<()> {
    write_atomic(path, &serde_json::to_vec(document)?)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("answer state has no parent")?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(parent, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))?;
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.as_file_mut().write_all(bytes)?;
    file.as_file_mut().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn directory(home: &Path, session: &str) -> Result<PathBuf> {
    ensure!(
        !session.is_empty() && session.len() <= MAX_SESSION_BYTES,
        "missing or oversized answer session ID"
    );
    Ok(home.join(STATE).join(digest(session.as_bytes())))
}

fn request(directory: &Path) -> Result<Request> {
    inspect_request(
        &read(&directory.join("request.json"))
            .context("answer request is missing; run the managed UserPromptSubmit hook")?,
    )
}

fn begin(home: &Path, input: &HookInput) -> Result<String> {
    let prompt = input
        .prompt
        .as_deref()
        .context("answer request has no prompt")?;
    ensure!(!prompt.trim().is_empty(), "answer request is empty");
    let directory = directory(home, &input.session_id)?;
    fs::create_dir_all(&directory)?;
    let generation = tempfile::NamedTempFile::new_in(&directory)?;
    let generation = generation
        .path()
        .file_name()
        .context("request generation has no name")?
        .to_string_lossy()
        .into_owned();
    write_json(
        &directory.join("request.json"),
        &Request {
            version: VERSION,
            generation,
            prompt: prompt.to_owned(),
            assurance: None,
        },
    )?;
    Ok(format!(
        "Before completing this request, use japanese-lexicon for Japanese readings and checked-arguments for reasoning.\nPrepare an exact answer draft and evidence, then run dotfiles answer check --session {} --draft <file> --evidence <file>.\nThe Stop hook rejects absent or stale evidence.\nA proof checks the declared formal claim; it does not certify empirical premises or their natural-language interpretation.\n",
        platform::refusal::quote(&input.session_id)
    ))
}

struct Derivation {
    proof: String,
    hypothetical: bool,
}

fn derivation(
    document: &Document,
    claim: &Claim,
    argument: &Argument,
    conditional: bool,
    claims: &BTreeMap<&str, (&Claim, bool)>,
    index: usize,
) -> Result<Derivation> {
    ensure!(
        !argument.premises.is_empty(),
        "an inference needs explicit earlier premises"
    );
    let mut seen = BTreeSet::new();
    let mut premises = Vec::new();
    let mut hypothetical = false;
    for id in &argument.premises {
        ensure!(seen.insert(id), "duplicate argument premise");
        let (premise, conditional) = claims.get(id.as_str()).with_context(|| {
            format!("claim {} names a missing or forward premise {id}", claim.id)
        })?;
        ensure!(
            !matches!(
                premise.basis,
                Basis::Proposal { .. }
                    | Basis::Unverified { .. }
                    | Basis::Question
                    | Basis::Parallel { .. }
                    | Basis::Comparison { .. }
            ),
            "an unverified claim or a relation declaration cannot support a formal inference"
        );
        hypothetical |= conditional;
        premises.push(
            premise
                .formula
                .as_ref()
                .context("an argument premise has no formal interpretation")?,
        );
    }
    ensure!(
        claim.formula.as_ref() == Some(&argument.conclusion),
        "claim formula differs from its argument conclusion"
    );
    ensure!(
        !hypothetical || conditional,
        "a hypothetical premise needs a conditional conclusion"
    );
    if conditional {
        labeled(
            &claim.text,
            "conditional",
            &["仮定の下で:", "仮定の下で：", "Under these assumptions:"],
        )?;
    }
    Ok(Derivation {
        proof: arguments::theorem(index, &document.atoms, &premises, &argument.conclusion)?,
        hypothetical,
    })
}

struct MarkdownFence {
    marker: u8,
    width: usize,
}

fn code_line(line: &str, fence: &mut Option<MarkdownFence>) -> bool {
    let inside = fence.is_some();
    let text = line.trim_start_matches(' ');
    if line.len() - text.len() > MAX_FENCE_INDENT {
        return inside;
    }
    let Some(&marker) = text.as_bytes().first() else {
        return inside;
    };
    if !matches!(marker, b'`' | b'~') {
        return inside;
    }
    let width = text.bytes().take_while(|&byte| byte == marker).count();
    let Some(rest) = text.get(width..) else {
        return inside;
    };
    if width < MIN_FENCE_WIDTH {
        return inside;
    }
    if let Some(active) = fence {
        if marker == active.marker
            && width >= active.width
            && rest.trim_matches([' ', '\t']).is_empty()
        {
            *fence = None;
        }
    } else if marker == b'~' || !rest.contains('`') {
        *fence = Some(MarkdownFence { marker, width });
    }
    inside || fence.is_some()
}

fn proposal_scopes(lines: &[&str]) -> Result<Vec<bool>> {
    use answer_rules::{PlanDelimiter, ProposalRegion};
    let mut region = ProposalRegion::Prose;
    let mut fence = None;
    let mut scopes = Vec::with_capacity(lines.len());
    for &line in lines {
        let delimiter = if code_line(line, &mut fence) {
            PlanDelimiter::Text
        } else if line == PLAN_OPEN {
            PlanDelimiter::Open
        } else if line == PLAN_CLOSE {
            PlanDelimiter::Close
        } else {
            PlanDelimiter::Text
        };
        let step = answer_rules::proposal_step(region, delimiter)
            .map_err(|error| anyhow::anyhow!("invalid proposal block: {error:?}"))?;
        region = step.region;
        scopes.push(step.in_plan);
    }
    ensure!(
        answer_rules::proposal_finished(region),
        "proposal block is missing its closing delimiter"
    );
    Ok(scopes)
}

fn coverage(draft: &str, document: &Document, prompt: &str) -> Result<(String, usize)> {
    ensure!(
        document.version == VERSION,
        "unsupported answer evidence version"
    );
    ensure!(
        !document.claims.is_empty() && document.claims.len() <= MAX_CLAIMS,
        "answer evidence needs 1..={MAX_CLAIMS} claims"
    );
    let lines = draft
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    ensure!(
        lines.len() == document.claims.len(),
        "each nonempty draft line needs exactly one claim"
    );
    let scopes = proposal_scopes(&lines)?;
    let mut claims: BTreeMap<&str, (&Claim, bool)> = BTreeMap::new();
    let mut proof = String::new();
    let mut count = 0;
    for ((line, in_plan), claim) in lines.into_iter().zip(scopes).zip(&document.claims) {
        ensure!(
            line == claim.text,
            "claim {} differs from the exact draft line",
            claim.id
        );
        ensure!(
            !claim.id.trim().is_empty() && !claims.contains_key(claim.id.as_str()),
            "empty or duplicate claim ID"
        );
        let mut hypothetical = false;
        match &claim.basis {
            Basis::Source { source } => ensure!(
                line.contains(&source.url),
                "claim {} omits its source link",
                claim.id
            ),
            Basis::Observation { .. } => ensure!(
                line.starts_with("観測:")
                    || line.starts_with("観測：")
                    || line.starts_with("Observation:"),
                "a tool observation needs its visible label"
            ),
            Basis::Comparison { comparison } => {
                comparison.check(line)?;
                for basis in comparison.bases() {
                    let Some((source, _)) = claims.get(basis) else {
                        anyhow::bail!("comparison names a missing or forward evidence claim");
                    };
                    ensure!(
                        matches!(
                            source.basis,
                            Basis::Source { .. } | Basis::Observation { .. } | Basis::User { .. }
                        ),
                        "a comparison measurement needs a source, request testimony, or a tool observation"
                    );
                }
            }
            Basis::Parallel { parallel } => parallel.check(line)?,
            Basis::User { quote } => {
                ensure!(
                    !quote.is_empty() && prompt.contains(quote),
                    "claim {} does not quote this user's request",
                    claim.id
                );
                ensure!(
                    line.contains(quote),
                    "claim {} omits the attributed user statement",
                    claim.id
                );
            }
            Basis::Hypothesis { reason } => {
                labeled(line, reason, &["仮説:", "仮説：", "Hypothesis:"])?;
                hypothetical = true;
            }
            Basis::Inference {
                argument,
                conditional,
            } => {
                let derived = derivation(document, claim, argument, *conditional, &claims, count)?;
                hypothetical = derived.hypothetical;
                proof.push_str(&derived.proof);
                count += 1;
            }
            Basis::Proposal { reason } => proposal(line, reason, in_plan)?,
            Basis::Unverified { reason } => {
                labeled(line, reason, &["未検証:", "未検証：", "Unverified:"])?;
            }
            Basis::Question => ensure!(
                line.ends_with(['?', '？']),
                "a question must end with a question mark"
            ),
        }
        claims.insert(&claim.id, (claim, hypothetical));
    }
    Ok((proof, count))
}

fn proposal(line: &str, reason: &str, in_plan: bool) -> Result<()> {
    if in_plan {
        ensure!(!reason.trim().is_empty(), "a proposal needs its reason");
        Ok(())
    } else {
        labeled(line, reason, &["提案:", "提案：", "Proposal:"])
    }
}

fn labeled(line: &str, reason: &str, prefixes: &[&str]) -> Result<()> {
    ensure!(
        !reason.trim().is_empty(),
        "a nonfactual statement needs its reason"
    );
    ensure!(
        prefixes.iter().any(|prefix| line.starts_with(prefix)),
        "the answer must label this statement: {}",
        prefixes.join(" / ")
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ObservationTool {
    Git,
    Gh,
    Cargo,
    Just,
}

impl ObservationTool {
    fn tool(self, arguments: &[String]) -> Result<platform::tool::Tool> {
        use platform::tool::Tool;
        let allowed = match self {
            Self::Git => matches!(
                arguments.first().map(String::as_str),
                Some("status" | "diff" | "show" | "log" | "rev-parse" | "--version")
            ),
            Self::Gh => {
                matches!(arguments, [group, verb, ..] if matches!(group.as_str(), "pr" | "issue" | "repo") && verb == "view")
            }
            Self::Cargo => matches!(
                arguments.first().map(String::as_str),
                Some("check" | "test" | "clippy")
            ),
            Self::Just => matches!(
                arguments.first().map(String::as_str),
                Some("check" | "test" | "lint" | "typecheck" | "ci")
            ),
        };
        ensure!(
            allowed,
            "observation permits read commands and named project checks only"
        );
        Ok(match self {
            Self::Git => Tool::Git,
            Self::Gh => Tool::Gh,
            Self::Cargo => Tool::Cargo,
            Self::Just => Tool::Just,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    version: u8,
    generation: String,
    request_sha256: String,
    program: String,
    arguments: Vec<String>,
    directory: PathBuf,
    exit: i32,
    stdout: String,
    stderr: String,
}

fn observe(home: &Path, session: &str, tool: ObservationTool, arguments: &[String]) -> Result<()> {
    let directory = directory(home, session)?;
    let state = request(&directory)?;
    let tool = tool.tool(arguments)?;
    let output = tool
        .command()
        .args(arguments)
        .output()
        .context("run the authorized observation command")?;
    ensure!(
        request(&directory)?.identity()? == state.identity()?,
        "answer request changed during the observation"
    );
    ensure!(
        output.stdout.len() as u64 + output.stderr.len() as u64 <= MAX_INPUT_BYTES / 2,
        "observation exceeds its retained byte bound"
    );
    let observation = Observation {
        version: RETAINED_VERSION,
        request_sha256: state.identity()?,
        generation: state.generation,
        program: tool.program().to_owned(),
        arguments: arguments.to_vec(),
        directory: std::env::current_dir()?,
        exit: output
            .status
            .code()
            .context("observation command did not complete with an exit status")?,
        stdout: String::from_utf8(output.stdout)?,
        stderr: String::from_utf8(output.stderr)?,
    };
    let id = digest(&serde_json::to_vec(&observation)?);
    write_json(
        &directory.join("observations").join(format!("{id}.json")),
        &observation,
    )?;
    print!("{}{}", observation.stdout, observation.stderr);
    println!("Observation {id}: exit {}.", observation.exit);
    Ok(())
}

fn check_observation(
    directory: &Path,
    state: &Request,
    id: &str,
    excerpt: &str,
    exit: i32,
) -> Result<()> {
    ensure!(
        id.len() == SHA256_HEX_BYTES && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid observation record ID"
    );
    let text = read(&directory.join("observations").join(format!("{id}.json")))?;
    let observation: Observation = serde_json::from_str(&text)?;
    ensure!(
        digest(&serde_json::to_vec(&observation)?) == id,
        "observation content differs from its identity"
    );
    ensure!(
        observation.version == RETAINED_VERSION
            && observation.generation == state.generation
            && observation.request_sha256 == state.identity()?,
        "observation belongs to another answer request"
    );
    ensure!(
        observation.exit == exit,
        "observation exit status differs from the claimed status"
    );
    ensure!(
        !excerpt.is_empty()
            && (observation.stdout.contains(excerpt) || observation.stderr.contains(excerpt)),
        "observation does not contain the declared excerpt"
    );
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Prover {
    pub version: u8,
    pub image: String,
    pub lean_version: String,
}

fn prove(home: &Path, source: &str, count: usize) -> Result<ProofRun> {
    let config: Prover = serde_json::from_str(&read(&home.join(PROVER_CONFIG)).context(
        "answer prover is missing; run `mise x -- just install-answer-prover` in dotfiles",
    )?)?;
    let version = lean_version()?;
    ensure!(
        config.version == VERSION && config.lean_version == version,
        "answer prover configuration differs from the pinned release"
    );
    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("Answer.lean"), source)?;
    let mut mount = std::ffi::OsString::from("type=bind,\"source=");
    mount.push(directory.path());
    mount.push("\",target=/proof,readonly");
    let mut arguments: Vec<std::ffi::OsString> = [
        "run",
        "--rm",
        "--network",
        "none",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,exec,size=256m",
        "--mount",
    ]
    .map(Into::into)
    .into();
    arguments.push(mount);
    arguments.extend([
        config.image.as_str(), "bash", "-c",
        "set -eu; /opt/lean/bin/lean --version; cp /proof/Answer.lean /tmp/Answer.lean; sha256sum /tmp/Answer.lean; cd /tmp; /opt/lean/bin/lean -DwarningAsError=true -o Answer.olean Answer.lean; LEAN_PATH=/tmp /opt/lean/bin/leanchecker --fresh Answer; echo ANSWER_PROOF_COMPLETE",
    ].map(Into::into));
    let output = crate::bounded::run(&arguments, &config.image)?;
    ensure!(
        output.status.success(),
        "answer proof is incomplete or failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout)?;
    validate_report(&report, count, &digest(source.as_bytes()))?;
    Ok(ProofRun {
        image: config.image,
        report,
    })
}

fn validate_report(report: &str, count: usize, source_sha256: &str) -> Result<()> {
    let version = lean_version()?;
    ensure!(
        report
            .lines()
            .next()
            .is_some_and(|line| line.starts_with(&format!("Lean (version {version},"))),
        "the proof image runs another Lean release"
    );
    ensure!(
        report
            .lines()
            .filter(|line| *line == format!("{source_sha256}  /tmp/Answer.lean"))
            .count()
            == 1,
        "proof report names another source"
    );
    for index in 0..count {
        let expected = format!("'argument{index}' does not depend on any axioms");
        ensure!(
            report.lines().filter(|line| *line == expected).count() == 1,
            "argument {index} lacks its complete axiom report"
        );
    }
    ensure!(
        report.lines().last() == Some("ANSWER_PROOF_COMPLETE"),
        "answer proof did not complete its independent kernel replay"
    );
    Ok(())
}

fn check(home: &Path, session: &str, draft: &Path, evidence: &Path) -> Result<()> {
    check_at(home, session, draft, evidence, &std::env::current_dir()?)
}

fn check_at(home: &Path, session: &str, draft: &Path, evidence: &Path, cwd: &Path) -> Result<()> {
    let directory = directory(home, session)?;
    let state = request(&directory)?;
    state.check_assurance(home, cwd)?;
    let draft = read(draft)?;
    let evidence = read(evidence)?;
    let foreign = dotguard::lang::scan(&draft, dotguard::lang::Mode::Japanese);
    ensure!(
        foreign.is_empty(),
        "answer draft holds characters outside the Japanese and English repertoires; name a foreign character by its code point instead:\n{}",
        foreign
            .iter()
            .map(|hit| dotguard::lang::evidence("draft", hit))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let Plan {
        document,
        proof,
        arguments: count,
    } = inspect(&draft, &evidence, &state.prompt)?;
    let mut fetched = BTreeMap::new();
    for claim in &document.claims {
        match &claim.basis {
            Basis::Source { source } => sources::check(source, &mut fetched)?,
            Basis::Observation {
                record,
                excerpt,
                exit,
            } => check_observation(&directory, &state, record, excerpt, *exit)?,
            _ => {}
        }
    }
    for reading in &document.readings {
        lexicon::check(reading, &draft, &mut fetched)?;
    }
    let run = if count == 0 {
        None
    } else {
        Some(prove(home, &proof, count)?)
    };
    ensure!(
        request(&directory)?.identity()? == state.identity()?,
        "answer request changed during verification"
    );
    let artifacts = directory.join(&state.generation);
    write_atomic(&artifacts.join("draft.txt"), draft.as_bytes())?;
    write_atomic(&artifacts.join("evidence.json"), evidence.as_bytes())?;
    for (url, body) in &fetched {
        write_atomic(&source_path(&artifacts, url), body.as_bytes())?;
    }
    let proof = if let Some(run) = run {
        write_atomic(&artifacts.join("Answer.lean"), proof.as_bytes())?;
        write_atomic(&artifacts.join("proof.txt"), run.report.as_bytes())?;
        Some(ProofEvidence {
            image: run.image,
            source_sha256: digest(proof.as_bytes()),
            report_sha256: digest(run.report.as_bytes()),
            arguments: count,
        })
    } else {
        None
    };
    let receipt = Receipt {
        version: RETAINED_VERSION,
        request_sha256: state.identity()?,
        generation: state.generation,
        draft_sha256: draft_digest(&draft),
        evidence_sha256: digest(evidence.as_bytes()),
        producer: platform::BUILD_HASH.to_owned(),
        sources: fetched
            .into_iter()
            .map(|(url, body)| (url, digest(body.as_bytes())))
            .collect(),
        proof,
    };
    ensure!(
        request(&directory)?.identity()? == receipt.request_sha256,
        "answer request changed while retaining evidence"
    );
    write_json(&directory.join("receipt.json"), &receipt)?;
    println!(
        "Checked {} claim(s), {} reading(s), {count} Lean argument(s).",
        document.claims.len(),
        document.readings.len()
    );
    println!(
        "This checks declared evidence and formal entailment; natural-language interpretation and premise truth remain external assumptions."
    );
    Ok(())
}

fn stop(home: &Path, input: &HookInput) -> Result<()> {
    let cwd = input.cwd.clone().map_or_else(std::env::current_dir, Ok)?;
    stop_at(home, input, &cwd)
}

fn stop_at(home: &Path, input: &HookInput, cwd: &Path) -> Result<()> {
    let directory = directory(home, &input.session_id)?;
    let state = request(&directory)?;
    let receipt: Receipt = serde_json::from_str(&read(&directory.join("receipt.json")).context(
        "answer evidence is missing; check the exact answer draft before completing this request",
    )?)?;
    let draft = input
        .last_assistant_message
        .as_deref()
        .context("the Stop hook supplied no answer text")?;
    let binding = answer_rules::binding(
        receipt.version == RETAINED_VERSION
            && receipt.generation == state.generation
            && receipt.request_sha256 == state.identity()?,
        receipt.draft_sha256 == draft_digest(draft),
        receipt.producer == platform::BUILD_HASH,
    );
    ensure!(
        binding == answer_rules::Binding::Current,
        "answer evidence is stale or incomplete: another request, draft, or producer"
    );
    let artifacts = directory.join(&state.generation);
    let saved_draft = read(&artifacts.join("draft.txt"))?;
    let raw_evidence = read(&artifacts.join("evidence.json"))?;
    ensure!(
        receipt.draft_sha256 == draft_digest(&saved_draft)
            && receipt.evidence_sha256 == digest(raw_evidence.as_bytes()),
        "retained answer inputs differ from their identities"
    );
    let plan = inspect(&saved_draft, &raw_evidence, &state.prompt)?;
    check_retained_sources(&artifacts, &receipt, &plan.document)?;
    for claim in &plan.document.claims {
        if let Basis::Observation {
            record,
            excerpt,
            exit,
        } = &claim.basis
        {
            check_observation(&directory, &state, record, excerpt, *exit)?;
        }
    }
    check_retained_proof(&artifacts, &receipt, &plan)?;
    let evidence = Evidence {
        binding,
        coverage: Check::Complete,
        sources: Check::Complete,
        proof_required: plan.arguments != 0,
        proof: if receipt.proof.is_some() {
            Proof::Verified
        } else {
            Proof::NotRequired
        },
    };
    ensure!(
        answer_rules::may_complete(evidence),
        "answer evidence is stale or incomplete; check the exact final draft for this request"
    );
    state.check_assurance(home, cwd)?;
    ensure!(
        request(&directory)?.identity()? == state.identity()?,
        "answer request changed during the final evidence check"
    );
    Ok(())
}

fn source_path(directory: &Path, url: &str) -> PathBuf {
    directory.join(format!("source-{}.txt", digest(url.as_bytes())))
}

fn check_retained_sources(directory: &Path, receipt: &Receipt, document: &Document) -> Result<()> {
    let mut records = document
        .claims
        .iter()
        .filter_map(|claim| {
            if let Basis::Source { source } = &claim.basis {
                Some(source)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    records.extend(document.readings.iter().flat_map(lexicon::records));
    let expected = records
        .iter()
        .map(|source| sources::validate(source).map(|url| url.to_string()))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        expected.iter().eq(receipt.sources.keys()),
        "retained source inventory differs from the answer evidence"
    );
    let mut fetched = BTreeMap::new();
    for (url, expected) in &receipt.sources {
        let body = read_limit(
            &source_path(directory, url),
            sources::MAX_SOURCE_BYTES as u64,
        )?;
        ensure!(
            *expected == digest(body.as_bytes()),
            "retained source content differs from its identity"
        );
        fetched.insert(url.clone(), body);
    }
    for source in records {
        sources::check_saved(source, &fetched)?;
    }
    Ok(())
}

fn check_retained_proof(directory: &Path, receipt: &Receipt, plan: &Plan) -> Result<()> {
    let Some(proof) = &receipt.proof else {
        ensure!(plan.arguments == 0, "required proof artifacts are missing");
        return Ok(());
    };
    ensure!(
        plan.arguments != 0 && proof.arguments == plan.arguments,
        "retained proof covers another argument inventory"
    );
    ensure!(
        crate::bounded::pinned(&proof.image),
        "retained proof names a mutable image"
    );
    let source = read_limit(&directory.join("Answer.lean"), MAX_PROOF_BYTES)?;
    let report = read(&directory.join("proof.txt"))?;
    ensure!(
        source == plan.proof && proof.source_sha256 == digest(source.as_bytes()),
        "retained proof source differs from the exact answer arguments"
    );
    ensure!(
        proof.report_sha256 == digest(report.as_bytes()),
        "retained proof report differs from its identity"
    );
    validate_report(&report, plan.arguments, &proof.source_sha256)
}

/// Run the command or return an explicit hook refusal.
///
/// # Errors
///
/// Refuses absent requests, malformed evidence, unsupported sources, and missing proof evidence.
pub fn execute(action: Action) -> Result<()> {
    match action {
        Action::Check {
            session,
            draft,
            evidence,
        } => check(
            &platform::home::home().context("native home is unavailable")?,
            &session,
            &draft,
            &evidence,
        ),
        Action::Observe {
            session,
            tool,
            arguments,
        } => observe(
            &platform::home::home().context("native home is unavailable")?,
            &session,
            tool,
            &arguments,
        ),
        Action::Begin | Action::Stop => {
            let parsed = (|| -> Result<(PathBuf, HookInput)> {
                let home = platform::home::home().context("native home is unavailable")?;
                let mut text = String::new();
                std::io::stdin()
                    .take(MAX_INPUT_BYTES + 1)
                    .read_to_string(&mut text)?;
                ensure!(
                    text.len() as u64 <= MAX_INPUT_BYTES,
                    "answer hook input exceeds its byte bound"
                );
                Ok((
                    home,
                    serde_json::from_str(&text).context("invalid answer hook payload")?,
                ))
            })();
            let (home, input) = match parsed {
                Ok(parsed) => parsed,
                Err(error) => {
                    println!(
                        "{}",
                        serde_json::json!({"decision":"block","reason":format!("{error:#}"),"continue":false,"stopReason":format!("{error:#}"),"systemMessage":"Answer evidence could not be checked."})
                    );
                    return Ok(());
                }
            };
            if matches!(action, Action::Begin) {
                match begin(&home, &input) {
                    Ok(context) => print!("{context}"),
                    Err(error) => println!(
                        "{}",
                        serde_json::json!({"decision":"block","reason":format!("{error:#}")})
                    ),
                }
            } else {
                let reason = stop(&home, &input).err().map(|error| format!("{error:#}"));
                println!(
                    "{}",
                    crate::assure::stop_response(reason.as_deref(), input.stop_hook_active)
                );
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_git;
    use Formula::{Atom, Implies};
    use serde_json::json;

    fn input(prompt: &str) -> HookInput {
        HookInput {
            session_id: "session".to_owned(),
            cwd: None,
            prompt: Some(prompt.to_owned()),
            last_assistant_message: Some("Can we test this?".to_owned()),
            stop_hook_active: false,
        }
    }

    fn document() -> Document {
        serde_json::from_value(json!({
            "version":1,"atoms":[],"readings":[],
            "claims":[{"id":"question","text":"Can we test this?","formula":null,"basis":{"kind":"question"}}]
        })).unwrap()
    }

    struct ContextFixture {
        scope: tempfile::TempDir,
        home: PathBuf,
        root: PathBuf,
        config: PathBuf,
    }

    impl ContextFixture {
        fn new() -> Self {
            let scope = tempfile::tempdir().unwrap();
            let home = scope.path().join("home");
            let root = scope.path().join("repository with 'quotes'");
            let config = scope.path().join("gitconfig");
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&root).unwrap();
            fs::write(&config, "[user]\nname = Fixture\nemail = fixture@example.com\n[commit]\ngpgsign = false\n[init]\ndefaultBranch = main\n").unwrap();
            let fixture = Self {
                scope,
                home,
                root,
                config,
            };
            fixture.git(&["init", "--quiet"]);
            fs::write(fixture.root.join("README.md"), "fixture\n").unwrap();
            fixture.git(&["add", "--all"]);
            fixture.git(&["commit", "--quiet", "-m", "chore: create fixture"]);
            let catalog = fixture.home.join(crate::assure::CATALOG);
            fs::create_dir_all(catalog.parent().unwrap()).unwrap();
            fs::write(catalog, "version = 1\nexclude = []\n[[shape]]\nid = 'parser'\nrequirement = 'a parser owner'\nnext = 'add an owner'\nmarker = 'fuzz_target!'\nsignals = [{ suffix = '.rs', prefix = 'pub fn parse(' }]\n").unwrap();
            fixture
        }

        fn git(&self, arguments: &[&str]) -> String {
            let output = fixture_git::command("git", &self.config)
                .current_dir(&self.root)
                .args(arguments)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        }

        fn begin(&self) -> HookInput {
            let mut input = input("Prepare the project without publishing it.");
            input.cwd = Some(self.root.clone());
            begin(&self.home, &input).unwrap();
            let directory = directory(&self.home, "session").unwrap();
            let mut state = request(&directory).unwrap();
            state.assurance = Some(Assurance {
                root: self.root.clone(),
                commit: self.git(&["rev-parse", "HEAD"]),
            });
            write_json(&directory.join("request.json"), &state).unwrap();
            input
        }

        fn check(&self) {
            let draft = self.scope.path().join("draft.txt");
            let evidence = self.scope.path().join("evidence.json");
            fs::write(&draft, "Can we test this?\n").unwrap();
            write_json(&evidence, &document()).unwrap();
            check_at(&self.home, "session", &draft, &evidence, &self.root).unwrap();
        }
    }

    #[test]
    fn current_requests_keep_their_shape_and_context_parsing_is_strict() {
        let home = tempfile::tempdir().unwrap();
        begin(home.path(), &input("request")).unwrap();
        let directory = directory(home.path(), "session").unwrap();
        let current = serde_json::to_value(request(&directory).unwrap()).unwrap();
        assert_eq!(current.as_object().unwrap().len(), 3);
        let mut context = current.clone();
        context["assurance"] = json!({
            "root": home.path(),
            "commit": "ee178c449d6507de8045f8f79b92269da9b752fb"
        });
        for (field, value) in [
            ("root", json!("relative")),
            ("root", json!(format!("{}\0", home.path().display()))),
            ("commit", json!("HEAD")),
            ("commit", json!("ee178c4")),
            ("commit", json!(null)),
            ("unchecked", json!(true)),
        ] {
            let mut invalid = context.clone();
            invalid["assurance"][field] = value;
            assert!(inspect_request(&invalid.to_string()).is_err(), "{field}");
        }
        for field in ["root", "commit"] {
            let mut invalid = context.clone();
            invalid["assurance"].as_object_mut().unwrap().remove(field);
            assert!(
                inspect_request(&invalid.to_string()).is_err(),
                "missing {field}"
            );
        }
        let mut unknown = current.clone();
        unknown["unchecked"] = json!(true);
        assert!(inspect_request(&unknown.to_string()).is_err());
        let mut nullable = current.clone();
        nullable["assurance"] = json!(null);
        assert_eq!(
            serde_json::to_value(inspect_request(&nullable.to_string()).unwrap()).unwrap(),
            current
        );
        assert!(
            inspect_request(&" ".repeat(usize::try_from(MAX_INPUT_BYTES).unwrap() + 1)).is_err()
        );
        for entry in [current, context] {
            let parsed = inspect_request(&entry.to_string()).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), entry);
        }
    }

    #[test]
    fn contextual_completion_enforces_the_saved_baseline_and_repository() {
        let fixture = ContextFixture::new();
        let input = fixture.begin();
        fixture.check();
        assert!(stop(&fixture.home, &input).is_ok());
        let directory = directory(&fixture.home, "session").unwrap();
        let mut state = request(&directory).unwrap();
        assert!(
            state
                .check_assurance(&fixture.home, fixture.scope.path())
                .is_err()
        );
        let nested = fixture.root.join("nested");
        fs::create_dir(&nested).unwrap();
        assert!(state.check_assurance(&fixture.home, &nested).is_ok());
        let original_root = state.assurance.as_ref().unwrap().root.clone();
        state.assurance.as_mut().unwrap().root = nested;
        assert!(state.check_assurance(&fixture.home, &fixture.root).is_err());
        state.assurance.as_mut().unwrap().root = original_root;
        state.assurance.as_mut().unwrap().commit = "f".repeat(40);
        assert!(state.check_assurance(&fixture.home, &fixture.root).is_err());
        fs::write(
            fixture.root.join("unowned.rs"),
            "pub fn parse(input: &str) {}\n",
        )
        .unwrap();
        let original = request(&directory).unwrap();
        let error = original
            .check_assurance(&fixture.home, &fixture.root)
            .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<crate::refusal::Refusal>()
                .unwrap()
                .rule,
            "assure.added"
        );
        assert!(stop(&fixture.home, &input).is_err());
        observe(
            &fixture.home,
            "session",
            ObservationTool::Git,
            &["--version".to_owned()],
        )
        .unwrap();
    }

    #[test]
    fn changing_any_context_invalidates_observations_and_receipts() {
        let fixture = ContextFixture::new();
        let input = fixture.begin();
        fixture.check();
        observe(
            &fixture.home,
            "session",
            ObservationTool::Git,
            &["--version".to_owned()],
        )
        .unwrap();
        let directory = directory(&fixture.home, "session").unwrap();
        let record = fs::read_dir(directory.join("observations"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let id = record.file_stem().unwrap().to_str().unwrap();
        let original = serde_json::to_value(request(&directory).unwrap()).unwrap();
        let mut mutations = Vec::new();
        let mut changed = original.clone();
        changed["assurance"]["root"] = json!(fixture.scope.path());
        mutations.push(changed);
        let mut changed = original.clone();
        changed["assurance"]["commit"] = json!("f".repeat(40));
        mutations.push(changed);
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove("assurance");
        mutations.push(changed);
        for changed in mutations {
            write_json(&directory.join("request.json"), &changed).unwrap();
            let state = request(&directory).unwrap();
            assert!(check_observation(&directory, &state, id, "git version", 0).is_err());
            let error = stop(&fixture.home, &input).unwrap_err();
            assert!(format!("{error:#}").contains("stale or incomplete"));
        }
        write_json(&directory.join("request.json"), &original).unwrap();
        assert!(stop(&fixture.home, &input).is_ok());
    }

    #[test]
    fn saved_requests_preserve_the_known_assurance_context() {
        let home = tempfile::tempdir().unwrap();
        let directory = directory(home.path(), "session").unwrap();
        let original = json!({
            "version": VERSION,
            "generation": ".tmpSavedRequest",
            "prompt": "Prepare the project without publishing it.",
            "assurance": {
                "root": home.path(),
                "commit": "ee178c449d6507de8045f8f79b92269da9b752fb"
            }
        });
        write_json(&directory.join("request.json"), &original).unwrap();
        let state = request(&directory).unwrap();
        assert_eq!(serde_json::to_value(state).unwrap(), original);
    }

    #[test]
    fn changing_the_prompt_without_renewing_the_generation_invalidates_evidence() {
        let home = tempfile::tempdir().unwrap();
        let draft = home.path().join("draft.txt");
        let evidence = home.path().join("evidence.json");
        fs::write(&draft, "Can we test this?\n").unwrap();
        write_json(&evidence, &document()).unwrap();
        let input = input("request");
        begin(home.path(), &input).unwrap();
        check(home.path(), "session", &draft, &evidence).unwrap();
        let directory = directory(home.path(), "session").unwrap();
        observe(
            home.path(),
            "session",
            ObservationTool::Git,
            &["--version".to_owned()],
        )
        .unwrap();
        let record = fs::read_dir(directory.join("observations"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let id = record.file_stem().unwrap().to_str().unwrap();
        let mut state = request(&directory).unwrap();
        state.prompt = "another request".to_owned();
        write_json(&directory.join("request.json"), &state).unwrap();
        assert!(check_observation(&directory, &state, id, "git version", 0).is_err());
        assert!(stop(home.path(), &input).is_err());
    }

    #[test]
    fn coverage_checks_every_line_and_rejects_hidden_fields() {
        let document = document();
        assert!(coverage("Can we test this?\n", &document, "request").is_ok());
        assert!(
            coverage(
                "Can we test this?\nAn unsupported conclusion.\n",
                &document,
                "request"
            )
            .is_err()
        );
        assert!(coverage("Can we test that?\n", &document, "request").is_err());
        let mut encoded = serde_json::to_value(document).unwrap();
        encoded["claims"][0]["unexpected"] = json!("an unchecked assertion");
        assert!(serde_json::from_value::<Document>(encoded).is_err());
    }

    #[test]
    fn receipts_are_bound_to_the_exact_request_draft_and_producer() {
        let home = tempfile::tempdir().unwrap();
        let draft = home.path().join("draft.txt");
        let evidence = home.path().join("evidence.json");
        fs::write(&draft, "Can we test this?\n").unwrap();
        write_json(&evidence, &document()).unwrap();
        let mut input = input("same request");
        begin(home.path(), &input).unwrap();
        check(home.path(), "session", &draft, &evidence).unwrap();
        assert!(stop(home.path(), &input).is_ok());
        input.last_assistant_message = Some("Can we test that?".to_owned());
        assert!(stop(home.path(), &input).is_err());
        input.last_assistant_message = Some("Can we test this?".to_owned());
        let path = directory(home.path(), "session")
            .unwrap()
            .join("receipt.json");
        let original = read(&path).unwrap();
        let mut receipt: Receipt = serde_json::from_str(&original).unwrap();
        receipt.producer = "another build".to_owned();
        write_json(&path, &receipt).unwrap();
        assert!(stop(home.path(), &input).is_err());
        fs::write(path, original).unwrap();
        begin(home.path(), &input).unwrap();
        assert!(stop(home.path(), &input).is_err());
    }

    #[test]
    fn retained_inputs_and_source_snapshots_are_checked_again_at_stop() {
        let home = tempfile::tempdir().unwrap();
        let draft = home.path().join("draft.txt");
        let evidence = home.path().join("evidence.json");
        fs::write(&draft, "Can we test this?\n").unwrap();
        write_json(&evidence, &document()).unwrap();
        let input = input("request");
        begin(home.path(), &input).unwrap();
        check(home.path(), "session", &draft, &evidence).unwrap();
        let directory = directory(home.path(), "session").unwrap();
        let state = request(&directory).unwrap();
        let artifacts = directory.join(&state.generation);
        for name in ["draft.txt", "evidence.json"] {
            let path = artifacts.join(name);
            let original = fs::read(&path).unwrap();
            fs::write(&path, "changed input").unwrap();
            assert!(stop(home.path(), &input).is_err(), "{name}");
            fs::remove_file(&path).unwrap();
            assert!(stop(home.path(), &input).is_err(), "missing {name}");
            fs::write(path, original).unwrap();
            assert!(stop(home.path(), &input).is_ok());
        }
        let mut receipt: Receipt =
            serde_json::from_str(&read(&directory.join("receipt.json")).unwrap()).unwrap();
        let source: Source = serde_json::from_value(json!({
            "url":"https://clrd.ninjal.ac.jp/unidic/", "authority":"public",
            "title":"Synthetic test record", "publisher":"Test fixture", "locator":"Test",
            "excerpt":"Synthetic entry text"
        }))
        .unwrap();
        let mut document = document();
        document.claims[0].basis = Basis::Source { source };
        let url = "https://clrd.ninjal.ac.jp/unidic/";
        receipt
            .sources
            .insert(url.to_owned(), digest(b"Synthetic entry text"));
        let path = source_path(&artifacts, url);
        assert!(check_retained_sources(&artifacts, &receipt, &document).is_err());
        fs::write(&path, "Synthetic entry text").unwrap();
        assert!(check_retained_sources(&artifacts, &receipt, &document).is_ok());
        fs::write(path, "Changed source entry").unwrap();
        assert!(check_retained_sources(&artifacts, &receipt, &document).is_err());
        receipt.sources.clear();
        assert!(check_retained_sources(&artifacts, &receipt, &document).is_err());
    }

    #[test]
    fn request_generation_cannot_escape_its_state_directory() {
        let home = tempfile::tempdir().unwrap();
        begin(home.path(), &input("request")).unwrap();
        let directory = directory(home.path(), "session").unwrap();
        for generation in [
            "../outside",
            ".tmp/../../outside",
            ".tmp\\outside",
            "",
            "old",
        ] {
            write_json(
                &directory.join("request.json"),
                &Request {
                    version: VERSION,
                    generation: generation.to_owned(),
                    prompt: "request".to_owned(),
                    assurance: None,
                },
            )
            .unwrap();
            assert!(request(&directory).is_err(), "{generation}");
        }
    }

    #[test]
    fn a_missing_prover_cannot_supply_proof_evidence() {
        let home = tempfile::tempdir().unwrap();
        let error = prove(home.path(), "", 1).unwrap_err();
        assert!(format!("{error:#}").contains("answer prover is missing"));
    }

    #[test]
    fn lexical_regressions_cannot_reach_source_or_proof_execution() {
        let corpus = include_str!("../../../fuzz/corpus/answer_plan/lexical");
        let (draft, evidence) = corpus.split_once("\n---\n").unwrap();
        assert!(inspect(draft, evidence, "request").is_ok());
        let original: serde_json::Value = serde_json::from_str(evidence).unwrap();
        for (field, value) in [
            ("kana", "げんていけん"),
            ("kana", ""),
            ("kana", "gentaiken"),
            ("romanized", "genteiken"),
            ("romanized", ""),
        ] {
            let mut changed = original.clone();
            changed["readings"][0][field] = serde_json::json!(value);
            assert!(
                inspect(draft, &changed.to_string(), "request").is_err(),
                "{field}: {value}"
            );
        }
    }

    #[test]
    fn observation_wrappers_refuse_publication_and_interpreters() {
        assert!(ObservationTool::Git.tool(&["--version".to_owned()]).is_ok());
        assert!(ObservationTool::Git.tool(&["push".to_owned()]).is_err());
        assert!(
            ObservationTool::Gh
                .tool(&["issue".to_owned(), "comment".to_owned()])
                .is_err()
        );
        assert!(
            ObservationTool::Cargo
                .tool(&["publish".to_owned()])
                .is_err()
        );
        assert!(ObservationTool::Just.tool(&["release".to_owned()]).is_err());
    }

    #[test]
    #[ignore = "requires an available live publisher dictionary entry over HTTPS"]
    fn a_live_dictionary_entry_attests_the_original_reading() {
        let home = tempfile::tempdir().unwrap();
        let url = "https://kotobank.jp/word/%E5%8E%9F%E4%BD%93%E9%A8%93-492640";
        let source = json!({
            "url":url,"authority":"dictionary","publisher":"Shogakukan",
            "title":"Digital Daijisen: 原体験","locator":"Entry heading and reading",
            "excerpt":"原体験<span>（読み）ゲンタイケン</span>"
        });
        let text = format!(
            "The reading of 原体験 is げんたいけん, and Cabinet2025-kana gives gentaiken ([Digital Daijisen]({url}))."
        );
        let evidence = json!({
            "version":1,"atoms":[],
            "claims":[{"id":"reading","text":text,"formula":null,"basis":{"kind":"source","source":source}}],
            "readings":[{"written":"原体験","kana":"げんたいけん","romanized":"gentaiken",
                "context":"The lexical noun in the user's reported example", "source":source,
                "standard":{"name":"cabinet2025-kana"}}]
        });
        let draft_path = home.path().join("draft.txt");
        let evidence_path = home.path().join("evidence.json");
        fs::write(&draft_path, &text).unwrap();
        write_json(&evidence_path, &evidence).unwrap();
        let mut input = input("Check the reading of 原体験.");
        input.last_assistant_message = Some(text);
        begin(home.path(), &input).unwrap();
        check(home.path(), "session", &draft_path, &evidence_path).unwrap();
        assert!(stop(home.path(), &input).is_ok());
        let mut changed = evidence;
        changed["readings"][0]["romanized"] = json!("genteiken");
        write_json(&evidence_path, &changed).unwrap();
        assert!(check(home.path(), "session", &draft_path, &evidence_path).is_err());
    }

    #[test]
    #[ignore = "requires the explicit pinned image and bounded Docker runtime"]
    fn a_real_kernel_checks_the_generated_claim_and_replays_its_artifact() {
        let image = std::env::var("DOTFILES_ANSWER_TEST_IMAGE")
            .expect("set the immutable pinned Lean image ID");
        let home = tempfile::tempdir().unwrap();
        write_json(
            &home.path().join(PROVER_CONFIG),
            &Prover {
                version: VERSION,
                image,
                lean_version: lean_version().unwrap().to_owned(),
            },
        )
        .unwrap();
        let evidence = json!({
            "version":1,"atoms":["P", "Q"],"readings":[],
            "claims":[
                {"id":"p","text":"Hypothesis: P.","formula":Atom(0),"basis":{"kind":"hypothesis","reason":"Declared premise"}},
                {"id":"pq","text":"Hypothesis: P implies Q.","formula":Implies(Box::new(Atom(0)),Box::new(Atom(1))),"basis":{"kind":"hypothesis","reason":"Declared implication"}},
                {"id":"q","text":"Under these assumptions: Q.","formula":Atom(1),"basis":{"kind":"inference","conditional":true,"argument":{"premises":["p","pq"],"conclusion":Atom(1)}}}
            ]
        });
        let text = "Hypothesis: P.\nHypothesis: P implies Q.\nUnder these assumptions: Q.\n";
        let draft = home.path().join("draft.txt");
        let document = home.path().join("evidence.json");
        fs::write(&draft, text).unwrap();
        write_json(&document, &evidence).unwrap();
        let mut input = input("Check the declared inference.");
        input.last_assistant_message = Some(text.to_owned());
        begin(home.path(), &input).unwrap();
        check(home.path(), "session", &draft, &document).unwrap();
        assert!(stop(home.path(), &input).is_ok());
        let directory = directory(home.path(), "session").unwrap();
        let artifacts = directory.join(request(&directory).unwrap().generation);
        for name in ["Answer.lean", "proof.txt"] {
            let path = artifacts.join(name);
            let original = fs::read(&path).unwrap();
            fs::write(&path, "changed proof artifact").unwrap();
            assert!(stop(home.path(), &input).is_err(), "{name}");
            fs::remove_file(&path).unwrap();
            assert!(stop(home.path(), &input).is_err(), "missing {name}");
            fs::write(path, original).unwrap();
            assert!(stop(home.path(), &input).is_ok());
        }
        let false_claim =
            "theorem argument0 : false = true := by decide\n#print axioms argument0\n";
        let error = format!("{:#}", prove(home.path(), false_claim, 1).unwrap_err());
        assert!(
            error.contains("decide") && error.contains("false"),
            "{error}"
        );
    }
}
