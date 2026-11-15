use super::{Client, Config, Secrets, Tool, decode, field};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use rules::doppler_organization_rules::{Change, Cutover, Evidence, Mode, Scope, Verdict, decide};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Stdio;

const SYNC_LIMIT: usize = 100;
const FIELD_LIMIT: usize = 100;
const MANIFEST_LIMIT: u64 = 64 * 1024;
const MANAGED_FIELDS: [&str; 3] = ["DOPPLER_PROJECT", "DOPPLER_ENVIRONMENT", "DOPPLER_CONFIG"];
const INTEGRATIONS: &str = "/v3/integrations";
const SYNC_ENDPOINT: &str = "/v3/configs/config/syncs";

#[derive(Args, Debug)]
pub struct Request {
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    config_dir: PathBuf,
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
    Prepare,
    Verify,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    workplace: String,
    repository: String,
    repository_id: u64,
    owner_id: u64,
    environment: String,
    branches: BTreeSet<String>,
    #[serde(default)]
    tags: BTreeSet<String>,
    project: String,
    source_config: String,
    config: String,
    fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Integration {
    slug: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    enabled: bool,
    syncs: Vec<Sync>,
}

#[derive(Debug, Deserialize)]
struct Sync {
    slug: String,
    project: String,
    config: String,
    description: String,
    enabled: bool,
    status: String,
}

#[derive(Deserialize)]
struct DestinationOptions {
    #[serde(rename = "ownerType")]
    owner_type: String,
    repos: BTreeMap<String, Vec<RepositoryOption>>,
}

#[derive(Deserialize)]
struct RepositoryOption {
    id: u64,
    name: String,
    archived: bool,
    owner: RepositoryOwner,
}

#[derive(Deserialize)]
struct RepositoryOwner {
    id: u64,
    login: String,
    #[serde(rename = "type")]
    kind: String,
}

fn validate_destination_options(manifest: &Manifest, value: &Value) -> Result<()> {
    let options: DestinationOptions = decode(
        field(value, "options")?.clone(),
        "GitHub destination options",
    )?;
    ensure!(
        options.owner_type == "User" && options.repos.len() == 1,
        "one personal GitHub account is required"
    );
    let (owner, name) = manifest
        .repository
        .split_once('/')
        .context("repository owner missing")?;
    let candidates: Vec<_> = options
        .repos
        .get(owner)
        .context("connected GitHub owner differs")?
        .iter()
        .filter(|repository| repository.name == name)
        .collect();
    let [repository] = candidates.as_slice() else {
        anyhow::bail!(
            "one accessible repository option is required; access or option pagination may be incomplete"
        );
    };
    ensure!(
        repository.id == manifest.repository_id
            && repository.owner.id == manifest.owner_id
            && repository.owner.login == owner
            && repository.owner.kind == "User"
            && !repository.archived,
        "connected GitHub repository identity differs or is archived"
    );
    Ok(())
}

fn destination_options(
    client: &Client,
    manifest: &Manifest,
    connection: &Integration,
) -> Result<()> {
    let value = client.read(
        "/v3/integrations/integration/options",
        &[("integration", &connection.slug)],
    )?;
    validate_destination_options(manifest, &value)
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

fn secret_name(value: &str) -> bool {
    let first = value.as_bytes().first().copied();
    first.is_some_and(|b| b.is_ascii_uppercase() || b == b'_')
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && !value.starts_with("GITHUB_")
        && !value.starts_with("DOPPLER_")
}

impl Manifest {
    fn validate(&self) -> Result<()> {
        let (owner, repository) = self
            .repository
            .split_once('/')
            .context("exact repository required")?;
        ensure!(
            [
                owner,
                repository,
                &self.environment,
                &self.project,
                &self.source_config,
                &self.config
            ]
            .into_iter()
            .all(component),
            "invalid target component"
        );
        ensure!(
            self.repository_id != 0 && self.owner_id != 0,
            "immutable repository identities required"
        );
        ensure!(
            !self.branches.is_empty() || !self.tags.is_empty(),
            "restricted deployment refs required"
        );
        ensure!(
            self.branches
                .iter()
                .chain(&self.tags)
                .all(|s| !s.is_empty() && !s.contains(['\n', '\r'])),
            "invalid deployment ref"
        );
        ensure!(
            self.branches.iter().all(|s| !s.contains(['*', '?', '[']))
                && self.tags.iter().all(|s| s != "*"),
            "unrestricted deployment patterns are refused"
        );
        ensure!(
            !self.fields.is_empty() && self.fields.len() + MANAGED_FIELDS.len() <= FIELD_LIMIT,
            "unsupported field count"
        );
        ensure!(
            self.fields
                .iter()
                .all(|(target, source)| secret_name(target) && component(source)),
            "incompatible secret name"
        );
        ensure!(
            self.config != self.source_config || self.fields.iter().all(|(a, b)| a == b),
            "renamed fields require a separate sync config"
        );
        Ok(())
    }

    fn description(&self) -> String {
        format!(
            "Actions: {} / {}",
            self.repository.replace('/', " / "),
            self.environment
        )
    }

    fn names(&self) -> BTreeSet<String> {
        self.fields.keys().cloned().collect()
    }
}

fn gh(endpoint: &str) -> Result<Value> {
    let output = Tool::Gh
        .command()
        .args(["api", endpoint])
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success(),
        "GitHub metadata lookup failed; response withheld"
    );
    let value: Value = serde_json::from_slice(&output.stdout).context("invalid GitHub metadata")?;
    ensure!(
        value
            .get("total_count")
            .and_then(Value::as_u64)
            .is_none_or(|n| n <= FIELD_LIMIT as u64),
        "GitHub pagination exceeds supported scope"
    );
    Ok(value)
}

fn github_scope(manifest: &Manifest) -> Result<Value> {
    let base = format!("repos/{}", manifest.repository);
    let repository = gh(&base)?;
    ensure!(
        repository.get("id").and_then(Value::as_u64) == Some(manifest.repository_id)
            && repository.pointer("/owner/id").and_then(Value::as_u64) == Some(manifest.owner_id)
            && repository.get("full_name").and_then(Value::as_str) == Some(&manifest.repository),
        "GitHub repository identity differs"
    );
    let endpoint = format!("{base}/environments/{}", manifest.environment);
    let environment = gh(&endpoint)?;
    ensure!(
        environment.get("name").and_then(Value::as_str) == Some(&manifest.environment),
        "GitHub environment identity differs"
    );
    ensure!(
        environment
            .pointer("/deployment_branch_policy/custom_branch_policies")
            .and_then(Value::as_bool)
            == Some(true)
            && environment
                .pointer("/deployment_branch_policy/protected_branches")
                .and_then(Value::as_bool)
                == Some(false),
        "environment requires explicit deployment branch policies"
    );
    let policies = gh(&format!(
        "{endpoint}/deployment-branch-policies?per_page=100"
    ))?;
    let rows = field(&policies, "branch_policies")?
        .as_array()
        .context("deployment policies missing")?;
    let mut branches = BTreeSet::new();
    let mut tags = BTreeSet::new();
    for row in rows {
        let name = field(row, "name")?
            .as_str()
            .context("invalid deployment policy name")?
            .to_owned();
        match field(row, "type")?.as_str() {
            Some("branch") => {
                branches.insert(name);
            }
            Some("tag") => {
                tags.insert(name);
            }
            _ => anyhow::bail!("unsupported deployment policy type"),
        }
    }
    ensure!(
        branches == manifest.branches && tags == manifest.tags,
        "deployment refs differ from the selected policy"
    );
    Ok(environment)
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

fn integrations(client: &Client) -> Result<Vec<Integration>> {
    let response = client.read(INTEGRATIONS, &[])?;
    decode(
        field(&response, "integrations")?.clone(),
        "integration metadata",
    )
}

fn selected<'a>(
    manifest: &Manifest,
    integrations: &'a [Integration],
) -> Result<(&'a Integration, Option<&'a Sync>)> {
    let owner = manifest
        .repository
        .split_once('/')
        .context("repository owner missing")?
        .0;
    let matches: Vec<_> = integrations
        .iter()
        .filter(|i| i.kind == "github-actions" && i.name == owner && i.enabled)
        .collect();
    let [integration] = matches.as_slice() else {
        anyhow::bail!("one enabled GitHub connection required");
    };
    let description = manifest.description();
    let writers: Vec<_> = integrations
        .iter()
        .flat_map(|i| &i.syncs)
        .filter(|s| s.description == description)
        .collect();
    ensure!(writers.len() <= 1, "competing destination writers");
    if let Some(sync) = writers.first() {
        ensure!(
            sync.project == manifest.project
                && sync.config == manifest.config
                && integration.syncs.iter().any(|s| s.slug == sync.slug),
            "destination already has another writer"
        );
        ensure!(
            sync.enabled && sync.status == "synced",
            "existing sync is not healthy"
        );
    }
    ensure!(
        !writers.is_empty()
            || integrations.iter().map(|i| i.syncs.len()).sum::<usize>() < SYNC_LIMIT,
        "published sync capacity exhausted"
    );
    Ok((integration, writers.first().copied()))
}

fn destination_fields(manifest: &Manifest, owned: bool) -> Result<()> {
    let endpoint = format!(
        "repos/{}/environments/{}",
        manifest.repository, manifest.environment
    );
    let secrets = gh(&format!("{endpoint}/secrets?per_page=100"))?;
    let variables = gh(&format!("{endpoint}/variables?per_page=100"))?;
    let selected: BTreeSet<_> = manifest
        .names()
        .into_iter()
        .chain(MANAGED_FIELDS.map(str::to_owned))
        .collect();
    let repository_secrets = gh(&format!(
        "repos/{}/actions/secrets?per_page=100",
        manifest.repository
    ))?;
    for row in field(&repository_secrets, "secrets")?
        .as_array()
        .context("repository secret metadata missing")?
    {
        let name = field(row, "name")?
            .as_str()
            .context("repository secret name missing")?;
        ensure!(
            !selected.contains(name),
            "repository secret would provide an implicit fallback at {name}"
        );
    }
    for (value, kind) in [(&secrets, "secrets"), (&variables, "variables")] {
        let rows = field(value, kind)?
            .as_array()
            .context("destination field metadata missing")?;
        for row in rows {
            let name = field(row, "name")?
                .as_str()
                .context("destination field name missing")?;
            ensure!(
                !selected.contains(name) || owned && kind == "secrets",
                "destination field collision at {name}"
            );
        }
    }
    let existing: BTreeSet<_> = field(&secrets, "secrets")?
        .as_array()
        .context("destination secret metadata missing")?
        .iter()
        .map(|row| {
            field(row, "name")?
                .as_str()
                .map(str::to_owned)
                .context("destination secret name missing")
        })
        .collect::<Result<_>>()?;
    ensure!(
        existing.union(&selected).count() <= FIELD_LIMIT,
        "GitHub environment secret capacity exhausted"
    );
    Ok(())
}

fn configs(client: &Client, manifest: &Manifest) -> Result<Vec<Config>> {
    client.list("/v3/configs", &[("project", &manifest.project)], "configs")
}

fn source_fields(client: &Client, manifest: &Manifest) -> Result<()> {
    let source = names(client, &manifest.project, &manifest.source_config)?;
    ensure!(
        manifest.fields.values().all(|name| source.contains(name)),
        "source fields missing"
    );
    if manifest.config == manifest.source_config {
        ensure!(
            source == manifest.names(),
            "direct sync would expose unrelated fields"
        );
    }
    Ok(())
}

fn reference_update(references: &BTreeMap<String, String>) -> Value {
    let changes: Vec<_> = references
        .iter()
        .map(|(name, reference)| {
            json!({"name":name,"value":reference,"visibility":"masked","originalName":null})
        })
        .collect();
    json!({"change_requests":changes})
}

fn reference_target(manifest: &Manifest, source: &str, raw: &str) -> Result<String> {
    if !raw.contains("${") {
        return Ok(format!(
            "${{{}.{}.{source}}}",
            manifest.project, manifest.source_config
        ));
    }
    let target = raw
        .strip_prefix("${")
        .and_then(|s| s.strip_suffix('}'))
        .context("source must be a literal or one direct reference; value withheld")?;
    let parts: Vec<_> = target.split('.').collect();
    ensure!(
        parts.iter().all(|s| component(s)),
        "unsupported source reference; value withheld"
    );
    match parts.as_slice() {
        [name] => Ok(format!(
            "${{{}.{}.{name}}}",
            manifest.project, manifest.source_config
        )),
        [config, name] => Ok(format!("${{{}.{config}.{name}}}", manifest.project)),
        [_, _, _] => Ok(raw.to_owned()),
        _ => anyhow::bail!("unsupported source reference; value withheld"),
    }
}

fn source_values(client: &Client, manifest: &Manifest) -> Result<Secrets> {
    let selected = manifest
        .fields
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    let response = client.read(
        "/v3/configs/config/secrets",
        &[
            ("project", &manifest.project),
            ("config", &manifest.source_config),
            ("secrets", &selected),
            ("include_dynamic_secrets", "false"),
            ("include_managed_secrets", "false"),
        ],
    )?;
    decode(
        field(&response, "secrets")?.clone(),
        "selected source fields",
    )
}

fn reference_targets(manifest: &Manifest, values: &Secrets) -> Result<BTreeMap<String, String>> {
    manifest
        .fields
        .iter()
        .map(|(name, source)| {
            let value = values
                .get(source)
                .context("selected source field missing")?;
            ensure!(
                value
                    .computed
                    .as_deref()
                    .is_some_and(|s| !s.is_empty() && !s.contains("${")),
                "source field is empty or unresolved; value withheld"
            );
            let raw = value
                .raw
                .as_deref()
                .context("unreadable source reference; value withheld")?;
            Ok((name.clone(), reference_target(manifest, source, raw)?))
        })
        .collect()
}

fn prepare_config(client: &Client, manifest: &Manifest) -> Result<()> {
    if manifest.config == manifest.source_config {
        return Ok(());
    }
    let values = source_values(client, manifest)?;
    let references = reference_targets(manifest, &values)?;
    let mut existing = configs(client, manifest)?;
    if !existing.iter().any(|c| c.name == manifest.config) {
        let environment = manifest
            .config
            .split('_')
            .next()
            .context("sync config environment missing")?;
        if !existing.iter().any(|c| c.environment == environment) {
            client.write("/v3/environments", &[("project", &manifest.project)], &json!({"slug":environment,"name":"GitHub synchronization","personal_configs":false}))?;
        }
        existing = configs(client, manifest)?;
        if !existing.iter().any(|c| c.name == manifest.config) {
            client.write("/v3/configs", &[], &json!({"project":manifest.project,"environment":environment,"name":manifest.config}))?;
        }
    }
    let present = names(client, &manifest.project, &manifest.config)?;
    ensure!(
        present.is_subset(&manifest.names()),
        "sync config contains unrelated fields"
    );
    if !present.is_empty() {
        let values = super::secrets(client, &manifest.project, &manifest.config)?;
        for (name, value) in values {
            let expected = references.get(&name).context("unrelated sync field")?;
            ensure!(
                value.raw.as_deref() == Some(expected),
                "sync field conflicts; value withheld"
            );
        }
    }
    client.write(
        "/v3/configs/config/secrets",
        &[("project", &manifest.project), ("config", &manifest.config)],
        &reference_update(&references),
    )?;
    ensure!(
        names(client, &manifest.project, &manifest.config)? == manifest.names(),
        "sync config field readback differs"
    );
    let verified = super::secrets(client, &manifest.project, &manifest.config)?;
    for (name, source) in &manifest.fields {
        let value = verified
            .get(name)
            .context("sync reference missing after preparation")?;
        let expected = references.get(name).context("selected reference missing")?;
        ensure!(
            value.raw.as_deref() == Some(expected)
                && value.computed
                    == values
                        .get(source)
                        .context("selected source field missing")?
                        .computed,
            "sync reference is empty, unresolved, or changed; values withheld"
        );
    }
    Ok(())
}

fn prepare(client: &mut Client, manifest: &Manifest, environment: &Value) -> Result<()> {
    ensure!(
        decide(
            Mode::Apply,
            Change::Prepare,
            Evidence {
                scope: Scope {
                    workplace: true,
                    repository: true
                },
                capacity: true,
                cutover: Cutover {
                    destination: false,
                    consumer: false
                },
                unused: false,
            },
        ) == Verdict::Allowed,
        "sync preparation refused"
    );
    client.writable = true;
    prepare_config(client, manifest)?;
    ensure!(
        github_scope(manifest)? == *environment,
        "environment protection changed during preparation"
    );
    source_fields(client, manifest)?;
    let fresh = integrations(client)?;
    let (connection, existing) = selected(manifest, &fresh)?;
    destination_options(client, manifest, connection)?;
    destination_fields(manifest, existing.is_some())?;
    if existing.is_none() {
        let repository = manifest
            .repository
            .split_once('/')
            .context("repository missing")?
            .1;
        client.write(SYNC_ENDPOINT, &[("project", &manifest.project), ("config", &manifest.config)], &json!({"integration":connection.slug,"import_option":"none","await_initial_sync":true,"data":{"feature":"actions","sync_target":"repo","repo_name":repository,"environment_name":manifest.environment,"sync_unmasked_as_variables":false}}))?;
    }
    Ok(())
}

/// Plans, prepares, or verifies one protected environment's native sync.
///
/// # Errors
/// Refuses incompatible fields, changed scope, competing writers, or failed delivery.
pub fn run(request: &Request) -> Result<()> {
    ensure!(
        request.phase != Phase::Prepare || request.apply,
        "prepare requires --apply"
    );
    ensure!(
        request.phase == Phase::Prepare || !request.apply,
        "read-only phases refuse --apply"
    );
    ensure!(
        std::fs::metadata(&request.manifest)?.len() <= MANIFEST_LIMIT,
        "sync manifest too large"
    );
    let manifest: Manifest = toml::from_str(&std::fs::read_to_string(&request.manifest)?)?;
    manifest.validate()?;
    let mut client = Client::open(&request.config_dir, &request.scope)?;
    let workplace = client.read("/v3/workplace", &[])?;
    ensure!(
        field(field(&workplace, "workplace")?, "id")?.as_str() == Some(&manifest.workplace),
        "workplace identity differs"
    );
    let environment = github_scope(&manifest)?;
    source_fields(&client, &manifest)?;
    let connections = integrations(&client)?;
    let (connection, current) = selected(&manifest, &connections)?;
    destination_options(&client, &manifest, connection)?;
    destination_fields(&manifest, current.is_some())?;
    println!(
        "Selected {}/{} -> {} environment {}; {} selected fields; {} existing syncs",
        manifest.project,
        manifest.config,
        manifest.repository,
        manifest.environment,
        manifest.fields.len(),
        connections.iter().map(|i| i.syncs.len()).sum::<usize>()
    );
    if request.phase == Phase::Plan {
        return Ok(());
    }
    if request.phase == Phase::Prepare {
        prepare(&mut client, &manifest, &environment)?;
    }
    let verified = integrations(&client)?;
    let (actual, sync) = selected(&manifest, &verified)?;
    ensure!(
        actual.slug == connection.slug && sync.is_some(),
        "selected sync missing after preparation"
    );
    ensure!(
        github_scope(&manifest)? == environment,
        "environment protection changed during synchronization"
    );
    ensure!(
        names(&client, &manifest.project, &manifest.config)? == manifest.names(),
        "sync source fields differ"
    );
    let destination = gh(&format!(
        "repos/{}/environments/{}/secrets?per_page=100",
        manifest.repository, manifest.environment
    ))?;
    let fields: BTreeSet<_> = field(&destination, "secrets")?
        .as_array()
        .context("secret metadata missing")?
        .iter()
        .filter_map(|row| row.get("name").and_then(Value::as_str))
        .collect();
    ensure!(
        manifest
            .fields
            .keys()
            .all(|name| fields.contains(name.as_str())),
        "synced destination fields missing"
    );
    println!(
        "Verified native synchronization; consumer authentication and identity retirement remain separate"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Secret;
    use super::*;

    #[test]
    fn github_reserved_names_are_rejected() {
        for name in [
            "GITHUB_APP_PRIVATE_KEY",
            "DOPPLER_TOKEN",
            "1KEY",
            "lowercase",
            "KEY-NAME",
        ] {
            assert!(!secret_name(name), "{name}");
        }
        assert!(secret_name("RELEASE_APP_PRIVATE_KEY"));
    }

    #[test]
    fn target_components_cannot_escape_the_selected_api_path() {
        for name in [
            "",
            ".",
            "..",
            "release/other",
            "release?other=1",
            "release\n",
        ] {
            assert!(!component(name), "{name}");
        }
        assert!(component("release-automation"));
    }

    fn manifest() -> Manifest {
        Manifest {
            workplace: "workplace".into(),
            repository: "owner/repository".into(),
            repository_id: 1,
            owner_id: 2,
            environment: "release".into(),
            branches: BTreeSet::from(["main".into()]),
            tags: BTreeSet::new(),
            project: "repository".into(),
            source_config: "automation".into(),
            config: "github_automation".into(),
            fields: BTreeMap::from([(
                "RELEASE_APP_PRIVATE_KEY".into(),
                "GITHUB_APP_PRIVATE_KEY".into(),
            )]),
        }
    }

    #[test]
    fn projection_updates_create_only_masked_selected_references() {
        let references = BTreeMap::from([(
            "RELEASE_APP_PRIVATE_KEY".into(),
            "${repository.automation.GITHUB_APP_PRIVATE_KEY}".into(),
        )]);
        let update = reference_update(&references);
        assert!(update.get("secrets").is_none());
        let changes = update["change_requests"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["name"], "RELEASE_APP_PRIVATE_KEY");
        assert_eq!(changes[0]["visibility"], "masked");
        assert!(changes[0]["originalName"].is_null());
        assert_eq!(
            changes[0]["value"],
            "${repository.automation.GITHUB_APP_PRIVATE_KEY}"
        );
    }

    #[test]
    fn projection_reuses_the_canonical_reference_instead_of_nesting_references() {
        let manifest = manifest();
        assert_eq!(
            reference_target(&manifest, "APP_KEY", "${shared-app.prd.KEY}").unwrap(),
            "${shared-app.prd.KEY}"
        );
        assert_eq!(
            reference_target(&manifest, "APP_KEY", "${prd.KEY}").unwrap(),
            "${repository.prd.KEY}"
        );
        assert_eq!(
            reference_target(&manifest, "APP_KEY", "${KEY}").unwrap(),
            "${repository.automation.KEY}"
        );
        assert_eq!(
            reference_target(&manifest, "APP_KEY", "fixture literal").unwrap(),
            "${repository.automation.APP_KEY}"
        );
        for raw in [
            "prefix-${KEY}",
            "${KEY}suffix",
            "${KEY}${OTHER}",
            "${}",
            "${a..KEY}",
        ] {
            assert!(reference_target(&manifest, "APP_KEY", raw).is_err());
        }
    }

    #[test]
    fn projection_refuses_missing_unreadable_empty_or_unresolved_sources() {
        let manifest = manifest();
        let mut values = Secrets::new();
        assert!(reference_targets(&manifest, &values).is_err());
        let source = "GITHUB_APP_PRIVATE_KEY".to_owned();
        let value = Secret {
            raw: Some("${shared-app.prd.KEY}".into()),
            computed: Some("fixture-key".into()),
            raw_visibility: "masked".into(),
            computed_visibility: "masked".into(),
            raw_value_type: json!({"type":"string"}),
            computed_value_type: json!({"type":"string"}),
            note: String::new(),
        };
        values.insert(source.clone(), value.clone());
        assert_eq!(
            reference_targets(&manifest, &values).unwrap()["RELEASE_APP_PRIVATE_KEY"],
            "${shared-app.prd.KEY}"
        );
        for computed in [None, Some(String::new()), Some("${missing}".into())] {
            values.get_mut(&source).unwrap().computed = computed;
            assert!(reference_targets(&manifest, &values).is_err());
        }
        values.insert(source.clone(), value);
        values.get_mut(&source).unwrap().raw = None;
        assert!(reference_targets(&manifest, &values).is_err());
    }

    fn connection() -> Integration {
        Integration {
            slug: "connection".into(),
            name: "owner".into(),
            kind: "github-actions".into(),
            enabled: true,
            syncs: Vec::new(),
        }
    }

    fn sync(manifest: &Manifest) -> Sync {
        Sync {
            slug: "sync".into(),
            project: manifest.project.clone(),
            config: manifest.config.clone(),
            description: manifest.description(),
            enabled: true,
            status: "synced".into(),
        }
    }

    #[test]
    fn destination_writers_cannot_be_replaced_or_duplicated() {
        let manifest = manifest();
        let mut integration = connection();
        integration.syncs.push(sync(&manifest));
        let (selected_connection, writer) =
            selected(&manifest, std::slice::from_ref(&integration)).unwrap();
        assert_eq!(selected_connection.slug, "connection");
        assert_eq!(writer.unwrap().slug, "sync");
        integration.syncs[0].config = "another_source".into();
        assert!(selected(&manifest, std::slice::from_ref(&integration)).is_err());
        integration.syncs[0].config.clone_from(&manifest.config);
        integration.syncs.push(sync(&manifest));
        assert!(selected(&manifest, &[integration]).is_err());
    }

    #[test]
    fn capacity_refuses_a_new_sync_but_allows_verifying_an_existing_writer() {
        let manifest = manifest();
        let mut integration = connection();
        for index in 0..SYNC_LIMIT {
            let mut unrelated = sync(&manifest);
            unrelated.description = format!("Actions: owner / repository-{index} / release");
            integration.syncs.push(unrelated);
        }
        assert!(selected(&manifest, std::slice::from_ref(&integration)).is_err());
        integration.syncs[0] = sync(&manifest);
        assert!(selected(&manifest, &[integration]).is_ok());
    }

    #[test]
    fn disabled_writers_and_ambiguous_connections_are_refused() {
        let manifest = manifest();
        let mut integration = connection();
        let mut writer = sync(&manifest);
        writer.enabled = false;
        integration.syncs.push(writer);
        assert!(selected(&manifest, &[integration]).is_err());
        assert!(selected(&manifest, &[connection(), connection()]).is_err());
    }

    #[test]
    fn unrestricted_refs_and_in_place_renames_are_refused() {
        let mut manifest = manifest();
        assert!(manifest.validate().is_ok());
        manifest.branches.clear();
        assert!(manifest.validate().is_err());
        manifest.branches.insert("main".into());
        manifest.branches.insert("*".into());
        assert!(manifest.validate().is_err());
        manifest.branches.remove("*");
        manifest.config.clone_from(&manifest.source_config);
        assert!(manifest.validate().is_err());
    }

    fn options() -> Value {
        json!({"options":{"ownerType":"User","hasMoreRepos":false,"repos":{"owner":[{
            "id":1,"name":"repository","archived":false,
            "owner":{"id":2,"login":"owner","type":"User"}
        }]}}})
    }

    #[test]
    fn connection_options_bind_repository_and_owner_ids_instead_of_display_names() {
        let manifest = manifest();
        let mut options = options();
        assert!(validate_destination_options(&manifest, &options).is_ok());
        options["options"]["repos"]["owner"][0]["owner"]["id"] = json!(3);
        assert!(validate_destination_options(&manifest, &options).is_err());
        options["options"]["repos"]["owner"][0]["owner"]["id"] = json!(2);
        options["options"]["repos"]["owner"][0]["id"] = json!(3);
        assert!(validate_destination_options(&manifest, &options).is_err());
    }

    #[test]
    fn missing_archived_and_ambiguous_repository_options_are_refused() {
        let manifest = manifest();
        let mut options = options();
        options["options"]["repos"]["owner"][0]["archived"] = json!(true);
        assert!(validate_destination_options(&manifest, &options).is_err());
        options["options"]["repos"]["owner"][0]["archived"] = json!(false);
        let duplicate = options["options"]["repos"]["owner"][0].clone();
        options["options"]["repos"]["owner"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(validate_destination_options(&manifest, &options).is_err());
        options["options"]["repos"]["owner"] = json!([]);
        assert!(validate_destination_options(&manifest, &options).is_err());
    }
}
