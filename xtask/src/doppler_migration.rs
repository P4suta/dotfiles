use anyhow::{Context, Result, bail, ensure};
use clap::{Args, ValueEnum};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::tool::Tool;

mod connections;
pub mod organization;

const API: &str = "https://api.doppler.com";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 20;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const CURL_STATUS: &str = "\n%{http_code}\n%header{retry-after}";
const RATE_LIMIT_SECONDS: u64 = 60;
const HTTP_RATE_LIMITED: u16 = 429;
const HTTP_SUCCESS: std::ops::Range<u16> = 200..300;

#[derive(Args, Debug)]
pub struct Request {
    #[arg(long)]
    source: String,
    #[arg(long)]
    destination: String,
    #[arg(long)]
    source_config_dir: PathBuf,
    #[arg(long)]
    destination_config_dir: PathBuf,
    #[arg(long)]
    scope: PathBuf,
    #[arg(long, value_enum, default_value_t = Phase::Plan)]
    phase: Phase,
    #[arg(long)]
    apply: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Phase {
    Plan,
    Copy,
    Verify,
    Connect,
    VerifyConnections,
}

struct Client {
    credential: String,
    writable: bool,
}

#[derive(Clone, Deserialize)]
struct Project {
    id: String,
    name: String,
    description: String,
}

#[derive(Clone, Deserialize)]
struct Environment {
    id: String,
    name: String,
}

#[derive(Clone, Deserialize)]
struct Config {
    name: String,
    environment: String,
    root: bool,
    locked: bool,
    #[serde(default)]
    inherits: Vec<Value>,
    #[serde(default)]
    inheritable: bool,
}

struct ProjectSnapshot {
    project: Project,
    environments: Vec<Environment>,
    configs: Vec<Config>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Secret {
    raw: Option<String>,
    computed: Option<String>,
    raw_visibility: String,
    computed_visibility: String,
    raw_value_type: Value,
    computed_value_type: Value,
    note: String,
}

type Secrets = BTreeMap<String, Secret>;
type ConfigKey = (String, String);
type SecretSnapshot = BTreeMap<ConfigKey, Secrets>;

impl Client {
    fn open(directory: &Path, scope: &Path) -> Result<Self> {
        ensure!(
            directory.is_absolute() && scope.is_absolute(),
            "absolute CLI paths required"
        );
        let output = Tool::Doppler
            .command()
            .args(["configure", "get", "token", "--plain", "--config-dir"])
            .arg(directory)
            .arg("--scope")
            .arg(scope)
            .args(["--no-read-env", "--no-check-version"])
            .stdin(Stdio::null())
            .output()
            .context("start Doppler credential lookup")?;
        ensure!(
            output.status.success(),
            "Doppler credential lookup failed; response withheld"
        );
        let credential = String::from_utf8(output.stdout).context("credential encoding invalid")?;
        let credential = credential.trim().to_owned();
        ensure!(
            credential.starts_with("dp.")
                && credential
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "unexpected credential format"
        );
        Ok(Self {
            credential,
            writable: false,
        })
    }

    fn read(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        self.request("GET", path, query, None)
    }

    fn write(&self, path: &str, query: &[(&str, &str)], body: &Value) -> Result<Value> {
        ensure!(self.writable, "workplace mutations were not authorized");
        self.request("POST", path, query, Some(body))
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value> {
        ensure!(
            method == "GET" || self.writable,
            "workplace mutations were not authorized"
        );
        let url = endpoint(path, query)?;
        let config = curl_config(&url, method, &self.credential, body)?;
        for attempt in 0..=2 {
            let mut child = Tool::Curl
                .command()
                .args([
                    "--disable",
                    "--silent",
                    "--show-error",
                    "--connect-timeout",
                    "10",
                    "--max-time",
                    "45",
                    "--max-filesize",
                ])
                .arg(MAX_RESPONSE_BYTES.to_string())
                .args(["--write-out", CURL_STATUS, "--config", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("start private Doppler API request")?;
            let mut input = child.stdin.take().context("API input unavailable")?;
            let sent = input.write_all(config.as_bytes());
            drop(input);
            let output = child
                .wait_with_output()
                .context("wait for private API request")?;
            ensure!(
                sent.is_ok() && output.status.success(),
                "Doppler transport failed at {path}; response withheld; inspect state before retrying mutations"
            );
            ensure!(
                output.stdout.len() <= MAX_RESPONSE_BYTES,
                "API response exceeds memory budget"
            );
            let text = std::str::from_utf8(&output.stdout)
                .map_err(|_| anyhow::anyhow!("invalid API encoding at {path}; body withheld"))?;
            let (body, status, retry) = response_parts(text)?;
            if status == HTTP_RATE_LIMITED && attempt < 2 {
                let delay = retry.parse::<u64>().unwrap_or(RATE_LIMIT_SECONDS);
                ensure!(
                    delay <= RATE_LIMIT_SECONDS,
                    "API rate limit needs a later retry"
                );
                eprintln!("Doppler rate limit: waiting {delay} seconds");
                std::thread::sleep(std::time::Duration::from_secs(delay));
                continue;
            }
            ensure!(
                HTTP_SUCCESS.contains(&status),
                "Doppler API {method} {path} returned HTTP {status}; body withheld"
            );
            let value = response_body(body, method)?;
            ensure!(
                value.get("success").and_then(Value::as_bool) != Some(false),
                "Doppler rejected {path}; body withheld"
            );
            return Ok(value);
        }
        bail!("Doppler rate limit remained active")
    }

    fn list<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        key: &str,
    ) -> Result<Vec<T>> {
        let mut rows = Vec::new();
        for page in 1..=MAX_PAGES {
            let number = page.to_string();
            let size = PAGE_SIZE.to_string();
            let mut params = query.to_vec();
            params.extend([("page", number.as_str()), ("per_page", size.as_str())]);
            let response = self.read(path, &params)?;
            let values = response
                .get(key)
                .and_then(Value::as_array)
                .with_context(|| format!("missing {key} metadata at {path}"))?;
            let finished = values.len() < PAGE_SIZE;
            for value in values {
                rows.push(decode(value.clone(), path)?);
            }
            if finished {
                return Ok(rows);
            }
        }
        bail!("pagination budget exceeded at {path}")
    }
}

fn decode<T: for<'de> Deserialize<'de>>(value: Value, path: &str) -> Result<T> {
    serde_json::from_value(value)
        .map_err(|_| anyhow::anyhow!("unexpected API schema at {path}; body withheld"))
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value
        .get(name)
        .with_context(|| format!("missing API field {name}"))
}

fn endpoint(path: &str, query: &[(&str, &str)]) -> Result<String> {
    ensure!(
        path.starts_with("/v3/") && !path.contains(['?', '#', '\n', '\r']),
        "invalid API path"
    );
    let mut url = format!("{API}{path}");
    for (index, (key, value)) in query.iter().enumerate() {
        ensure!(
            key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "invalid query key"
        );
        url.push(if index == 0 { '?' } else { '&' });
        url.push_str(key);
        url.push('=');
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                url.push(char::from(byte));
            } else {
                use std::fmt::Write as _;
                write!(&mut url, "%{byte:02X}")?;
            }
        }
    }
    Ok(url)
}

fn curl_config(url: &str, method: &str, credential: &str, body: Option<&Value>) -> Result<String> {
    let mut config = format!(
        "url = {}\nrequest = {}\nheader = {}\nheader = \"Accept: application/json\"\n",
        serde_json::to_string(url)?,
        serde_json::to_string(method)?,
        serde_json::to_string(&format!("Authorization: Bearer {credential}"))?
    );
    if let Some(body) = body {
        config.push_str("header = \"Content-Type: application/json\"\ndata-binary = ");
        config.push_str(&serde_json::to_string(&serde_json::to_string(body)?)?);
        config.push('\n');
    }
    Ok(config)
}

fn response_parts(text: &str) -> Result<(&str, u16, &str)> {
    let (body, retry) = text
        .rsplit_once('\n')
        .context("API rate metadata missing")?;
    let (body, status) = body.rsplit_once('\n').context("API status missing")?;
    let status: u16 = status.parse().context("invalid API status")?;
    Ok((body, status, retry))
}

fn response_body(body: &str, method: &str) -> Result<Value> {
    if method == "DELETE" && body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(body).map_err(|_| anyhow::anyhow!("invalid API JSON; body withheld"))
}

fn inventory(client: &Client) -> Result<Vec<ProjectSnapshot>> {
    let projects: Vec<Project> = client.list("/v3/projects", &[], "projects")?;
    projects
        .into_iter()
        .map(|project| {
            let query = [("project", project.id.as_str())];
            let environments = client.list("/v3/environments", &query, "environments")?;
            let configs: Vec<Config> = client.list("/v3/configs", &query, "configs")?;
            ensure!(
                configs
                    .iter()
                    .all(|c| c.inherits.is_empty() && !c.inheritable),
                "explicit config inheritance needs a separate migration adapter"
            );
            Ok(ProjectSnapshot {
                project,
                environments,
                configs,
            })
        })
        .collect()
}

fn ensure_structure(
    source: &[ProjectSnapshot],
    destination: &[ProjectSnapshot],
    client: &Client,
) -> Result<()> {
    for snapshot in source {
        let project = &snapshot.project;
        if let Some(existing) = destination.iter().find(|s| s.project.id == project.id) {
            ensure!(
                existing.project.name == project.name
                    && existing.project.description == project.description,
                "destination project metadata conflicts for {}",
                project.id
            );
        } else {
            let created = client.write(
                "/v3/projects",
                &[],
                &json!({"name":project.name,"description":project.description}),
            )?;
            ensure!(
                field(field(&created, "project")?, "id")?.as_str() == Some(&project.id),
                "created project slug differs"
            );
        }
        let query = [("project", project.id.as_str())];
        let environments: Vec<Environment> =
            client.list("/v3/environments", &query, "environments")?;
        for environment in &snapshot.environments {
            if let Some(existing) = environments.iter().find(|e| e.id == environment.id) {
                ensure!(
                    existing.name == environment.name,
                    "destination environment metadata conflicts"
                );
            } else {
                client.write(
                    "/v3/environments",
                    &[],
                    &json!({"project":project.id,"name":environment.name,"slug":environment.id}),
                )?;
            }
        }
        let configs: Vec<Config> = client.list("/v3/configs", &query, "configs")?;
        for config in &snapshot.configs {
            if let Some(existing) = configs.iter().find(|c| c.name == config.name) {
                ensure!(
                    existing.environment == config.environment && existing.root == config.root,
                    "destination config metadata conflicts"
                );
            } else {
                client.write(
                    "/v3/configs",
                    &query,
                    &json!({"name":config.name,"environment":config.environment}),
                )?;
            }
            if config.locked && !configs.iter().any(|c| c.name == config.name && c.locked) {
                client.write(
                    "/v3/configs/config/lock",
                    &[("project", &project.id), ("config", &config.name)],
                    &json!({}),
                )?;
            }
        }
        println!("Prepared project {}", project.id);
    }
    Ok(())
}

fn secrets(client: &Client, project: &str, config: &str) -> Result<Secrets> {
    let response = client.read(
        "/v3/configs/config/secrets",
        &[
            ("project", project),
            ("config", config),
            ("include_dynamic_secrets", "false"),
            ("include_managed_secrets", "false"),
        ],
    )?;
    let values: Secrets = decode(field(&response, "secrets")?.clone(), "secrets")?;
    ensure!(
        values
            .values()
            .all(|s| s.raw.is_some() && s.computed.is_some()),
        "unreadable or restricted source value; use an approved scoped non-user credential"
    );
    Ok(values)
}

fn secret_snapshot(client: &Client, projects: &[ProjectSnapshot]) -> Result<SecretSnapshot> {
    let mut snapshot = BTreeMap::new();
    for project in projects {
        for config in &project.configs {
            snapshot.insert(
                (project.project.id.clone(), config.name.clone()),
                secrets(client, &project.project.id, &config.name)?,
            );
        }
        println!("Read private values for {}", project.project.id);
    }
    Ok(snapshot)
}

fn raw_equal(left: &Secret, right: &Secret) -> bool {
    left.raw == right.raw
        && left.raw_visibility == right.raw_visibility
        && left.raw_value_type == right.raw_value_type
}

fn copy_values(client: &Client, snapshot: &SecretSnapshot) -> Result<()> {
    let mut pending: Vec<_> = snapshot.keys().cloned().collect();
    pending.sort_by_key(|(project, config)| {
        (
            !project.starts_with("shared-"),
            config.ends_with("_personal"),
            project.clone(),
            config.clone(),
        )
    });
    let mut notes = BTreeMap::new();
    for (project, config) in pending {
        let values = snapshot
            .get(&(project.clone(), config.clone()))
            .context("source config missing")?;
        let existing = secrets(client, &project, &config)?;
        ensure!(
            existing.keys().all(|key| values.contains_key(key)),
            "destination has extra fields in {project}/{config}"
        );
        let mut changes = Vec::new();
        for (name, value) in values {
            if let Some(old) = existing.get(name) {
                ensure!(
                    raw_equal(old, value),
                    "destination conflicts for {project}/{config}/{name}"
                );
            } else {
                changes.push(json!({"name":name,"value":value.raw,"visibility":value.raw_visibility,"valueType":value.raw_value_type,"originalName":null}));
            }
            if !value.note.is_empty() {
                let key = (project.clone(), name.clone());
                if let Some(previous) = notes.insert(key, value.note.clone()) {
                    ensure!(
                        previous == value.note,
                        "source project note differs across configs"
                    );
                }
            }
        }
        if !changes.is_empty() {
            client.write(
                "/v3/configs/config/secrets",
                &[("project", &project), ("config", &config)],
                &json!({"change_requests":changes}),
            )?;
        }
        println!("Copied {project}/{config}: {} fields", values.len());
    }
    for ((project, name), note) in notes {
        client.write(
            "/v3/projects/project/note",
            &[("project", &project)],
            &json!({"secret":name,"note":note}),
        )?;
    }
    Ok(())
}

fn verify_values(client: &Client, source: &SecretSnapshot) -> Result<()> {
    let mut fields = 0;
    for ((project, config), values) in source {
        let destination = secrets(client, project, config)?;
        ensure!(
            values == &destination,
            "secret equality verification failed for {project}/{config}; values withheld"
        );
        fields += values.len();
    }
    println!(
        "Verified {} configs and {fields} fields: raw/computed values, notes, visibility, and types match",
        source.len()
    );
    Ok(())
}

fn verify_structure(source: &[ProjectSnapshot], destination: &[ProjectSnapshot]) -> Result<()> {
    let source_ids: BTreeSet<_> = source.iter().map(|p| &p.project.id).collect();
    let destination_ids: BTreeSet<_> = destination.iter().map(|p| &p.project.id).collect();
    ensure!(source_ids == destination_ids, "project sets differ");
    for project in source {
        let target = destination
            .iter()
            .find(|p| p.project.id == project.project.id)
            .context("destination project missing")?;
        ensure!(
            target.project.name == project.project.name
                && target.project.description == project.project.description,
            "project metadata differs"
        );
        ensure!(
            project.environments.len() == target.environments.len()
                && project.environments.iter().all(|e| target
                    .environments
                    .iter()
                    .any(|t| t.id == e.id && t.name == e.name)),
            "environment metadata differs"
        );
        ensure!(
            project.configs.len() == target.configs.len()
                && project
                    .configs
                    .iter()
                    .all(|c| target.configs.iter().any(|t| t.name == c.name
                        && t.environment == c.environment
                        && t.root == c.root
                        && t.locked == c.locked)),
            "config metadata differs"
        );
    }
    Ok(())
}

/// # Errors
/// Refuses identity mismatches, unsupported inheritance, conflicts, and failed private verification.
pub fn run(request: &Request) -> Result<()> {
    let source = Client::open(&request.source_config_dir, &request.scope)?;
    let mut destination = Client::open(&request.destination_config_dir, &request.scope)?;
    let source_workplace = source.read("/v3/workplace", &[])?;
    let destination_workplace = destination.read("/v3/workplace", &[])?;
    let source_matches =
        field(field(&source_workplace, "workplace")?, "id")?.as_str() == Some(&request.source);
    let destination_matches = field(field(&destination_workplace, "workplace")?, "id")?.as_str()
        == Some(&request.destination);
    ensure!(
        source_matches && destination_matches && request.source != request.destination,
        "workplace identity verification failed"
    );
    destination.writable = rules::doppler_migration_rules::may_copy(
        if request.apply {
            rules::doppler_migration_rules::Mode::Apply
        } else {
            rules::doppler_migration_rules::Mode::ReadOnly
        },
        source_matches,
        destination_matches,
        request.source != request.destination,
    );
    ensure!(
        !matches!(request.phase, Phase::Copy | Phase::Connect) || destination.writable,
        "copy and connect require --apply"
    );
    ensure!(
        !request.apply || matches!(request.phase, Phase::Copy | Phase::Connect),
        "--apply is only valid for copy and connect"
    );
    let source_projects = inventory(&source)?;
    let destination_projects = inventory(&destination)?;
    println!(
        "Verified workplaces; source: {} projects, {} configs; destination: {} projects",
        source_projects.len(),
        source_projects
            .iter()
            .map(|p| p.configs.len())
            .sum::<usize>(),
        destination_projects.len()
    );
    if request.phase == Phase::Plan {
        return Ok(());
    }
    if request.phase == Phase::Copy {
        ensure_structure(&source_projects, &destination_projects, &destination)?;
    }
    let snapshot = secret_snapshot(&source, &source_projects)?;
    if request.phase == Phase::Copy {
        copy_values(&destination, &snapshot)?;
    }
    verify_structure(&source_projects, &inventory(&destination)?)?;
    verify_values(&destination, &snapshot)?;
    if matches!(request.phase, Phase::Connect | Phase::VerifyConnections) {
        connections::run(&source, &destination, &source_projects, &snapshot)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn curl_stdin_config_preserves_multiline_values_and_literal_expressions() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = json!({"secrets":{"KEY":"line one\n\"quoted\"\r\n\\literal\t日本語\u{0008}\u{000c}","REF":"${shared.release.KEY}"}});
        let expected = body.clone();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = socket.read(&mut buffer).unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&bytes[..header_end]).unwrap();
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(str::to_owned)
                        })
                        .unwrap()
                        .parse()
                        .unwrap();
                    if bytes.len() >= header_end + 4 + length {
                        let received: Value =
                            serde_json::from_slice(&bytes[header_end + 4..]).unwrap();
                        assert_eq!(received, expected);
                        assert!(header.contains("Authorization: Bearer fixture-only"));
                        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\nConnection: close\r\n\r\n{\"success\":true}").unwrap();
                        break;
                    }
                }
            }
        });
        let config = curl_config(
            &format!("http://{address}"),
            "POST",
            "fixture-only",
            Some(&body),
        )
        .unwrap();
        let mut child = Tool::Curl
            .command()
            .args([
                "--disable",
                "--silent",
                "--max-time",
                "10",
                "--noproxy",
                "*",
                "--config",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(config.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        server.join().unwrap();
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"success":true})
        );
    }

    #[test]
    fn api_query_encodes_delimiters_without_changing_the_host() {
        assert_eq!(
            endpoint("/v3/configs", &[("project", "a&config=b\n")]).unwrap(),
            format!("{API}/v3/configs?project=a%26config%3Db%0A")
        );
        assert!(endpoint("https://other.invalid/v3/configs", &[]).is_err());
        assert!(endpoint("/v3/configs?x=y", &[]).is_err());
    }

    #[test]
    fn status_parser_keeps_multiline_json_private_and_intact() {
        assert_eq!(
            response_parts("{\n\"success\":true\n}\n200\n").unwrap(),
            ("{\n\"success\":true\n}", 200, "")
        );
        assert_eq!(response_parts("{}\n429\n7").unwrap(), ("{}", 429, "7"));
        assert!(response_parts("malformed").is_err());
    }

    #[test]
    fn source_and_read_only_clients_refuse_mutations_before_starting_curl() {
        let client = Client {
            credential: "test-only".to_owned(),
            writable: false,
        };
        assert!(client.write("/v3/projects", &[], &json!({})).is_err());
        assert!(
            client
                .request("DELETE", "/v3/projects/project", &[], None)
                .is_err()
        );
    }

    #[test]
    fn successful_deletes_accept_an_empty_body_without_relaxing_reads() {
        assert_eq!(response_body("", "DELETE").unwrap(), Value::Null);
        assert!(response_body("", "GET").is_err());
        assert!(
            !response_body("private-fixture", "GET")
                .unwrap_err()
                .to_string()
                .contains("private-fixture")
        );
    }
}
