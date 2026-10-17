use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use rules::phase_receipt_rules::{Disposition, Evidence, disposition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;

const CACHE_VARIABLE: &str = "DOTFILES_HOST_CACHE";
const DIRECTORY: &str = "proof-evidence";
const ANCHORS: &str = "verified-kani";
const ANCHOR_LOCK: &str = "anchors.lock";
const SCHEMA: u32 = 1;
const MAX_CLOSURE_ENTRIES: usize = 100_000;
const HASH_BUFFER_BYTES: usize = 64 * 1024;
#[cfg(unix)]
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const WRITE_PERMISSION_BITS: u32 = 0o222;
#[cfg(unix)]
const MUTABLE_OR_SHARED_ANCHOR_BITS: u32 = 0o277;
#[cfg(unix)]
const SEALED_FILE_MODE: u32 = 0o400;
#[cfg(unix)]
const SEALED_DIRECTORY_MODE: u32 = 0o500;
const ENGINE_INPUTS: [&str; 39] = [
    "xtask/src/skill_proofs.rs",
    "xtask/src/phase_evidence.rs",
    "xtask/src/obligations.rs",
    "xtask/src/checks.rs",
    "xtask/src/lib.rs",
    "xtask/tests/container_proofs.rs",
    "xtask/tests/support/incident_fixture.rs",
    "crates/platform/src/lib.rs",
    "crates/platform/src/tool.rs",
    "crates/platform/src/process.rs",
    "crates/platform/src/home.rs",
    "crates/platform/src/paths.rs",
    "crates/platform/src/checks.rs",
    "crates/platform/src/refusal.rs",
    "crates/platform/src/incidents.rs",
    "crates/platform/src/cleanup.rs",
    "crates/platform/src/build_inputs.rs",
    "crates/platform/build.rs",
    "crates/skill-ops/src/lib.rs",
    "crates/rules/src/lib.rs",
    "crates/rules/src/proof_budget_rules.rs",
    "crates/rules/src/obligation_rules.rs",
    "crates/rules/src/phase_receipt_rules.rs",
    "crates/rules/src/incident_rules.rs",
    "Cargo.toml",
    "Cargo.lock",
    "xtask/Cargo.toml",
    "crates/platform/Cargo.toml",
    "crates/model/Cargo.toml",
    "crates/skill-ops/Cargo.toml",
    "crates/rules/Cargo.toml",
    "crates/coderabbit-guard/Cargo.toml",
    "crates/dotfiles/Cargo.toml",
    "crates/dotguard/Cargo.toml",
    "crates/pr-workflow/Cargo.toml",
    "crates/prose/Cargo.toml",
    ".cargo/config.toml",
    "mise.toml",
    "mise.lock",
];
const ENGINE_SOURCE_DOMAIN: &[u8] = b"kani-engine-sources-v2\0";
const ENGINE_DOMAIN: &str = "kani-engine-source-and-compiler-v2";
const LEGACY_CARGO_CONFIG: &str = ".cargo/config";
const KANI_COMPILER_TOOLCHAIN: &str = "rust-toolchain-version";
const COMPILER_BIN: &str = "bin";
const COMPILER_LIB: &str = "lib";
const RUSTUP_AUTO_INSTALL: &str = "RUSTUP_AUTO_INSTALL";
const MODEL_INPUTS: [&str; 9] = [
    "specs",
    "xtask/src",
    "xtask/proofs",
    "xtask/tests/container_proofs.rs",
    "home/dot_agents/skills",
    "home/dot_config/skill-ops/policy.json",
    "policy/obligations.toml",
    "policy/baselines.toml",
    "fuzz",
];

thread_local! {
    static ACTIVE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KaniBatch {
    pub(crate) names: Vec<String>,
    pub(crate) counterexample: bool,
    pub(crate) document: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawResult {
    pub(crate) status: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

impl From<Output> for RawResult {
    fn from(output: Output) -> Self {
        Self {
            status: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }
}

impl RawResult {
    pub(crate) fn success(&self) -> bool {
        self.status == Some(0)
    }

    pub(crate) fn failure(&self) -> bool {
        self.status.is_some_and(|code| code != 0)
    }

    pub(crate) fn stdout(&self) -> Result<&str> {
        Ok(std::str::from_utf8(&self.stdout)?)
    }

    pub(crate) fn stderr(&self) -> Result<&str> {
        Ok(std::str::from_utf8(&self.stderr)?)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct KaniInput {
    source: String,
    source_hash: String,
    probes_hash: String,
    engine_hash: String,
    verifier_hash: String,
    settings_hash: String,
    target: String,
    version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    schema: u32,
    input: KaniInput,
    reports: Vec<KaniBatch>,
}

pub(crate) struct KaniReadback {
    identity: String,
    harness_duration_ms: u64,
}

impl KaniReadback {
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }
    pub(crate) fn harness_duration_ms(&self) -> u64 {
        self.harness_duration_ms
    }
}

#[derive(Debug, Serialize)]
struct PhaseTerminal<'a> {
    schema: u32,
    phase: &'a str,
    elapsed_ms: u128,
    workload: usize,
    completed: bool,
    source_hash: String,
    producer_hash: &'a str,
    commit: Option<String>,
    tree: Option<String>,
    target: String,
}

fn digest(bytes: &[u8]) -> String {
    crate::build_inputs::hex(&Sha256::digest(bytes))
}

fn persist(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn anchors_lock(storage: &Path) -> Result<fs::File> {
    Ok(fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(storage.join(ANCHOR_LOCK))?)
}

fn storage(root: &Path) -> Result<PathBuf> {
    let path = std::env::var_os(CACHE_VARIABLE)
        .map(PathBuf::from)
        .map_or_else(
            || {
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(|home| PathBuf::from(home).join(platform::code_environment::HOST_CACHE))
                    .context("proof evidence owner home")
            },
            Ok,
        )?;
    ensure!(
        path.is_absolute(),
        "proof evidence cache must be an absolute owner path"
    );
    fs::create_dir_all(&path)?;
    let path = crate::canonical(&path)?;
    ensure!(
        !path.starts_with(crate::canonical(root)?),
        "proof evidence must live outside candidate Source"
    );
    let directory = path.join(DIRECTORY);
    fs::create_dir_all(&directory)?;
    ensure!(
        crate::canonical(&directory)? == directory
            && !directory.starts_with(crate::canonical(root)?),
        "proof evidence storage must be an owned directory outside candidate Source"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &directory,
            fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE),
        )?;
    }
    Ok(directory)
}

/// Retains a phase's private diagnostics and reports elapsed time and exact declared workload.
///
/// # Errors
///
/// Preserves failed or incomplete work as an error with a public opaque artifact identity.
pub fn run<T>(
    root: &Path,
    phase: &str,
    workload: usize,
    check: impl FnOnce() -> Result<T>,
) -> Result<T> {
    run_in_storage(root, phase, workload, &storage(root)?, check)
}

fn run_in_storage<T>(
    root: &Path,
    phase: &str,
    workload: usize,
    storage: &Path,
    check: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let started = Instant::now();
    let scope = tempfile::Builder::new()
        .prefix("execution-")
        .tempdir_in(storage)?;
    let path = scope.keep();
    secure_private_directory(&path)?;
    let mut source = Sha256::new();
    hash_path(root, &mut source)?;
    let identity = |suffix: &str| -> Option<String> {
        let output = Tool::Git
            .command()
            .arg("-C")
            .arg(root)
            .args(["rev-parse", suffix])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|text| text.trim().to_owned())
            })
            .flatten()
    };
    let mut terminal = PhaseTerminal {
        schema: SCHEMA,
        phase,
        elapsed_ms: 0,
        workload,
        completed: false,
        source_hash: crate::build_inputs::hex(&source.finalize()),
        producer_hash: platform::BUILD_HASH,
        commit: identity("HEAD"),
        tree: identity("HEAD^{tree}"),
        target: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
    };
    persist(&path.join("started.json"), &serde_json::to_vec(&terminal)?)?;
    let previous = ACTIVE.with(|active| active.replace(Some(path.clone())));
    let result = check();
    ACTIVE.with(|active| active.replace(previous));
    terminal.elapsed_ms = started.elapsed().as_millis();
    terminal.completed = result.is_ok();
    persist(&path.join("terminal.json"), &serde_json::to_vec(&terminal)?)?;
    println!(
        "Proof phase {phase}: {} ms, workload={workload}, completed={}",
        terminal.elapsed_ms, terminal.completed
    );
    match result {
        Ok(value) => {
            let mut artifact = Sha256::new();
            hash_path(&path, &mut artifact)?;
            fs::write(
                path.join("identity.sha256"),
                crate::build_inputs::hex(&artifact.finalize()),
            )?;
            Ok(value)
        }
        Err(error) => {
            fs::write(path.join("failure.txt"), format!("{error:#}"))?;
            let mut artifact = Sha256::new();
            hash_path(&path, &mut artifact)?;
            let artifact = crate::build_inputs::hex(&artifact.finalize());
            fs::write(path.join("identity.sha256"), &artifact)?;
            anyhow::bail!("proof phase {phase} failed; retained private evidence sha256:{artifact}")
        }
    }
}

/// Captures a verifier's output before native scratch or container cleanup.
///
/// # Errors
///
/// Retains startup failure diagnostics and refuses an unavailable child or evidence storage.
pub fn output(command: &mut Command, label: &str) -> Result<Output> {
    let directory = ACTIVE.with(|active| active.borrow().clone());
    let result = match directory {
        Some(directory) => recorded_output(command, label, &directory),
        None => command.output().map_err(anyhow::Error::from),
    };
    match &result {
        Ok(output) => {
            retain(label, "stdout", &output.stdout)?;
            retain(label, "stderr", &output.stderr)?;
            retain(label, "status", output.status.to_string().as_bytes())?;
        }
        Err(error) => {
            retain(label, "startup", error.to_string().as_bytes())?;
        }
    }
    result.with_context(|| format!("start proof operation {label}"))
}

fn recorded_output(command: &mut Command, label: &str, directory: &Path) -> Result<Output> {
    let scope = tempfile::Builder::new()
        .prefix("command-")
        .tempdir_in(directory)?;
    let path = scope.keep();
    let arguments: Vec<_> = command
        .get_args()
        .map(std::ffi::OsStr::as_encoded_bytes)
        .collect();
    let environment: Vec<_> = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.as_encoded_bytes(),
                value.map(std::ffi::OsStr::as_encoded_bytes),
            )
        })
        .collect();
    let configuration = serde_json::to_vec(&(
        command.get_program().as_encoded_bytes(),
        &arguments,
        &environment,
        command.get_current_dir(),
    ))?;
    persist(&path.join("configuration.json"), &configuration)?;
    persist(
        &path.join("started.json"),
        &serde_json::to_vec(&serde_json::json!({
            "schema":SCHEMA,"label":label,"configuration_hash":digest(&configuration),
            "completed":false,"stdout":"stdout.log","stderr":"stderr.log"
        }))?,
    )?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(fs::File::create(path.join("stdout.log"))?))
        .stderr(Stdio::from(fs::File::create(path.join("stderr.log"))?));
    let status = command.spawn()?.wait()?;
    persist(
        &path.join("terminal.json"),
        &serde_json::to_vec(&serde_json::json!({
            "schema":SCHEMA,"label":label,"configuration_hash":digest(&configuration),
            "completed":true,"status":status.code(),"termination":status.to_string()
        }))?,
    )?;
    Ok(Output {
        status,
        stdout: fs::read(path.join("stdout.log"))?,
        stderr: fs::read(path.join("stderr.log"))?,
    })
}

fn secure_private_directory(directory: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let owner = Tool::Whoami
            .command()
            .args(["/user", "/fo", "csv", "/nh"])
            .output()?;
        ensure!(
            owner.status.success(),
            "resolve the proof evidence owner SID"
        );
        let text = String::from_utf8(owner.stdout)?;
        let sid = text
            .split([',', '"', ' ', '\r', '\n'])
            .find(|field| {
                field.strip_prefix("S-1-").is_some_and(|suffix| {
                    !suffix.is_empty()
                        && suffix
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || byte == b'-')
                })
            })
            .context("proof evidence owner SID")?;
        let acl = Tool::Icacls
            .command()
            .arg(directory)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!("*{sid}:(OI)(CI)F"))
            .arg("/Q")
            .output()?;
        ensure!(
            acl.status.success(),
            "protect the new private proof evidence directory"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            directory,
            fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE),
        )?;
    }
    Ok(())
}

pub(crate) fn retain(label: &str, kind: &str, bytes: &[u8]) -> Result<()> {
    ACTIVE.with(|active| -> Result<()> {
        if let Some(directory) = active.borrow().as_ref() {
            let identity = digest(&serde_json::to_vec(&(label, kind, bytes))?);
            fs::write(directory.join(format!("{identity}.{kind}")), bytes)?;
            let record = serde_json::to_vec(&serde_json::json!({"label":label,"kind":kind,"sha256":digest(bytes),"bytes":bytes.len(),"file":format!("{identity}.{kind}")}))?;
            let mut manifest = fs::OpenOptions::new().append(true).create(true).open(directory.join("artifacts.jsonl"))?;
            manifest.write_all(&record)?;
            manifest.write_all(b"\n")?;
            manifest.sync_all()?;
        }
        Ok(())
    })
}

pub(crate) fn private_failure(label: &str, error: anyhow::Error) -> anyhow::Error {
    if !ACTIVE.with(|active| active.borrow().is_some()) {
        return error;
    }
    let bytes = format!("{error:#}").into_bytes();
    match retain(label, "failure", &bytes) {
        Ok(()) => anyhow::anyhow!("private proof diagnostic sha256:{}", digest(&bytes)),
        Err(_) => {
            anyhow::anyhow!("private proof diagnostic could not be retained; phase incomplete")
        }
    }
}

pub(crate) fn public_label(label: &str) -> std::borrow::Cow<'_, str> {
    if ACTIVE.with(|active| active.borrow().is_some()) {
        std::borrow::Cow::Owned(format!(
            "proof operation sha256:{}",
            digest(label.as_bytes())
        ))
    } else {
        std::borrow::Cow::Borrowed(label)
    }
}

pub(crate) fn engine_digest(root: &Path) -> Result<String> {
    let driver = crate::skill_proofs::kani_environment_driver()?;
    let compiler = compiler_distribution(&driver, |toolchain| {
        output(
            Tool::Rustup
                .command()
                .current_dir(root)
                .env(RUSTUP_AUTO_INSTALL, "0")
                .args(["which", "--toolchain", toolchain, "rustc"]),
            "existing Kani compiler distribution",
        )
        .map(RawResult::from)
    })?;
    engine_digest_with_compiler(root, &compiler)
}

fn engine_digest_with_compiler(root: &Path, compiler: &Path) -> Result<String> {
    Ok(digest(&serde_json::to_vec(&(
        ENGINE_DOMAIN,
        engine_source_digest(root)?,
        verifier_digest(compiler)?,
    ))?))
}

fn engine_source_digest(root: &Path) -> Result<String> {
    let mut engine = Sha256::new();
    engine.update(ENGINE_SOURCE_DOMAIN);
    for relative in ENGINE_INPUTS {
        engine.update(relative.as_bytes());
        engine.update([0]);
        hash_path(&root.join(relative), &mut engine)?;
    }
    engine.update(LEGACY_CARGO_CONFIG.as_bytes());
    match fs::symlink_metadata(root.join(LEGACY_CARGO_CONFIG)) {
        Ok(_) => hash_path(&root.join(LEGACY_CARGO_CONFIG), &mut engine)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            engine.update(b"\0absent\0");
        }
        Err(error) => return Err(error.into()),
    }
    Ok(crate::build_inputs::hex(&engine.finalize()))
}

fn compiler_distribution(
    driver: &Path,
    lookup: impl FnOnce(&str) -> Result<RawResult>,
) -> Result<PathBuf> {
    let directory = driver
        .parent()
        .and_then(Path::parent)
        .context("Kani distribution directory")?;
    let configuration = fs::read_to_string(directory.join(KANI_COMPILER_TOOLCHAIN))?;
    let toolchain = configuration.trim();
    ensure!(
        !toolchain.is_empty()
            && !toolchain.starts_with('-')
            && !toolchain
                .chars()
                .any(|character| character.is_whitespace() || character.is_control()),
        "the Kani distribution must name one installed Rustup toolchain"
    );
    let result = lookup(toolchain)?;
    ensure!(
        result.success(),
        "cannot resolve the installed Kani compiler: status={:?}; {}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = result.stdout()?;
    ensure!(
        stdout.lines().count() == 1,
        "Rustup must name exactly one installed compiler"
    );
    let compiler = Path::new(stdout.trim());
    ensure!(
        compiler.is_absolute() && compiler.is_file(),
        "Rustup must name an existing absolute compiler path"
    );
    let binary = format!("rustc{}", std::env::consts::EXE_SUFFIX);
    let bin = compiler.parent().context("Rustup compiler bin directory")?;
    ensure!(
        compiler.file_name() == Some(std::ffi::OsStr::new(&binary))
            && bin.file_name() == Some(std::ffi::OsStr::new(COMPILER_BIN)),
        "Rustup must locate the compiler inside its distribution bin directory"
    );
    let distribution = crate::canonical(bin.parent().context("Rustup compiler distribution")?)?;
    ensure!(
        distribution.join(COMPILER_LIB).is_dir(),
        "the Rustup compiler distribution is missing its libraries"
    );
    Ok(distribution)
}

pub(crate) fn kani_input(
    root: &Path,
    source: &str,
    probes: &str,
    flags: &BTreeSet<&'static str>,
    verifier_hash: &str,
    engine_hash: &str,
) -> Result<KaniInput> {
    let settings = serde_json::to_vec(&(
        flags,
        std::env::var_os("RUSTFLAGS"),
        std::env::var_os("CARGO_ENCODED_RUSTFLAGS"),
    ))?;
    Ok(KaniInput {
        source: source.into(),
        source_hash: digest(&fs::read(root.join(source))?),
        probes_hash: digest(probes.as_bytes()),
        engine_hash: engine_hash.into(),
        verifier_hash: verifier_hash.into(),
        settings_hash: digest(&settings),
        target: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        version: crate::skill_proofs::KANI_VERSION.into(),
    })
}

pub(crate) fn verifier_digest(directory: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash_path(directory, &mut hash)?;
    Ok(crate::build_inputs::hex(&hash.finalize()))
}

pub(crate) fn model_inputs(root: &Path) -> Result<String> {
    let mut inputs = Vec::new();
    for relative in MODEL_INPUTS {
        inputs.push((relative.to_owned(), verifier_digest(&root.join(relative))?));
    }
    inputs.push((
        "runtime Source".into(),
        platform::build_inputs::content_hash(root)?,
    ));
    Ok(digest(&serde_json::to_vec(&inputs)?))
}

fn hash_path(path: &Path, hash: &mut Sha256) -> Result<()> {
    let mut pending = vec![path.to_owned()];
    let mut visited = BTreeSet::new();
    let mut entries_seen = 0_usize;
    while let Some(path) = pending.pop() {
        entries_seen += 1;
        ensure!(
            entries_seen <= MAX_CLOSURE_ENTRIES,
            "proof closure exceeds its bounded input inventory"
        );
        let metadata = fs::symlink_metadata(&path)?;
        hash.update(
            path.file_name()
                .context("proof input name")?
                .as_encoded_bytes(),
        );
        hash.update([0]);
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)?;
            let bytes = target.as_os_str().as_encoded_bytes();
            hash.update(b"link\0");
            hash.update(bytes.len().to_le_bytes());
            hash.update(bytes);
        }
        let metadata = fs::metadata(&path)?;
        if metadata.is_file() {
            hash.update(b"file\0");
            hash.update(metadata.len().to_le_bytes());
            let mut file = fs::File::open(&path)?;
            let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hash.update(
                    buffer
                        .get(..read)
                        .context("proof file read exceeded its buffer")?,
                );
            }
        } else {
            hash.update(b"directory\0");
            ensure!(
                metadata.is_dir(),
                "proof closure cannot include a special filesystem node"
            );
            if !visited.insert(crate::canonical(&path)?) {
                hash.update(b"visited-directory\0");
                continue;
            }
            let mut entries = fs::read_dir(path)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            entries.sort();
            for entry in entries.into_iter().rev() {
                let fuzz_output = entry.file_name().is_some_and(|name| name == "artifacts")
                    && entry
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|name| matches!(name.to_str(), Some("fuzz" | "fuzz-fixture")));
                if !fuzz_output
                    && !entry.file_name().is_some_and(|name| {
                        matches!(name.to_str(), Some("target" | ".git" | "node_modules"))
                    })
                {
                    pending.push(entry);
                }
            }
        }
    }
    Ok(())
}

fn local_reuse(root: &Path) -> bool {
    if !cfg!(unix)
        || std::env::var_os("CI").is_some()
        || std::env::var_os(crate::skill_proofs::INSIDE_CONTAINER).is_some()
    {
        return false;
    }
    if fs::read(root.join("xtask/src/skill_proofs.rs"))
        .is_ok_and(|bytes| bytes == include_bytes!("skill_proofs.rs"))
        && fs::read(root.join("xtask/src/phase_evidence.rs"))
            .is_ok_and(|bytes| bytes == include_bytes!("phase_evidence.rs"))
        && fs::read(root.join("xtask/src/obligations.rs"))
            .is_ok_and(|bytes| bytes == include_bytes!("obligations.rs"))
        && fs::read(root.join("xtask/src/checks.rs"))
            .is_ok_and(|bytes| bytes == include_bytes!("checks.rs"))
        && fs::read(root.join("xtask/src/lib.rs"))
            .is_ok_and(|bytes| bytes == include_bytes!("lib.rs"))
        && platform::build_inputs::content_hash(root).is_ok_and(|hash| hash == platform::BUILD_HASH)
    {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            return fs::metadata(root)
                .is_ok_and(|metadata| metadata.permissions().mode() & WRITE_PERMISSION_BITS == 0)
                && fs::OpenOptions::new()
                    .write(true)
                    .open(root.join("Cargo.toml"))
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::PermissionDenied);
        }
    }
    false
}

fn validate_batches(
    reports: &[KaniBatch],
    production: &[String],
    rejected: &[String],
) -> Result<()> {
    ensure!(
        !production.is_empty() && !rejected.is_empty(),
        "cached Kani production or rejecting inventory is empty"
    );
    for (expected, counterexample) in [(production, false), (rejected, true)] {
        let batches: Vec<_> = reports
            .iter()
            .filter(|report| report.counterexample == counterexample)
            .collect();
        let listed: Vec<_> = batches
            .iter()
            .flat_map(|batch| batch.names.iter().cloned())
            .collect();
        ensure!(
            listed == expected,
            "cached Kani inventory is missing, duplicated, reordered, or unexpected"
        );
        for batch in batches {
            let names: Vec<_> = batch.names.iter().map(String::as_str).collect();
            crate::skill_proofs::validate_results(&batch.document, &names, counterexample)?;
        }
    }
    Ok(())
}

pub(crate) fn cached_kani(
    root: &Path,
    input: &KaniInput,
    production: &[String],
    rejected: &[String],
) -> Result<Option<KaniReadback>> {
    let producer = local_reuse(root);
    if !producer {
        return Ok(None);
    }
    let storage = storage(root)?;
    let lock = anchors_lock(&storage)?;
    lock.lock_shared()?;
    let directory = storage.join(ANCHORS);
    let identity = digest(&serde_json::to_vec(input)?);
    let path = directory.join(format!("{identity}.json"));
    let bytes = if trusted_anchor(&directory, &path)? {
        Some(fs::read(&path)?)
    } else {
        None
    };
    let anchor = bytes
        .as_ref()
        .and_then(|bytes| serde_json::from_slice::<Anchor>(bytes).ok());
    let evidence = match &anchor {
        Some(anchor) if anchor.schema != SCHEMA || anchor.input != *input => Evidence::Incompatible,
        Some(anchor) if validate_batches(&anchor.reports, production, rejected).is_ok() => {
            Evidence::Verified
        }
        Some(_) => Evidence::Incomplete,
        None if bytes.is_some() => Evidence::Incomplete,
        None if fs::symlink_metadata(&path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            Evidence::Absent
        }
        None => Evidence::Untrusted,
    };
    match disposition(
        producer,
        evidence,
        directory.join(format!("{identity}.pending")).exists(),
    ) {
        Disposition::Measure => Ok(None),
        Disposition::Refuse => anyhow::bail!(
            "existing Kani origin is {evidence:?}; owner inspection is required before measurement"
        ),
        Disposition::Reuse => {
            let bytes = bytes.context("verified Kani anchor bytes")?;
            let anchor = anchor.context("verified Kani anchor document")?;
            let mut harness_duration_ms = 0_u64;
            for batch in &anchor.reports {
                for report in batch.document["verification_results"]["results"]
                    .as_array()
                    .context("verified Kani result inventory")?
                {
                    harness_duration_ms = harness_duration_ms
                        .checked_add(
                            report["duration_ms"]
                                .as_u64()
                                .context("verified Kani duration")?,
                        )
                        .context("Kani origin duration overflow")?;
                }
            }
            retain("Kani reused origin", "anchor", &bytes)?;
            Ok(Some(KaniReadback {
                identity: digest(&bytes),
                harness_duration_ms,
            }))
        }
    }
}

#[cfg(unix)]
fn trusted_anchor(directory: &Path, path: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let owner = rustix::process::geteuid().as_raw();
    for (candidate, file) in [(directory, false), (path, true)] {
        let metadata = match fs::symlink_metadata(candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if metadata.uid() != owner
            || metadata.mode() & MUTABLE_OR_SHARED_ANCHOR_BITS != 0
            || metadata.file_type().is_symlink()
            || (if file {
                !metadata.is_file()
            } else {
                !metadata.is_dir()
            })
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(not(unix))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "non-Unix refuses reuse through the same fallible interface as Unix ownership checks"
)]
fn trusted_anchor(_directory: &Path, _path: &Path) -> Result<bool> {
    Ok(false)
}

pub(crate) fn publish_kani(
    root: &Path,
    input: KaniInput,
    reports: Vec<KaniBatch>,
    production: &[String],
    rejected: &[String],
) -> Result<()> {
    validate_batches(&reports, production, rejected)?;
    if !local_reuse(root) {
        return Ok(());
    }
    let storage = storage(root)?;
    let lock = anchors_lock(&storage)?;
    lock.lock()?;
    let directory = storage.join(ANCHORS);
    fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &directory,
            fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE),
        )?;
    }
    let identity = digest(&serde_json::to_vec(&input)?);
    let path = directory.join(format!("{identity}.json"));
    let published = (|| -> Result<()> {
        if path.exists() {
            let prior: Anchor = serde_json::from_slice(&fs::read(&path)?)?;
            ensure!(
                prior.schema == SCHEMA && prior.input == input,
                "existing immutable Kani anchor has another identity"
            );
            validate_batches(&prior.reports, production, rejected)?;
            let pending = directory.join(format!("{identity}.pending"));
            if pending.exists() {
                fs::remove_file(pending)?;
            }
            return Ok(());
        }
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        serde_json::to_writer(
            file.as_file_mut(),
            &Anchor {
                schema: SCHEMA,
                input,
                reports,
            },
        )?;
        file.as_file().sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(file.path(), fs::Permissions::from_mode(SEALED_FILE_MODE))?;
        }
        file.persist_noclobber(&path)?;
        let pending = directory.join(format!("{identity}.pending"));
        if pending.exists() {
            fs::remove_file(pending)?;
        }
        Ok(())
    })();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &directory,
            fs::Permissions::from_mode(SEALED_DIRECTORY_MODE),
        )?;
    }
    published
}

pub(crate) fn begin_kani(root: &Path, input: &KaniInput) -> Result<()> {
    if !local_reuse(root) {
        return Ok(());
    }
    let storage = storage(root)?;
    let lock = anchors_lock(&storage)?;
    lock.lock()?;
    let directory = storage.join(ANCHORS);
    fs::create_dir_all(&directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &directory,
            fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE),
        )?;
    }
    let identity = digest(&serde_json::to_vec(input)?);
    let pending = directory.join(format!("{identity}.pending"));
    let written = (|| -> Result<()> {
        if !pending.exists() {
            let file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pending)?;
            serde_json::to_writer(&file, input)?;
            file.sync_all()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&pending, fs::Permissions::from_mode(SEALED_FILE_MODE))?;
            }
        }
        Ok(())
    })();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &directory,
            fs::Permissions::from_mode(SEALED_DIRECTORY_MODE),
        )?;
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_fixture() -> Result<tempfile::TempDir> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("workspace root")?;
        let scope = tempfile::tempdir()?;
        for relative in ENGINE_INPUTS {
            let source = root.join(relative);
            let target = scope.path().join(relative);
            if source.is_dir() {
                fs::create_dir_all(target)?;
            } else {
                fs::create_dir_all(target.parent().context("fixture input parent")?)?;
                fs::write(target, fs::read(source)?)?;
            }
        }
        for relative in [
            "xtask/tests/container_proofs.rs",
            "xtask/src/skill_proofs.rs",
            "crates/platform/src/issue_context.rs",
            ".cargo/config.toml",
        ] {
            let target = scope.path().join(relative);
            fs::create_dir_all(target.parent().context("fixture adapter parent")?)?;
            fs::write(target, fs::read(root.join(relative))?)?;
        }
        Ok(scope)
    }

    #[test]
    fn kani_engine_identity_excludes_unrelated_issue_context_changes() -> Result<()> {
        const RUNTIME: &str = "crates/platform/src/issue_context.rs";
        let scope = engine_fixture()?;
        let original = engine_source_digest(scope.path())?;
        let path = scope.path().join(RUNTIME);
        let mut bytes = fs::read(&path)?;
        bytes.extend_from_slice(b"\n// changed runtime input\n");
        fs::write(path, bytes)?;
        assert_eq!(engine_source_digest(scope.path())?, original);
        Ok(())
    }

    #[test]
    fn kani_engine_identity_binds_the_driver_decoder_budget_and_compiler_configuration()
    -> Result<()> {
        for relative in [
            "xtask/tests/container_proofs.rs",
            "xtask/src/skill_proofs.rs",
            "crates/rules/src/proof_budget_rules.rs",
            "Cargo.lock",
            "mise.toml",
            ".cargo/config.toml",
        ] {
            let scope = engine_fixture()?;
            let original = engine_source_digest(scope.path())?;
            let path = scope.path().join(relative);
            let mut bytes = fs::read(&path)?;
            bytes.extend_from_slice(b"\nchanged verification input\n");
            fs::write(path, bytes)?;
            assert_ne!(engine_source_digest(scope.path())?, original, "{relative}");
        }
        Ok(())
    }

    fn compiler_fixture() -> Result<tempfile::TempDir> {
        let scope = tempfile::tempdir()?;
        fs::create_dir_all(scope.path().join(COMPILER_BIN))?;
        fs::create_dir_all(scope.path().join(COMPILER_LIB))?;
        fs::write(
            scope
                .path()
                .join(COMPILER_BIN)
                .join(format!("rustc{}", std::env::consts::EXE_SUFFIX)),
            b"owned compiler byte fixture",
        )?;
        fs::write(
            scope.path().join(COMPILER_LIB).join("standard-library"),
            b"owned standard library byte fixture",
        )?;
        Ok(scope)
    }

    #[test]
    fn kani_engine_identity_binds_the_actual_compiler_and_standard_library_bytes() -> Result<()> {
        let source = engine_fixture()?;
        let compiler = compiler_fixture()?;
        let original = engine_digest_with_compiler(source.path(), compiler.path())?;
        for relative in [
            PathBuf::from(COMPILER_BIN).join(format!("rustc{}", std::env::consts::EXE_SUFFIX)),
            PathBuf::from(COMPILER_LIB).join("standard-library"),
        ] {
            let path = compiler.path().join(relative);
            let bytes = fs::read(&path)?;
            fs::write(&path, b"changed compiler distribution bytes")?;
            assert_ne!(
                engine_digest_with_compiler(source.path(), compiler.path())?,
                original
            );
            fs::write(path, bytes)?;
            assert_eq!(
                engine_digest_with_compiler(source.path(), compiler.path())?,
                original
            );
        }
        let runtime = source.path().join("crates/platform/src/issue_context.rs");
        fs::write(runtime, b"changed unrelated runtime bytes")?;
        assert_eq!(
            engine_digest_with_compiler(source.path(), compiler.path())?,
            original
        );
        Ok(())
    }

    #[test]
    fn kani_compiler_lookup_uses_the_distributions_exact_toolchain_and_refuses_bad_metadata()
    -> Result<()> {
        const TOOLCHAIN: &str = "nightly-2026-08-21";
        let kani = tempfile::tempdir()?;
        let driver = kani.path().join(COMPILER_BIN).join("kani-driver");
        fs::create_dir_all(driver.parent().context("driver fixture bin")?)?;
        let configuration = kani.path().join(KANI_COMPILER_TOOLCHAIN);
        fs::write(&configuration, format!("{TOOLCHAIN}\n"))?;
        let compiler = compiler_fixture()?;
        let binary = compiler
            .path()
            .join(COMPILER_BIN)
            .join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
        let metadata = RawResult {
            status: Some(0),
            stdout: format!("{}\n", binary.display()).into_bytes(),
            stderr: Vec::new(),
        };
        assert_eq!(
            compiler_distribution(&driver, |selected| {
                assert_eq!(selected, TOOLCHAIN);
                Ok(metadata.clone())
            })?,
            crate::canonical(compiler.path())?
        );
        for status in [None, Some(1)] {
            let mut failed = metadata.clone();
            failed.status = status;
            failed.stderr = b"the required toolchain is not installed".to_vec();
            let error = compiler_distribution(&driver, |_| Ok(failed))
                .err()
                .context("failed Rustup lookup must refuse")?;
            assert!(format!("{error:#}").contains("the required toolchain is not installed"));
        }
        for stdout in [
            Vec::new(),
            b"relative/bin/rustc\n".to_vec(),
            format!("{}\n{}\n", binary.display(), binary.display()).into_bytes(),
            vec![u8::MAX],
        ] {
            let mut malformed = metadata.clone();
            malformed.stdout = stdout;
            assert!(compiler_distribution(&driver, |_| Ok(malformed)).is_err());
        }
        assert!(
            compiler_distribution(&driver, |_| anyhow::bail!("Rustup process did not start"))
                .is_err()
        );
        fs::remove_file(binary)?;
        assert!(compiler_distribution(&driver, |_| Ok(metadata)).is_err());
        Ok(())
    }

    #[test]
    fn kani_compiler_lookup_refuses_a_missing_or_malformed_toolchain_before_the_child() -> Result<()>
    {
        let kani = tempfile::tempdir()?;
        let driver = kani.path().join(COMPILER_BIN).join("kani-driver");
        let called = std::cell::Cell::new(false);
        let lookup = |_: &str| -> Result<RawResult> {
            called.set(true);
            anyhow::bail!("unexpected lookup")
        };
        assert!(compiler_distribution(&driver, lookup).is_err());
        assert!(!called.get());
        for malformed in [
            "",
            " \n",
            "-help",
            "nightly extra",
            "nightly\nother",
            "nightly\0x",
        ] {
            fs::write(kani.path().join(KANI_COMPILER_TOOLCHAIN), malformed)?;
            assert!(
                compiler_distribution(&driver, |_| {
                    called.set(true);
                    anyhow::bail!("unexpected lookup")
                })
                .is_err()
            );
            assert!(!called.get());
        }
        Ok(())
    }

    #[test]
    fn kani_engine_identity_binds_legacy_cargo_configuration_when_present() -> Result<()> {
        let scope = engine_fixture()?;
        let absent = engine_source_digest(scope.path())?;
        fs::write(scope.path().join(LEGACY_CARGO_CONFIG), b"[build]\njobs=2\n")?;
        assert_ne!(engine_source_digest(scope.path())?, absent);
        Ok(())
    }

    #[test]
    fn kani_subject_identity_binds_source_probes_flags_and_verifier_separately() -> Result<()> {
        const SUBJECT: &str = "subject.rs";
        let scope = tempfile::tempdir()?;
        let path = scope.path().join(SUBJECT);
        fs::write(&path, b"actual subject bytes")?;
        let flags = BTreeSet::from(["stubbing"]);
        let input = kani_input(scope.path(), SUBJECT, "probe", &flags, "verifier", "engine")?;
        fs::write(&path, b"changed subject bytes")?;
        assert_ne!(
            kani_input(scope.path(), SUBJECT, "probe", &flags, "verifier", "engine")?,
            input
        );
        fs::write(&path, b"actual subject bytes")?;
        for (probes, options, verifier) in [
            ("changed probe", flags.clone(), "verifier"),
            (
                "probe",
                BTreeSet::from(["stubbing", "function-contracts"]),
                "verifier",
            ),
            ("probe", flags, "changed verifier"),
        ] {
            assert_ne!(
                kani_input(scope.path(), SUBJECT, probes, &options, verifier, "engine")?,
                input
            );
        }
        Ok(())
    }

    fn valid() -> KaniBatch {
        KaniBatch {
            names: vec!["valid".into()],
            counterexample: false,
            document: serde_json::json!({"verification_results":{"summary":{"status":"completed","executed":1,"successful":1,"failed":0},"results":[{"harness_id":"valid","duration_ms":1,"status":"Success","checks":[{"category":"assertion","status":"Success"},{"category":"cover","status":"Satisfied"},{"category":"cover","status":"Satisfied"}]}]}}),
        }
    }

    fn rejected() -> KaniBatch {
        KaniBatch {
            names: vec!["reject".into()],
            counterexample: true,
            document: serde_json::json!({"verification_results":{"summary":{"status":"completed","executed":1,"successful":0,"failed":1},"results":[{"harness_id":"reject","duration_ms":1,"status":"Failure","checks":[{"category":"assertion","status":"Failure"}]}]}}),
        }
    }

    #[test]
    fn cached_receipts_revalidate_every_property_and_complete_inventory() -> Result<()> {
        let expected = ["valid".into()];
        let probes = ["reject".into()];
        validate_batches(&[valid(), rejected()], &expected, &probes)?;
        assert!(validate_batches(&[valid()], &expected, &[]).is_err());
        assert!(validate_batches(&[], &expected, &probes).is_err());
        assert!(validate_batches(&[valid(), valid(), rejected()], &expected, &probes).is_err());
        for (path, replacement) in [
            (
                "/verification_results/results/0/duration_ms",
                serde_json::json!(30001),
            ),
            (
                "/verification_results/results/0/checks/0/status",
                serde_json::json!("Failure"),
            ),
            (
                "/verification_results/results/0/checks",
                serde_json::json!([]),
            ),
            (
                "/verification_results/summary/status",
                serde_json::json!("interrupted"),
            ),
        ] {
            let mut broken = valid();
            *broken
                .document
                .pointer_mut(path)
                .context("receipt fault path")? = replacement;
            assert!(validate_batches(&[broken, rejected()], &expected, &probes).is_err());
        }
        assert!(validate_batches(&[valid()], &expected, &probes).is_err());
        let mut missing_assertion = rejected();
        missing_assertion.document["verification_results"]["results"][0]["checks"] =
            serde_json::json!([]);
        assert!(validate_batches(&[valid(), missing_assertion], &expected, &probes).is_err());
        Ok(())
    }

    #[test]
    fn closure_hash_binds_directory_names_and_source_bodies() -> Result<()> {
        let scope = tempfile::tempdir()?;
        fs::create_dir(scope.path().join("a"))?;
        fs::write(scope.path().join("a/input.rs"), "first")?;
        let first = verifier_digest(scope.path())?;
        fs::write(scope.path().join("a/input.rs"), "second")?;
        assert_ne!(verifier_digest(scope.path())?, first);
        let second = verifier_digest(scope.path())?;
        fs::rename(scope.path().join("a"), scope.path().join("b"))?;
        assert_ne!(verifier_digest(scope.path())?, second);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn linked_verifier_closure_terminates_cycles_and_binds_target_bytes() -> Result<()> {
        let scope = tempfile::tempdir()?;
        fs::write(scope.path().join("input.rs"), "first")?;
        std::os::unix::fs::symlink(scope.path(), scope.path().join("cycle"))?;
        let first = verifier_digest(scope.path())?;
        fs::write(scope.path().join("input.rs"), "second")?;
        assert_ne!(verifier_digest(scope.path())?, first);
        Ok(())
    }

    #[test]
    fn an_unprotected_arbitrary_anchor_is_not_a_trusted_producer() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let path = scope.path().join("forged.json");
        fs::write(&path, serde_json::to_vec(&valid())?)?;
        assert!(!trusted_anchor(scope.path(), &path)?);
        Ok(())
    }

    #[test]
    fn anchor_readers_exclude_permission_changes_and_allow_other_readers() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let first = anchors_lock(scope.path())?;
        let second = anchors_lock(scope.path())?;
        let writer = anchors_lock(scope.path())?;
        first.lock_shared()?;
        second.try_lock_shared()?;
        assert!(matches!(
            writer.try_lock(),
            Err(fs::TryLockError::WouldBlock)
        ));
        first.unlock()?;
        second.unlock()?;
        writer.try_lock()?;
        assert!(matches!(
            first.try_lock_shared(),
            Err(fs::TryLockError::WouldBlock)
        ));
        writer.unlock()?;
        first.try_lock_shared()?;
        Ok(())
    }

    #[test]
    fn failed_phase_keeps_raw_diagnostics_without_returning_their_private_content() -> Result<()> {
        let root = tempfile::tempdir()?;
        let storage = tempfile::tempdir()?;
        let secret = "proof-private-credential-marker";
        let error = run_in_storage::<()>(root.path(), "privacy fixture", 1, storage.path(), || {
            let result = output(
                Tool::Rustc.command().arg(format!("--{secret}")),
                "invalid rustc argument",
            )?;
            ensure!(
                !result.status.success(),
                "rustc admitted an unknown fixture argument"
            );
            ensure!(
                String::from_utf8_lossy(&result.stderr).contains(secret),
                "fixture failed for another cause"
            );
            anyhow::bail!("private failure {secret} at {}", root.path().display())
        })
        .err()
        .context("the phase must report failure")?;
        let public = format!("{error:#}");
        assert!(!public.contains(secret));
        assert!(!public.contains(&root.path().display().to_string()));
        assert!(public.contains("retained private evidence sha256:"));
        let path = fs::read_dir(storage.path())?
            .next()
            .context("retained failure")??
            .path();
        let terminal: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("terminal.json"))?)?;
        assert_eq!(terminal["completed"], false);
        assert_eq!(terminal["workload"], 1);
        assert!(fs::read_to_string(path.join("failure.txt"))?.contains(secret));
        assert!(
            fs::read_to_string(path.join("artifacts.jsonl"))?.contains("invalid rustc argument")
        );
        assert!(path.join("identity.sha256").is_file());
        Ok(())
    }

    #[test]
    fn successful_phase_keeps_its_bound_terminal_and_artifact_manifest() -> Result<()> {
        let root = tempfile::tempdir()?;
        let storage = tempfile::tempdir()?;
        run_in_storage(root.path(), "success fixture", 1, storage.path(), || {
            let result = output(Tool::Rustc.command().arg("--version"), "rustc version")?;
            ensure!(result.status.success(), "rustc version failed");
            Ok(())
        })?;
        let path = fs::read_dir(storage.path())?
            .next()
            .context("retained success")??
            .path();
        let terminal: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("terminal.json"))?)?;
        assert_eq!(terminal["completed"], true);
        assert_eq!(terminal["workload"], 1);
        assert!(
            terminal["source_hash"]
                .as_str()
                .is_some_and(|hash| !hash.is_empty())
        );
        assert!(fs::read_to_string(path.join("artifacts.jsonl"))?.contains("rustc version"));
        assert!(path.join("identity.sha256").is_file());
        Ok(())
    }

    #[test]
    fn independent_check_failures_are_private_before_the_aggregate_prints_them() -> Result<()> {
        let scope = tempfile::tempdir()?;
        let previous = ACTIVE.with(|active| active.replace(Some(scope.path().to_owned())));
        let error = private_failure("inner fixture", anyhow::anyhow!("private-marker"));
        ACTIVE.with(|active| active.replace(previous));
        assert!(!format!("{error:#}").contains("private-marker"));
        assert!(
            fs::read_to_string(scope.path().join("artifacts.jsonl"))?.contains("inner fixture")
        );
        Ok(())
    }
}
