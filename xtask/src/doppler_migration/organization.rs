use super::{Client, Tool, decode, field, inventory};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use rules::doppler_organization_rules::{Change, Cutover, Evidence, Mode, Scope, Verdict, decide};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;

mod cutover;
mod model;
mod prepare;
mod retire;
mod workflows;

const IDENTITY_LIMIT: usize = 50;
const REPORT_LIMIT: u64 = 8 * 1024 * 1024;
const GITHUB_ORIGIN: &str = "https://github.com/";
const IDENTITY_TTL: usize = 600;
const ISSUER: &str = "https://token.actions.githubusercontent.com";
const FETCH_ACTION: &str =
    "dopplerhq/secrets-fetch-action@451892f16195f9ac360e1a5bcbf0b5fd0e957534";
const VERIFY_WORKFLOW: &str = ".github/workflows/verify-doppler.yml";

#[derive(Args, Debug)]
pub struct Request {
    #[arg(long)]
    workplace: String,
    #[arg(long)]
    config_dir: PathBuf,
    #[arg(long)]
    scope: PathBuf,
    #[arg(long)]
    checkouts: PathBuf,
    #[arg(long)]
    report: PathBuf,
    #[arg(long, value_enum, default_value_t = Phase::Plan)]
    phase: Phase,
    #[arg(long)]
    apply: bool,
    #[arg(long)]
    source: Option<String>,
    #[arg(long)]
    source_config_dir: Option<PathBuf>,
    #[arg(long)]
    consumer: Option<String>,
    #[arg(long)]
    verification_run: Option<u64>,
    #[arg(long)]
    verification_attempt: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Phase {
    Plan,
    Prepare,
    StopSourceSyncs,
    Workflows,
    Verify,
    Cleanup,
    RetireConsumer,
}

#[derive(Debug, Deserialize, Serialize)]
struct Report {
    workplace: String,
    checkouts: PathBuf,
    repositories: Vec<Repository>,
    configs: Vec<ConfigNames>,
    consumers: Vec<Consumer>,
    legacy_registry: Vec<ConfigNames>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ConfigNames {
    project: String,
    config: String,
    names: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Repository {
    name: String,
    id: u64,
    owner_id: u64,
    revision: String,
    directory: PathBuf,
    subject: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Purpose {
    Automation,
    SignWindows,
    SignMacos,
    NotarizeMacos,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Consumer {
    project: String,
    source: String,
    purpose: Purpose,
    repository: String,
    shared: String,
    aliases: BTreeMap<String, String>,
    jobs: Vec<Job>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Job {
    workflow: String,
    name: String,
    environment: String,
    refs: BTreeSet<String>,
    events: BTreeSet<String>,
}

struct Session {
    client: Client,
    mode: Mode,
}

impl Session {
    fn open(directory: &Path, scope: &Path, workplace: &str, apply: bool) -> Result<Self> {
        let mut client = Client::open(directory, scope)?;
        let response = client.read("/v3/workplace", &[])?;
        ensure!(
            text(field(&response, "workplace")?, "id")? == workplace,
            "workplace identity differs"
        );
        client.writable = apply;
        Ok(Self {
            client,
            mode: if apply { Mode::Apply } else { Mode::ReadOnly },
        })
    }

    fn change(
        &self,
        change: Change,
        evidence: Evidence,
        path: &str,
        query: &[(&str, &str)],
        body: &Value,
    ) -> Result<Value> {
        ensure!(
            decide(self.mode, change, evidence) == Verdict::Allowed,
            "organization change refused: {:?}",
            decide(self.mode, change, evidence)
        );
        self.client.write(path, query, body)
    }
}

fn evidence() -> Evidence {
    Evidence {
        scope: Scope {
            workplace: true,
            repository: true,
        },
        capacity: true,
        cutover: Cutover {
            destination: false,
            consumer: false,
        },
        unused: false,
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .with_context(|| format!("invalid metadata field {key}"))
}

fn gh(path: &str) -> Result<Value> {
    let output = Tool::Gh
        .command()
        .args(["api", path])
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "GitHub metadata request failed at {path}; body withheld"
    );
    serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow::anyhow!("invalid GitHub metadata; body withheld"))
}

fn git(directory: &Path, arguments: &[&str]) -> Result<String> {
    let output = Tool::Git
        .command()
        .current_dir(directory)
        .args(arguments)
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "Git metadata lookup failed; output withheld"
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn names(client: &Client, project: &str, config: &str) -> Result<BTreeSet<String>> {
    let response = client.read(
        "/v3/configs/config/secrets/names",
        &[
            ("project", project),
            ("config", config),
            ("include_dynamic_secrets", "false"),
            ("include_managed_secrets", "false"),
        ],
    )?;
    decode(field(&response, "names")?.clone(), "secret names")
}

fn write_report(path: &Path, report: &Report) -> Result<()> {
    use std::io::Write;
    ensure!(
        path.is_absolute() && !path.starts_with(&report.checkouts),
        "report must be machine-local and outside the checkouts"
    );
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(report)?)?;
    file.sync_all()?;
    Ok(())
}

fn read_report(request: &Request) -> Result<Report> {
    ensure!(
        std::fs::symlink_metadata(&request.report)?.is_file(),
        "report must be a native file"
    );
    ensure!(
        std::fs::metadata(&request.report)?.len() <= REPORT_LIMIT,
        "organization report exceeds the supported size"
    );
    let report: Report = serde_json::from_slice(&std::fs::read(&request.report)?)?;
    ensure!(
        report.workplace == request.workplace && report.checkouts == request.checkouts,
        "organization report scope differs"
    );
    for repository in &report.repositories {
        ensure!(
            repository.directory.parent() == Some(request.checkouts.as_path()),
            "repository escaped the checkout scope"
        );
        if request.phase != Phase::RetireConsumer {
            model::verify_repository(repository)?;
        }
    }
    Ok(report)
}

/// # Errors
/// Refuses identity drift, unexpected resources, missing consumer evidence, and writes without `--apply`.
pub fn run(request: &Request) -> Result<()> {
    ensure!(
        request.phase == Phase::RetireConsumer
            || request.consumer.is_none()
                && request.verification_run.is_none()
                && request.verification_attempt.is_none(),
        "consumer verification arguments require --phase retire-consumer"
    );
    ensure!(
        request.checkouts.is_absolute() && request.report.is_absolute(),
        "absolute organization paths required"
    );
    ensure!(
        request.phase == Phase::RetireConsumer
            || request.apply
                == matches!(
                    request.phase,
                    Phase::Prepare | Phase::StopSourceSyncs | Phase::Workflows | Phase::Cleanup
                ),
        "mutation phases require --apply; read-only phases refuse it"
    );
    let session = Session::open(
        &request.config_dir,
        &request.scope,
        &request.workplace,
        request.apply,
    )?;
    if request.phase == Phase::Plan {
        let report = model::inspect(&session.client, &request.workplace, &request.checkouts)?;
        write_report(&request.report, &report)?;
        println!(
            "{}",
            json!({"phase":"plan","projects":report.configs.iter().map(|c| &c.project).collect::<BTreeSet<_>>().len(),"configs":report.configs.len(),"empty":report.configs.iter().filter(|c| c.names.is_empty()).count(),"consumers":report.consumers.len(),"legacy_registry_configs":report.legacy_registry.len(),"report":request.report})
        );
        return Ok(());
    }
    let report = read_report(request)?;
    match request.phase {
        Phase::Plan => {}
        Phase::Prepare => prepare::run(&session, &report)?,
        Phase::StopSourceSyncs => retire::stop_source(request, &session, &report)?,
        Phase::Workflows => workflows::rewrite(&report)?,
        Phase::Verify => prepare::verify(&session, &report)?,
        Phase::Cleanup => retire::cleanup(request, &session, &report)?,
        Phase::RetireConsumer => cutover::run(request, &session, &report)?,
    }
    Ok(())
}
