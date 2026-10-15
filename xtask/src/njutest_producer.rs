use super::{
    Assessment, Document, Origin, PACKAGE, TOOL_VERSION, Temperature, UPSTREAM_REVISION,
    closed_document, digest, relative, rows, source_rows, text,
};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

const JOBS: &str = "2";
const TEST_THREADS: &str = "1";
const TRACE_DIRECTORY: &str = "trace";
#[cfg(unix)]
const MAX_ENTRIES: usize = 100_000;
const ORIGINAL_ENGINE_VERSION: &str = "27.1.0";
const CONFIGURATION: &str = ".njutest.toml";
const REPORT_DIRECTORY: &str = "target/njutest-assessment";
const FIXTURE: &str = "xtask/proofs/mutants-fixture";
const FIXTURE_PACKAGE: &str = "mutants-fixture";
const MAX_REPORT_EXIT_CODE: i32 = 2;
#[cfg(unix)]
const WRITE_PERMISSION_BITS: u32 = 0o222;

pub(super) fn prepare(root: &Path) -> Result<()> {
    ensure!(
        !cfg!(windows),
        "native njutest preparation requires Mac or Linux"
    );
    super::generate(root, true)?;
    let platform::tool::Provision::PinnedEnvironment { environment, key } =
        Tool::Njutest.provision()
    else {
        anyhow::bail!("njutest has no named environment")
    };
    let trusted = output(
        Tool::Mise
            .command()
            .current_dir(root)
            .args(["trust", "--quiet"])
            .arg(root.join(format!("mise.{environment}.toml"))),
        "njutest environment trust",
    )?;
    ensure!(
        trusted.status.success(),
        "cannot trust the generated managed environment"
    );
    let installed = output(
        Tool::Cargo
            .command()
            .current_dir(root)
            .env("MISE_ENV", environment)
            .env("MISE_AUTO_PRUNE", "false")
            .env("CARGO_BUILD_JOBS", JOBS)
            .args([
                "run",
                "--locked",
                "--package",
                "dotfiles",
                "--bin",
                "dotfiles",
                "--",
                "mise-install",
                key,
            ]),
        "njutest guarded managed installation",
    )?;
    ensure!(
        installed.status.success(),
        "managed njutest preparation failed"
    );
    Runner::existing(root)?;
    println!(
        "Prepared the optional njutest environment; compatibility and native parity remain unmeasured"
    );
    Ok(())
}

fn output(command: &mut Command, label: &str) -> Result<Output> {
    crate::phase_evidence::output(command, label)
}

fn read_only(command: &mut Command) -> &mut Command {
    command
        .env("MISE_AUTO_INSTALL", "false")
        .env("MISE_NOT_FOUND_AUTO_INSTALL", "false")
        .env("MISE_AUTO_PRUNE", "false")
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_BUILD_JOBS", JOBS)
        .env("RUST_TEST_THREADS", TEST_THREADS)
        .env("GIT_OPTIONAL_LOCKS", "0")
}

pub(super) fn plan(root: &Path) -> Result<()> {
    let runner = Runner::existing(root)?;
    let mut command = runner.prepared_command();
    let result = output(
        command
            .current_dir(root)
            .args(["plan", "--directory"])
            .arg(root)
            .args(["--package", PACKAGE, "--offline", "--locked"]),
        "njutest complete rules plan",
    )?;
    ensure!(result.status.success(), "pinned njutest plan failed");
    println!("Pinned njutest plan retained; native mutation parity remains unmeasured");
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let result = output(
        Tool::Git.command().current_dir(root).args(args),
        "njutest Source Git identity",
    )?;
    ensure!(
        result.status.success(),
        "cannot acquire committed assessment Source"
    );
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

#[derive(Serialize)]
struct ToolIdentity {
    upstream_revision: &'static str,
    executable_sha256: String,
    rustc: String,
    cargo: String,
    lockfile_sha256: String,
    configuration_sha256: String,
    original_engine: String,
}

struct Runner {
    executable: PathBuf,
    identity: ToolIdentity,
}

impl Runner {
    fn existing(root: &Path) -> Result<Self> {
        ensure!(
            !cfg!(windows),
            "native njutest assessment requires Mac or Linux"
        );
        let platform::tool::Provision::PinnedEnvironment {
            environment,
            key: pin,
        } = Tool::Njutest.provision()
        else {
            anyhow::bail!("njutest has no maintained pin")
        };
        let configured_bytes = fs::read(root.join(format!("mise.{environment}.toml")))?;
        let configured: toml::Value = toml::from_str(std::str::from_utf8(&configured_bytes)?)?;
        let version = format!("rev:{UPSTREAM_REVISION}");
        ensure!(
            configured["tools"][pin]["version"].as_str() == Some(version.as_str()),
            "njutest does not select the immutable upstream revision"
        );
        let route = output(
            read_only(&mut Tool::Mise.command())
                .current_dir(root)
                .env("MISE_ENV", environment)
                .args(["which", Tool::Njutest.program()]),
            "njutest managed executable",
        )?;
        ensure!(route.status.success(), "pinned njutest is not prepared");
        let executable =
            platform::paths::canonical(Path::new(String::from_utf8(route.stdout)?.trim()))?;
        let actual = output(
            read_only(&mut crate::tool::external(&executable)).arg("--version"),
            "njutest pinned version",
        )?;
        ensure!(
            actual.status.success()
                && String::from_utf8(actual.stdout)?.trim() == format!("njutest {TOOL_VERSION}"),
            "unexpected njutest version"
        );
        let rustc = output(
            read_only(&mut Tool::Rustc.command())
                .current_dir(root)
                .args(["--version", "--verbose"]),
            "njutest assessment rustc identity",
        )?;
        let cargo = output(
            read_only(&mut Tool::Cargo.command())
                .current_dir(root)
                .args(["--version", "--verbose"]),
            "njutest assessment cargo identity",
        )?;
        ensure!(
            rustc.status.success() && cargo.status.success(),
            "missing compiler identities"
        );
        let original_engine = output(
            read_only(&mut Tool::Cargo.command())
                .current_dir(root)
                .args(["mutants", "--version"]),
            "original native mutation engine version",
        )?;
        ensure!(
            original_engine.status.success()
                && String::from_utf8_lossy(&original_engine.stdout).trim()
                    == format!("cargo-mutants {ORIGINAL_ENGINE_VERSION}"),
            "unexpected original native mutation engine"
        );
        Ok(Self {
            identity: ToolIdentity {
                upstream_revision: UPSTREAM_REVISION,
                executable_sha256: digest(&fs::read(&executable)?),
                rustc: String::from_utf8(rustc.stdout)?,
                cargo: String::from_utf8(cargo.stdout)?,
                lockfile_sha256: digest(&fs::read(root.join("Cargo.lock"))?),
                configuration_sha256: digest(&configured_bytes),
                original_engine: String::from_utf8(original_engine.stdout)?,
            },
            executable,
        })
    }

    fn prepared_command(&self) -> Command {
        let mut command = crate::tool::external(&self.executable);
        read_only(&mut command);
        command
    }

    fn command(&self, cache: &Path) -> Command {
        let mut command = self.prepared_command();
        command.env("XDG_CACHE_HOME", cache);
        command
    }
}

struct Snapshot {
    scope: Option<tempfile::TempDir>,
    source: PathBuf,
    commit: String,
    tree: String,
    permissions: Vec<(PathBuf, fs::Permissions)>,
}

impl Snapshot {
    fn capture(root: &Path) -> Result<Self> {
        ensure!(
            git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
            "assessment requires clean committed Source"
        );
        let commit = git(root, &["rev-parse", "HEAD"])?;
        let tree = git(root, &["rev-parse", "HEAD^{tree}"])?;
        let scope = tempfile::tempdir()?;
        let source = scope.path().join("source");
        let cloned = output(
            Tool::Git
                .command()
                .args(["clone", "--no-local", "--no-hardlinks", "--no-checkout"])
                .arg(root)
                .arg(&source),
            "njutest owned Source capture",
        )?;
        ensure!(cloned.status.success(), "cannot capture assessment Source");
        let checked = output(
            Tool::Git
                .command()
                .current_dir(&source)
                .args(["checkout", "--detach"])
                .arg(&commit),
            "njutest captured revision checkout",
        )?;
        ensure!(
            checked.status.success() && git(&source, &["rev-parse", "HEAD^{tree}"])? == tree,
            "captured Source differs from the committed candidate"
        );
        ensure!(
            git(
                &source,
                &[
                    "ls-files",
                    "--",
                    "target",
                    "xtask/proofs/mutants-fixture/target"
                ]
            )?
            .is_empty(),
            "assessment output namespace contains tracked Source"
        );
        for directory in [source.join("target"), source.join(FIXTURE).join("target")] {
            fs::create_dir_all(directory)?;
        }
        Ok(Self {
            scope: Some(scope),
            source,
            commit,
            tree,
            permissions: Vec::new(),
        })
    }

    #[cfg(unix)]
    fn protect(&mut self) -> Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        let outputs = [
            self.source.join("target"),
            self.source.join(FIXTURE).join("target"),
        ];
        let mut pending = vec![self.source.clone()];
        let mut entries = Vec::new();
        while let Some(path) = pending.pop() {
            ensure!(
                entries.len() < MAX_ENTRIES,
                "assessment Source capture exceeds the bounded namespace"
            );
            if outputs.contains(&path) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_file() || metadata.is_dir(),
                "assessment Source contains a link or special node"
            );
            if metadata.is_dir() {
                for entry in fs::read_dir(&path)? {
                    pending.push(entry?.path());
                }
            }
            entries.push((path, metadata.permissions()));
        }
        for (path, original) in entries.into_iter().rev() {
            let protected = fs::Permissions::from_mode(original.mode() & !WRITE_PERMISSION_BITS);
            fs::set_permissions(&path, protected)?;
            self.permissions.push((path, original));
        }
        let probe = self.source.join(".njutest-write-probe");
        match fs::write(&probe, b"must refuse") {
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(()),
            result => {
                platform::cleanup::remove_file(&probe);
                anyhow::bail!("captured Source permits writes: {result:?}")
            }
        }
    }

    #[cfg(not(unix))]
    fn protect(&mut self) -> Result<()> {
        anyhow::bail!(
            "native njutest snapshot protection requires Unix: {}",
            self.source.display()
        )
    }

    fn retain_on_failure(&mut self) -> PathBuf {
        self.scope
            .take()
            .map_or_else(|| self.source.clone(), tempfile::TempDir::keep)
    }

    fn cache(&self) -> Result<PathBuf> {
        let cache = self
            .scope
            .as_ref()
            .context("missing owned assessment scope")?
            .path()
            .join("cache");
        fs::create_dir(&cache)?;
        ensure!(
            fs::read_dir(&cache)?.next().is_none(),
            "cold assessment cache is not empty"
        );
        Ok(cache)
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        for (path, permission) in self.permissions.drain(..).rev() {
            platform::cleanup::report(
                fs::set_permissions(&path, permission),
                format_args!("restore assessment-owned {}", path.display()),
            );
        }
    }
}

fn configuration(root: &Path) -> Result<PathBuf> {
    let original = root.join(CONFIGURATION);
    let mut config: toml::Value = if original.is_file() {
        toml::from_str(&fs::read_to_string(original)?)?
    } else {
        toml::from_str("version = 1\n")?
    };
    let table = config
        .as_table_mut()
        .context("njutest configuration is not a table")?;
    for name in ["execution", "reports"] {
        table
            .entry(name.to_owned())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    }
    table
        .get_mut("execution")
        .and_then(toml::Value::as_table_mut)
        .context("invalid execution configuration")?
        .insert("jobs".to_owned(), toml::Value::String(JOBS.to_owned()));
    table
        .get_mut("reports")
        .and_then(toml::Value::as_table_mut)
        .context("invalid reports configuration")?
        .insert(
            "directory".to_owned(),
            toml::Value::String(REPORT_DIRECTORY.to_owned()),
        );
    let path = root.join("target/njutest-assessment.toml");
    let bytes = toml::to_string(&config)?.into_bytes();
    fs::write(&path, &bytes)?;
    crate::phase_evidence::retain("njutest assessment", "effective-configuration", &bytes)?;
    Ok(path)
}

#[derive(Serialize)]
struct Measurement<'a> {
    commit: &'a str,
    tree: &'a str,
    tool: &'a ToolIdentity,
    temperature: Temperature,
    elapsed_ms: u128,
    exit_code: i32,
    report_sha256: String,
    cache_origin: &'static str,
    cargo_dependency_cache: &'static str,
    assessment: Assessment,
    mapping: Vec<OperatorMapping>,
}

#[derive(Serialize)]
struct OperatorMapping {
    original_identity: String,
    source_sha256: String,
    original: String,
    replacement: String,
    genre: String,
    original_span: Span,
    original_function: Option<OriginalFunction>,
    candidates: Vec<EngineOperator>,
}

#[derive(Serialize)]
struct EngineOperator {
    id: String,
    rule: String,
    rule_version: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalMutation {
    name: String,
    package: String,
    file: String,
    function: Option<OriginalFunction>,
    span: Span,
    replacement: String,
    genre: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OriginalFunction {
    function_name: String,
    return_type: String,
    span: Span,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Position {
    line: usize,
    column: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Span {
    start: Position,
    end: Position,
}

fn offset(source: &str, wanted: Position) -> Result<usize> {
    ensure!(
        wanted.line != 0 && wanted.column != 0,
        "invalid old mutation source position"
    );
    let (mut line, mut column) = (1, 1);
    for (offset, character) in source.char_indices() {
        if (line, column) == (wanted.line, wanted.column) {
            return Ok(offset);
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else if character != '\r' {
            column += 1;
        }
    }
    ensure!(
        (line, column) == (wanted.line, wanted.column),
        "old mutation span is outside Source"
    );
    Ok(source.len())
}

fn mappings(root: &Path, original: &[u8], catalogs: &[Value]) -> Result<Vec<OperatorMapping>> {
    let original: Vec<OriginalMutation> = serde_json::from_slice(original)?;
    ensure!(!original.is_empty(), "old mutation inventory is empty");
    let mut ids = BTreeSet::new();
    let mut result = Vec::new();
    for mutation in original {
        ensure!(
            mutation.package == PACKAGE && ids.insert(mutation.name.clone()),
            "old mutation selection changed or duplicated a row"
        );
        let source = fs::read_to_string(root.join(relative(&mutation.file)?))?;
        let start = offset(&source, mutation.span.start)?;
        let end = offset(&source, mutation.span.end)?;
        if let Some(function) = &mutation.function {
            ensure!(
                !function.function_name.is_empty()
                    && offset(&source, function.span.start)? <= start
                    && offset(&source, function.span.end)? >= end,
                "old function span does not contain its mutation"
            );
        }
        let original = source
            .get(start..end)
            .context("old mutation source span is inverted")?;
        let source_sha256 = digest(source.as_bytes());
        let mut candidates = Vec::new();
        for catalog in catalogs {
            for candidate in rows(catalog, "mutants")? {
                if candidate["path"] == mutation.file
                    && candidate["start_byte"] == start
                    && candidate["end_byte"] == end
                    && candidate["source_digest"] == source_sha256
                    && candidate["original"] == original
                    && candidate["replacement"] == mutation.replacement
                {
                    candidates.push(EngineOperator {
                        id: text(candidate, "id")?.to_owned(),
                        rule: text(candidate, "rule")?.to_owned(),
                        rule_version: super::count(candidate, "rule_version")?,
                    });
                }
            }
        }
        result.push(OperatorMapping {
            original_identity: mutation.name,
            source_sha256,
            original: original.to_owned(),
            replacement: mutation.replacement,
            genre: mutation.genre,
            original_span: mutation.span,
            original_function: mutation.function,
            candidates,
        });
    }
    Ok(result)
}

fn catalogs(root: &Path, trace: &Path, report: &Value) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let trace = trace.join("builds");
    for (index, build) in rows(&report["report"], "builds")?.iter().enumerate() {
        let path = trace.join(format!("{index:010}/engine/catalog-v1.json"));
        let bytes = fs::read(path).context("missing original catalog from owned trace")?;
        crate::phase_evidence::retain("njutest assessment", "original-catalog", &bytes)?;
        let catalog = closed_document(Document::Catalog, &bytes)?;
        super::catalog(&catalog)?;
        let expected: BTreeSet<_> = rows(&catalog, "mutants")?
            .iter()
            .chain(rows(&catalog, "rejections")?)
            .map(|row| text(row, "id"))
            .collect::<Result<_>>()?;
        let actual: BTreeSet<_> = rows(&build["parts"][0], "mutants")?
            .iter()
            .map(|row| text(row, "id"))
            .collect::<Result<_>>()?;
        ensure!(
            actual == expected,
            "complete report omitted or added a catalog mutation"
        );
        for candidate in rows(&catalog, "mutants")? {
            let path = root.join(relative(text(candidate, "path")?)?);
            let bytes = fs::read(&path)?;
            let start = usize::try_from(super::count(candidate, "start_byte")?)?;
            let end = usize::try_from(super::count(candidate, "end_byte")?)?;
            ensure!(
                digest(&bytes) == text(candidate, "source_digest")?
                    && bytes.get(start..end) == Some(text(candidate, "original")?.as_bytes()),
                "catalog Source digest or mutation span is stale"
            );
            let projected = rows(&build["parts"][0], "mutants")?
                .iter()
                .find(|row| row["id"] == candidate["id"])
                .context("missing catalog projection")?;
            for key in ["path", "rule", "original", "replacement"] {
                ensure!(
                    projected[key] == candidate[key],
                    "report mutation semantics contradict its original catalog"
                );
            }
        }
        result.push(catalog);
    }
    Ok(result)
}

fn trace_directory(root: &Path) -> Result<PathBuf> {
    Ok(tempfile::Builder::new()
        .prefix("njutest-trace-")
        .tempdir_in(root.join("target"))?
        .keep()
        .join(TRACE_DIRECTORY))
}

fn campaign(
    runner: &Runner,
    cache: &Path,
    root: &Path,
    package: &str,
    temperature: Temperature,
) -> Result<(Output, Assessment, Value, PathBuf)> {
    let config = configuration(root)?;
    let trace = trace_directory(root)?;
    let mut command = runner.command(cache);
    command
        .current_dir(root)
        .args(["verify", "--directory"])
        .arg(root)
        .arg("--config")
        .arg(config)
        .args([
            "--package",
            package,
            "--offline",
            "--locked",
            "--format",
            "json",
        ])
        .arg(format!("--trace={}", trace.display()));
    let result = output(
        &mut command,
        &format!("njutest {temperature:?} actual verification"),
    )?;
    let code = result
        .status
        .code()
        .context("njutest assessment was interrupted")?;
    ensure!(
        matches!(code, 0..=MAX_REPORT_EXIT_CODE),
        "njutest execution failed or was interrupted"
    );
    let report = closed_document(Document::Assurance, &result.stdout)?;
    let mut assessment = super::assurance(&report)?;
    for build in rows(&report["report"], "builds")? {
        source_rows(root, rows(&build["parts"][0], "sources")?)?;
    }
    ensure!(
        code != 0 || assessment.zero_survivors,
        "successful process contradicts mutation outcomes"
    );
    assessment.origin = Origin::Executed;
    assessment
        .limitations
        .retain(|limitation| !limitation.starts_with("Imported documents"));
    assessment.limitations.push("Actual owned producer; native baseline and operator parity still require reviewed complete comparison".to_owned());
    Ok((result, assessment, report, trace))
}

fn measure_cold(
    runner: &Runner,
    snapshot: &Snapshot,
    cache: &Path,
    inventory: &[u8],
) -> Result<(Value, Vec<Value>)> {
    let started = Instant::now();
    let (execution, assessment, report, trace) =
        campaign(runner, cache, &snapshot.source, PACKAGE, Temperature::Cold)?;
    let original_catalogs = catalogs(&snapshot.source, &trace, &report)?;
    let mapping = mappings(&snapshot.source, inventory, &original_catalogs)?;
    let (_, control, control_report, _) = campaign(
        runner,
        cache,
        &snapshot.source.join(FIXTURE),
        FIXTURE_PACKAGE,
        Temperature::Cold,
    )?;
    ensure!(
        !control.zero_survivors,
        "assertion-free fixture was admitted"
    );
    rejecting_control(&control_report)?;
    let measurement = Measurement {
        commit: &snapshot.commit,
        tree: &snapshot.tree,
        tool: &runner.identity,
        temperature: Temperature::Cold,
        elapsed_ms: started.elapsed().as_millis(),
        exit_code: execution.status.code().context("missing terminal exit")?,
        report_sha256: digest(&execution.stdout),
        cache_origin: "owned-empty-njutest-cache",
        cargo_dependency_cache: "prepared-shared-cargo-inputs",
        assessment,
        mapping,
    };
    crate::phase_evidence::retain(
        "njutest assessment",
        "cold-measurement",
        &serde_json::to_vec_pretty(&measurement)?,
    )?;
    Ok((report, original_catalogs))
}

fn measure_warm(
    runner: &Runner,
    snapshot: &Snapshot,
    cache: &Path,
    cold_report: &Value,
    original_catalogs: &[Value],
    inventory: &[u8],
) -> Result<()> {
    let started = Instant::now();
    let (execution, assessment, report, _) =
        campaign(runner, cache, &snapshot.source, PACKAGE, Temperature::Warm)?;
    ensure!(
        report["report"]["repository"] == cold_report["report"]["repository"]
            && report["report"]["scope"] == cold_report["report"]["scope"],
        "warm assessment selected different Source or configuration"
    );
    for (warm, cold) in rows(&report["report"], "builds")?
        .iter()
        .zip(rows(&cold_report["report"], "builds")?)
    {
        ensure!(
            warm["name"] == cold["name"] && warm["configuration"] == cold["configuration"],
            "warm configured build differs from its cold producer"
        );
        let warm_ids: BTreeSet<_> = rows(&warm["parts"][0], "mutants")?
            .iter()
            .map(|row| text(row, "id"))
            .collect::<Result<_>>()?;
        let cold_ids: BTreeSet<_> = rows(&cold["parts"][0], "mutants")?
            .iter()
            .map(|row| text(row, "id"))
            .collect::<Result<_>>()?;
        ensure!(
            warm_ids == cold_ids,
            "warm assessment changed the selected mutation inventory"
        );
    }
    let (_, control, control_report, _) = campaign(
        runner,
        cache,
        &snapshot.source.join(FIXTURE),
        FIXTURE_PACKAGE,
        Temperature::Warm,
    )?;
    ensure!(
        !control.zero_survivors,
        "warm assertion-free fixture was admitted"
    );
    rejecting_control(&control_report)?;
    let measurement = Measurement {
        commit: &snapshot.commit,
        tree: &snapshot.tree,
        tool: &runner.identity,
        temperature: Temperature::Warm,
        elapsed_ms: started.elapsed().as_millis(),
        exit_code: execution.status.code().context("missing terminal exit")?,
        report_sha256: digest(&execution.stdout),
        cache_origin: "same-owned-cold-producer",
        cargo_dependency_cache: "prepared-shared-cargo-inputs",
        assessment,
        mapping: mappings(&snapshot.source, inventory, original_catalogs)?,
    };
    crate::phase_evidence::retain(
        "njutest assessment",
        "warm-measurement",
        &serde_json::to_vec_pretty(&measurement)?,
    )?;
    Ok(())
}

fn measure_captured(
    runner: &Runner,
    snapshot: &Snapshot,
    cache: &Path,
    temperature: Temperature,
) -> Result<()> {
    let inventory = output(
        read_only(&mut Tool::Cargo.command())
            .args(["mutants", "--list", "--json", "--package", PACKAGE, "--dir"])
            .arg(&snapshot.source),
        "cargo-mutants original complete catalog",
    )?;
    ensure!(
        inventory.status.success(),
        "cannot acquire the current native mutation catalog"
    );
    crate::phase_evidence::retain(
        "njutest assessment",
        "original-cargo-mutants-inventory",
        &inventory.stdout,
    )?;
    let (cold_report, original_catalogs) =
        measure_cold(runner, snapshot, cache, &inventory.stdout)?;
    if matches!(temperature, Temperature::Warm) {
        measure_warm(
            runner,
            snapshot,
            cache,
            &cold_report,
            &original_catalogs,
            &inventory.stdout,
        )?;
    }
    println!(
        "Measured pinned njutest and its assertion-free control; native parity remains unmeasured"
    );
    Ok(())
}

pub(super) fn measure(root: &Path, temperature: Temperature) -> Result<()> {
    let root = platform::paths::canonical(root)?;
    let runner = Runner::existing(&root)?;
    let mut snapshot = Snapshot::capture(&root)?;
    let cache = snapshot.cache()?;
    snapshot.protect()?;
    let result = measure_captured(&runner, &snapshot, &cache, temperature);
    if result.is_err() {
        let retained = snapshot.retain_on_failure();
        return result.context(format!(
            "owned assessment artifacts retained at {}",
            retained.display()
        ));
    }
    result
}

fn rejecting_control(report: &Value) -> Result<()> {
    let mut survived = false;
    for build in rows(&report["report"], "builds")? {
        survived |= rows(&build["parts"][0], "mutants")?.iter().any(|row| {
            row["decision"]["outcome"] == "survived"
                && matches!(
                    row["evidence"]["kind"].as_str(),
                    Some("sealed" | "unproven")
                )
        });
    }
    ensure!(
        survived,
        "assertion-free control established no surviving mutation; infrastructure or missing evidence is not a sensitivity control"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_trace_allows_the_actual_runner_to_create_its_directory() -> Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir(root.path().join("target"))?;
        let trace = trace_directory(root.path())?;
        assert!(trace.parent().is_some_and(Path::is_dir));
        assert!(!trace.exists());
        fs::create_dir(&trace)?;
        assert_eq!(
            fs::create_dir(&trace).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        Ok(())
    }

    #[test]
    fn prepared_commands_refuse_automatic_installation_and_use_one_test_thread() {
        let mut command = crate::tool::external(Path::new("prepared-njutest"));
        command
            .env("MISE_AUTO_INSTALL", "true")
            .env("MISE_NOT_FOUND_AUTO_INSTALL", "true")
            .env("MISE_AUTO_PRUNE", "true")
            .env("RUST_TEST_THREADS", "2");
        read_only(&mut command);
        for (name, expected) in [
            ("MISE_AUTO_INSTALL", "false"),
            ("MISE_NOT_FOUND_AUTO_INSTALL", "false"),
            ("MISE_AUTO_PRUNE", "false"),
            ("RUST_TEST_THREADS", TEST_THREADS),
        ] {
            assert!(command.get_envs().any(|(key, value)| {
                key == name && value == Some(std::ffi::OsStr::new(expected))
            }));
        }
        assert_eq!(command.get_program(), "prepared-njutest");
    }

    #[test]
    fn old_native_spans_keep_unicode_columns_crlf_and_exclusive_end() {
        let source = "éx\r\nabc\n";
        assert_eq!(offset(source, Position { line: 1, column: 2 }).unwrap(), 2);
        assert_eq!(offset(source, Position { line: 2, column: 2 }).unwrap(), 6);
        assert!(offset(source, Position { line: 0, column: 1 }).is_err());
        assert!(offset(source, Position { line: 9, column: 1 }).is_err());
    }
}
