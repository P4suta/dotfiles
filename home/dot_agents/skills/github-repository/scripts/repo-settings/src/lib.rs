use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};

mod merge_queue_rules;
mod queue;

const BASELINE: &str = include_str!("../../../assets/baseline.json");
/// A repository's parts: `OWNER/REPO`.
const REPOSITORY_PARTS: usize = 2;
/// The largest page of the GitHub API, and the pages a listing may take before the tool gives up.
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 1000;
const GITHUB_DIRECTORY: &str = ".github";
const WORKFLOW_DIRECTORY: &str = ".github/workflows";
const CONTENTS_DIRECTORY_LIMIT: usize = 1000;
const REPOSITORY_KEYS: &[&str] = &[
    "allow_squash_merge",
    "allow_merge_commit",
    "allow_rebase_merge",
    "delete_branch_on_merge",
    "allow_auto_merge",
    "allow_update_branch",
    "squash_merge_commit_title",
    "squash_merge_commit_message",
    "web_commit_signoff_required",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub repository: BTreeMap<String, Value>,
    pub immutable_releases: bool,
    pub protected_default_branch: bool,
    pub protected_release_tags: bool,
    pub environment_admin_bypass: bool,
    pub security: SecurityProfile,
}

/// Secret scanning, push protection, and the Actions supply chain settings, none of which a profile may turn off.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SecurityProfile {
    pub secret_scanning: bool,
    pub push_protection: bool,
    pub sha_pinning_required: bool,
    pub selected_actions: bool,
}

const ENABLED_SCANNING: &str = r#"{"secret_scanning":{"status":"enabled"},"secret_scanning_push_protection":{"status":"enabled"}}"#;

impl Profile {
    pub fn baseline() -> Result<Self> {
        serde_json::from_str(BASELINE).context("parse bundled baseline")
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported profile schema");
        ensure!(
            self.immutable_releases,
            "immutable releases cannot be disabled"
        );
        ensure!(
            self.protected_default_branch && self.protected_release_tags,
            "branch and tag protection cannot be disabled"
        );
        ensure!(
            !self.environment_admin_bypass,
            "environment bypass cannot be enabled"
        );
        let security = &self.security;
        ensure!(
            security.secret_scanning
                && security.push_protection
                && security.sha_pinning_required
                && security.selected_actions,
            "secret scanning, push protection, SHA pinning, and selected actions cannot be disabled"
        );
        for (key, value) in &self.repository {
            ensure!(
                REPOSITORY_KEYS.contains(&key.as_str()),
                "unsupported repository setting: {key}"
            );
            let valid = match key.as_str() {
                "allow_merge_commit" | "allow_rebase_merge" => value == &json!(false),
                "squash_merge_commit_title" => value == &json!("PR_TITLE"),
                "squash_merge_commit_message" => {
                    matches!(value.as_str(), Some("BLANK" | "PR_BODY"))
                }
                _ => value == &json!(true),
            };
            ensure!(valid, "profile weakens or misconfigures setting: {key}");
        }
        ensure!(
            self.repository.len() == REPOSITORY_KEYS.len(),
            "profile must specify every managed setting"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum OwnerType {
    #[default]
    User,
    Organization,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub id: u64,
    pub full_name: String,
    pub owner_id: u64,
    #[serde(default)]
    pub owner_type: OwnerType,
    pub default_branch: String,
    pub archived: bool,
    pub fork: bool,
    pub admin: bool,
    pub settings: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub name: String,
    pub definition: Value,
    pub branch_policies: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub path: String,
    pub yaml: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub repository: Repository,
    pub immutable: Value,
    pub rulesets: Vec<Value>,
    pub environments: Vec<Environment>,
    pub workflow_permissions: Value,
    #[serde(default)]
    pub workflows: Vec<Workflow>,
    /// The repository's `security_and_analysis` object, which GitHub reports only to administrators.
    #[serde(default)]
    pub security: Value,
    /// `GET /repos/{repo}/actions/permissions`.
    #[serde(default)]
    pub actions: Value,
    /// The common repository checker result for the remote default branch.
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub repositories: Vec<State>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    #[serde(default)]
    pub scope: Scope,
    pub profile: Profile,
    pub snapshot: Snapshot,
    pub changes: Vec<Change>,
    pub findings: Vec<Finding>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    #[default]
    Governance,
    MergeSettings,
    EnvironmentProtections,
    ReleaseImmutability,
    Security,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub repository: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub repository: String,
    pub operation: Operation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    RepositorySettings {
        before: BTreeMap<String, Value>,
        after: BTreeMap<String, Value>,
    },
    EnableImmutableReleases,
    CreateRuleset {
        definition: Value,
    },
    UpdateRuleset {
        id: u64,
        before: Value,
        after: Value,
    },
    Environment {
        name: String,
        before: Value,
        after: Value,
    },
    SecretScanning {
        before: Value,
        after: Value,
    },
    ActionsPinning {
        before: Value,
        after: Value,
    },
}

pub fn validate_repo_name(name: &str) -> Result<()> {
    let parts: Vec<_> = name.split('/').collect();
    ensure!(
        parts.len() == REPOSITORY_PARTS
            && parts.iter().all(|part| !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))),
        "expected a literal OWNER/REPO name: {name}"
    );
    Ok(())
}

pub fn plan(snapshot: Snapshot, profile: Profile) -> Result<Plan> {
    profile.validate()?;
    ensure!(snapshot.schema_version == 1, "unsupported snapshot schema");
    let mut seen = BTreeSet::new();
    let mut changes = Vec::new();
    let mut findings = Vec::new();
    for state in &snapshot.repositories {
        let repo = &state.repository;
        validate_repo_name(&repo.full_name)?;
        ensure!(
            seen.insert(repo.full_name.clone()),
            "duplicate repository: {}",
            repo.full_name
        );
        ensure!(
            repo.admin && !repo.archived && !repo.fork,
            "refusing unmanaged, archived, or fork repository: {}",
            repo.full_name
        );
        ensure!(
            repo.id > 0 && repo.owner_id > 0 && !repo.default_branch.is_empty(),
            "incomplete repository identity"
        );
        let mut before = BTreeMap::new();
        let mut after = BTreeMap::new();
        for (key, value) in &profile.repository {
            let current = repo
                .settings
                .get(key)
                .with_context(|| format!("missing setting: {key}"))?;
            if current != value {
                before.insert(key.clone(), current.clone());
                after.insert(key.clone(), value.clone());
            }
        }
        if !after.is_empty() {
            if after.contains_key("squash_merge_commit_message") {
                before.insert(
                    "squash_merge_commit_title".into(),
                    repo.settings["squash_merge_commit_title"].clone(),
                );
                after.insert(
                    "squash_merge_commit_title".into(),
                    profile.repository["squash_merge_commit_title"].clone(),
                );
            }
            changes.push(Change {
                repository: repo.full_name.clone(),
                operation: Operation::RepositorySettings { before, after },
            });
        }
        let enabled = state.immutable["enabled"]
            .as_bool()
            .context("missing immutable release status")?;
        if !enabled {
            changes.push(Change {
                repository: repo.full_name.clone(),
                operation: Operation::EnableImmutableReleases,
            });
        }
        for target in ["branch", "tag"] {
            let baseline = baseline_ruleset(target);
            if !has_protection(&state.rulesets, &baseline)? {
                changes.push(Change {
                    repository: repo.full_name.clone(),
                    operation: Operation::CreateRuleset {
                        definition: baseline,
                    },
                });
            }
        }
        queue::changes(state, &mut changes, &mut findings)?;
        if state.workflow_permissions["default_workflow_permissions"] != "read" {
            findings.push(Finding { repository: repo.full_name.clone(), message: "Default workflow token is writable; review job permissions before changing its default.".into() });
        }
        if state.metadata["current"] != true {
            findings.push(Finding {
                repository: repo.full_name.clone(),
                message: format!("The common repository metadata check failed: {}. Run `dotfiles repository generate --remote {}` and review the generated files.", state.metadata["evidence"].as_str().unwrap_or("metadata evidence is missing"), repo.full_name),
            });
        }
        security_changes(state, &profile.security, &mut changes, &mut findings);
        for env in &state.environments {
            let current = env.definition["can_admins_bypass"]
                .as_bool()
                .context("missing environment bypass setting")?;
            if current && is_release_environment(&env.name) {
                let before = environment_payload(&env.definition)?;
                let mut after = before.clone();
                after["can_admins_bypass"] = json!(false);
                changes.push(Change {
                    repository: repo.full_name.clone(),
                    operation: Operation::Environment {
                        name: env.name.clone(),
                        before,
                        after,
                    },
                });
            }
        }
    }
    Ok(Plan {
        schema_version: 1,
        scope: Scope::Governance,
        profile,
        snapshot,
        changes,
        findings,
    })
}

pub fn plan_scoped(snapshot: Snapshot, profile: Profile, scope: Scope) -> Result<Plan> {
    let mut result = plan(snapshot, profile)?;
    result.scope = scope;
    match scope {
        Scope::Governance => {}
        Scope::MergeSettings => result
            .changes
            .retain(|change| matches!(change.operation, Operation::RepositorySettings { .. })),
        Scope::EnvironmentProtections => result
            .changes
            .retain(|change| matches!(change.operation, Operation::Environment { .. })),
        Scope::ReleaseImmutability => result
            .changes
            .retain(|change| matches!(change.operation, Operation::EnableImmutableReleases)),
        Scope::Security => result.changes.retain(|change| {
            matches!(
                change.operation,
                Operation::SecretScanning { .. } | Operation::ActionsPinning { .. }
            )
        }),
    }
    Ok(result)
}

/// Plans secret scanning, push protection, and SHA pinning, and reports a repository that runs any action.
/// Only a reviewer can confirm the list of actions the workflows use, so the plan never writes one.
fn security_changes(
    state: &State,
    profile: &SecurityProfile,
    changes: &mut Vec<Change>,
    findings: &mut Vec<Finding>,
) {
    let repository = &state.repository.full_name;
    let enabled = |key: &str| state.security[key]["status"] == "enabled";
    if (profile.secret_scanning && !enabled("secret_scanning"))
        || (profile.push_protection && !enabled("secret_scanning_push_protection"))
    {
        if state.security.is_object() {
            changes.push(Change {
                repository: repository.clone(),
                operation: Operation::SecretScanning {
                    before: state.security.clone(),
                    after: serde_json::from_str(ENABLED_SCANNING).unwrap_or(Value::Null),
                },
            });
        } else {
            findings.push(Finding { repository: repository.clone(), message: "GitHub reported no secret scanning status; audit with an administrator token, or enable Advanced Security for a private repository.".into() });
        }
    }
    if profile.sha_pinning_required && state.actions["sha_pinning_required"] != true {
        let mut after = state.actions.clone();
        after["sha_pinning_required"] = json!(true);
        changes.push(Change {
            repository: repository.clone(),
            operation: Operation::ActionsPinning {
                before: state.actions.clone(),
                after,
            },
        });
    }
    if profile.selected_actions && state.actions["allowed_actions"] != "selected" {
        findings.push(Finding { repository: repository.clone(), message: "Actions run without an allow list; select the actions the workflows use and set allowed_actions to selected, since the CLI never invents the list.".into() });
    }
}

fn is_release_environment(name: &str) -> bool {
    matches!(name, "release" | "crates-io" | "pypi" | "npm" | "hex")
        || name.starts_with("release-")
        || name.contains("signing")
        || name.contains("publish")
}

fn baseline_ruleset(target: &str) -> Value {
    let (name, include, rules) = if target == "branch" {
        (
            "personal default branch",
            "~DEFAULT_BRANCH",
            json!([
                {"type":"deletion"}, {"type":"non_fast_forward"},
                {"type":"required_linear_history"}, {"type":"required_signatures"},
                {"type":"pull_request","parameters":{
                    "required_approving_review_count":0,
                    "dismiss_stale_reviews_on_push":true,
                    "require_code_owner_review":false,
                    "require_last_push_approval":false,
                    "required_review_thread_resolution":true,
                    "allowed_merge_methods":["squash"]
                }}
            ]),
        )
    } else {
        (
            "personal release tags",
            "refs/tags/v*",
            json!([
                {"type":"deletion"}, {"type":"non_fast_forward"}, {"type":"update"}
            ]),
        )
    };
    json!({"name":name,"target":target,"enforcement":"active","bypass_actors":[],
        "conditions":{"ref_name":{"include":[include],"exclude":[]}},"rules":rules})
}

fn has_protection(rulesets: &[Value], baseline: &Value) -> Result<bool> {
    let mut effective = BTreeMap::new();
    for ruleset in rulesets {
        if ruleset["target"] != baseline["target"] || ruleset["enforcement"] != "active" {
            continue;
        }
        let bypass = ruleset["bypass_actors"]
            .as_array()
            .context("ruleset bypass actors unavailable; admin access is required")?;
        if !bypass.is_empty() {
            continue;
        }
        let refs = &ruleset["conditions"]["ref_name"];
        if refs["exclude"]
            .as_array()
            .is_none_or(|excluded| !excluded.is_empty())
        {
            continue;
        }
        let expected = &baseline["conditions"]["ref_name"]["include"][0];
        let covers = refs["include"].as_array().is_some_and(|includes| {
            includes.contains(expected) || includes.contains(&json!("~ALL"))
        });
        if !covers {
            continue;
        }
        for rule in ruleset["rules"]
            .as_array()
            .context("missing ruleset rules")?
        {
            if let Some(kind) = rule["type"].as_str() {
                effective
                    .entry(kind.to_owned())
                    .or_insert_with(Vec::new)
                    .push(rule);
            }
        }
    }
    for expected in baseline["rules"]
        .as_array()
        .context("invalid baseline rules")?
    {
        let kind = expected["type"].as_str().context("invalid rule type")?;
        let Some(actual) = effective.get(kind) else {
            return Ok(false);
        };
        if kind == "pull_request"
            && !actual.iter().any(|rule| {
                rule["parameters"]["required_review_thread_resolution"] == true
                    && rule["parameters"]["dismiss_stale_reviews_on_push"] == true
            })
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn environment_payload(definition: &Value) -> Result<Value> {
    let mut payload = Map::new();
    payload.insert(
        "can_admins_bypass".into(),
        definition["can_admins_bypass"].clone(),
    );
    payload.insert(
        "deployment_branch_policy".into(),
        definition["deployment_branch_policy"].clone(),
    );
    for rule in definition["protection_rules"]
        .as_array()
        .context("missing environment protection rules")?
    {
        match rule["type"].as_str() {
            Some("required_reviewers") => {
                payload.insert(
                    "prevent_self_review".into(),
                    rule["prevent_self_review"].clone(),
                );
                let reviewers = rule["reviewers"]
                    .as_array()
                    .context("missing reviewers")?
                    .iter()
                    .map(|entry| json!({"type":entry["type"],"id":entry["reviewer"]["id"]}))
                    .collect();
                payload.insert("reviewers".into(), Value::Array(reviewers));
            }
            Some("wait_timer") => {
                payload.insert("wait_timer".into(), rule["wait_timer"].clone());
            }
            Some("branch_policy") => {}
            _ => {
                bail!("custom environment protections need manual review before changing settings")
            }
        }
    }
    Ok(Value::Object(payload))
}

/// The complete set of remote effects supported by this tool.
/// Release publication, ref updates, credential reads, and protection deletion have no variant.
pub enum Mutation<'a> {
    RepositorySettings {
        repository: &'a str,
        settings: &'a BTreeMap<String, Value>,
    },
    EnableImmutableReleases {
        repository: &'a str,
    },
    CreateRuleset {
        repository: &'a str,
        definition: &'a Value,
    },
    UpdateRuleset {
        repository: &'a str,
        id: u64,
        before: &'a Value,
        after: &'a Value,
    },
    ProtectEnvironment {
        repository: &'a str,
        name: &'a str,
        previous: &'a Value,
    },
    SecretScanning {
        repository: &'a str,
    },
    ActionsPinning {
        repository: &'a str,
        previous: &'a Value,
    },
}

enum WriteMethod {
    Patch,
    Put,
    Post,
}

impl Mutation<'_> {
    fn request(self) -> Result<(WriteMethod, String, Option<Value>)> {
        let repository = match &self {
            Self::RepositorySettings { repository, .. }
            | Self::EnableImmutableReleases { repository }
            | Self::CreateRuleset { repository, .. }
            | Self::UpdateRuleset { repository, .. }
            | Self::ProtectEnvironment { repository, .. }
            | Self::SecretScanning { repository }
            | Self::ActionsPinning { repository, .. } => *repository,
        };
        validate_repo_name(repository)?;
        let root = format!("repos/{repository}");
        Ok(match self {
            Self::RepositorySettings { settings, .. } => {
                ensure!(
                    settings
                        .keys()
                        .all(|key| REPOSITORY_KEYS.contains(&key.as_str())),
                    "unmanaged repository setting"
                );
                (
                    WriteMethod::Patch,
                    root,
                    Some(serde_json::to_value(settings)?),
                )
            }
            Self::EnableImmutableReleases { .. } => {
                (WriteMethod::Put, format!("{root}/immutable-releases"), None)
            }
            Self::CreateRuleset { definition, .. } => {
                let expected = baseline_ruleset(
                    definition["target"]
                        .as_str()
                        .context("missing ruleset target")?,
                );
                ensure!(
                    *definition == expected,
                    "only additive baseline rulesets can be created"
                );
                (
                    WriteMethod::Post,
                    format!("{root}/rulesets"),
                    Some(expected),
                )
            }
            Self::UpdateRuleset {
                id, before, after, ..
            } => {
                ensure!(id > 0 && before["id"] == id, "ruleset identity mismatch");
                ensure!(
                    before["target"] == "branch" && before["enforcement"] == "active",
                    "queue requires an active branch ruleset"
                );
                let add_queue = after["rules"].as_array().context("missing rules")?.len()
                    > before["rules"].as_array().context("missing rules")?.len();
                ensure!(
                    queue::queued_ruleset(before, add_queue)? == *after,
                    "only the queue and strict status policy may change"
                );
                (
                    WriteMethod::Put,
                    format!("{root}/rulesets/{id}"),
                    Some(queue::payload(after)?),
                )
            }
            Self::ProtectEnvironment { name, previous, .. } => {
                let mut payload = previous.clone();
                payload["can_admins_bypass"] = json!(false);
                (
                    WriteMethod::Put,
                    format!("{root}/environments/{}", encode_segment(name)),
                    Some(payload),
                )
            }
            Self::SecretScanning { .. } => (
                WriteMethod::Patch,
                root,
                Some(
                    json!({"security_and_analysis": serde_json::from_str::<Value>(ENABLED_SCANNING)?}),
                ),
            ),
            Self::ActionsPinning { previous, .. } => {
                let enabled = previous["enabled"]
                    .as_bool()
                    .context("missing Actions enabled status")?;
                let allowed = previous["allowed_actions"]
                    .as_str()
                    .context("missing allowed actions policy")?;
                ensure!(
                    matches!(allowed, "all" | "local_only" | "selected"),
                    "unknown allowed actions policy: {allowed}"
                );
                (
                    WriteMethod::Put,
                    format!("{root}/actions/permissions"),
                    Some(
                        json!({"enabled": enabled, "allowed_actions": allowed, "sha_pinning_required": true}),
                    ),
                )
            }
        })
    }
}

pub trait Api {
    fn read(&self, endpoint: &str) -> Result<Value>;
    fn write(&self, mutation: Mutation<'_>) -> Result<Value>;
    fn metadata(&self, name: &str) -> Result<Value>;
}

pub struct Gh;

impl Api for Gh {
    fn metadata(&self, name: &str) -> Result<Value> {
        validate_repo_name(name)?;
        let directory = tempfile::tempdir()?;
        let checkout = directory.path().join("repository");
        let cloned = process("gh")
            .args(["repo", "clone", name])
            .arg(&checkout)
            .args(["--", "--filter=blob:none"])
            .output()?;
        ensure!(
            cloned.status.success(),
            "could not inspect {name}: {}",
            String::from_utf8_lossy(&cloned.stderr)
        );
        let checked = process("dotfiles")
            .arg("--root")
            .arg(&checkout)
            .args(["repository", "check", "--remote", name])
            .output()?;
        Ok(
            json!({"current": checked.status.success(), "evidence": String::from_utf8_lossy(&checked.stderr)}),
        )
    }
    fn read(&self, endpoint: &str) -> Result<Value> {
        gh_request("GET", endpoint, None)
    }

    fn write(&self, mutation: Mutation<'_>) -> Result<Value> {
        let (method, endpoint, body) = mutation.request()?;
        let method = match method {
            WriteMethod::Patch => "PATCH",
            WriteMethod::Put => "PUT",
            WriteMethod::Post => "POST",
        };
        gh_request(method, &endpoint, body.as_ref())
    }
}

fn gh_request(method: &str, endpoint: &str, body: Option<&Value>) -> Result<Value> {
    let mut command = process("gh");
    command.args([
        "api",
        "--hostname",
        "github.com",
        "--method",
        method,
        "--header",
        "Accept: application/vnd.github+json",
        "--header",
        "X-GitHub-Api-Version: 2026-03-10",
        endpoint,
    ]);
    command.env_remove("GH_DEBUG");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if body.is_some() {
        command.args(["--input", "-"]).stdin(Stdio::piped());
    }
    let mut child = command.spawn().context("start gh api")?;
    if let Some(body) = body {
        let mut input = child.stdin.take().context("missing gh stdin")?;
        serde_json::to_writer(&mut input, body).context("write API request")?;
        input.flush().context("flush API request")?;
    }
    let output = child.wait_with_output().context("wait for gh api")?;
    if !output.status.success() {
        bail!(
            "{method} {endpoint}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if output.stdout.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&output.stdout).with_context(|| format!("decode {method} {endpoint}"))
}

fn string(value: &Value, field: &str) -> Result<String> {
    value[field]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("missing string {field}"))
}

fn number(value: &Value, field: &str) -> Result<u64> {
    value[field]
        .as_u64()
        .with_context(|| format!("missing numeric {field}"))
}

pub fn audit(api: &impl Api, name: &str) -> Result<State> {
    validate_repo_name(name)?;
    let root = format!("repos/{name}");
    let raw = api.read(&root)?;
    let full_name = string(&raw, "full_name")?;
    ensure!(
        full_name.eq_ignore_ascii_case(name),
        "repository was renamed or redirected: {name} -> {full_name}"
    );
    let settings = REPOSITORY_KEYS
        .iter()
        .map(|key| {
            raw.get(*key)
                .cloned()
                .map(|value| ((*key).to_owned(), value))
                .with_context(|| format!("missing setting {key}"))
        })
        .collect::<Result<_>>()?;
    let repository = Repository {
        id: number(&raw, "id")?,
        full_name,
        owner_id: number(&raw["owner"], "id")?,
        owner_type: serde_json::from_value(raw["owner"]["type"].clone())
            .context("invalid repository owner type")?,
        default_branch: string(&raw, "default_branch")?,
        archived: raw["archived"]
            .as_bool()
            .context("missing archive status")?,
        fork: raw["fork"].as_bool().context("missing fork status")?,
        admin: raw["permissions"]["admin"].as_bool().unwrap_or(false),
        settings,
    };
    let immutable = api.read(&format!("{root}/immutable-releases"))?;
    let summaries = paged_array(api, &format!("{root}/rulesets"), None)?;
    let mut rulesets = Vec::new();
    for summary in summaries {
        let id = number(&summary, "id")?;
        rulesets.push(api.read(&format!("{root}/rulesets/{id}"))?);
    }
    rulesets.sort_by_key(|rule| rule["id"].as_u64().unwrap_or(0));
    let definitions = paged_array(api, &format!("{root}/environments"), Some("environments"))?;
    let mut environments = Vec::new();
    for definition in definitions {
        let name = string(&definition, "name")?;
        let policies = if definition["deployment_branch_policy"]["custom_branch_policies"] == true {
            paged_array(
                api,
                &format!(
                    "{root}/environments/{}/deployment-branch-policies",
                    encode_segment(&name)
                ),
                Some("branch_policies"),
            )?
        } else {
            Vec::new()
        };
        environments.push(Environment {
            name,
            definition,
            branch_policies: policies,
        });
    }
    environments.sort_by(|left, right| left.name.cmp(&right.name));
    let workflow_permissions = api.read(&format!("{root}/actions/permissions/workflow"))?;
    let actions = api.read(&format!("{root}/actions/permissions"))?;
    let workflows = audit_workflows(api, &root, &repository.default_branch)?;
    Ok(State {
        metadata: api.metadata(name)?,
        repository,
        immutable,
        rulesets,
        environments,
        workflow_permissions,
        workflows,
        security: raw["security_and_analysis"].clone(),
        actions,
    })
}

fn contents(api: &impl Api, root: &str, path: &str, branch: &str) -> Result<Vec<Value>> {
    let endpoint = if path.is_empty() {
        format!("{root}/contents")
    } else {
        format!("{root}/contents/{path}")
    };
    let listing = api.read(&format!("{endpoint}?ref={}", encode_segment(branch)))?;
    let entries = listing.as_array().context("expected contents directory")?;
    ensure!(
        entries.len() < CONTENTS_DIRECTORY_LIMIT,
        "contents listing may be truncated: {path}"
    );
    Ok(entries.clone())
}

fn audit_workflows(api: &impl Api, root: &str, branch: &str) -> Result<Vec<Workflow>> {
    let mut parent = "";
    for path in [GITHUB_DIRECTORY, WORKFLOW_DIRECTORY] {
        if !contents(api, root, parent, branch)?
            .iter()
            .any(|entry| entry["path"] == path && entry["type"] == "dir")
        {
            return Ok(Vec::new());
        }
        parent = path;
    }
    let mut workflows = Vec::new();
    for entry in contents(api, root, WORKFLOW_DIRECTORY, branch)? {
        let path = string(&entry, "path")?;
        if entry["type"] != "file" || !(path.ends_with(".yml") || path.ends_with(".yaml")) {
            continue;
        }
        ensure!(
            path.strip_prefix(WORKFLOW_DIRECTORY)
                .is_some_and(|suffix| suffix.starts_with('/') && !suffix[1..].contains('/')),
            "unexpected workflow path: {path}"
        );
        let encoded_path = path
            .split('/')
            .map(encode_segment)
            .collect::<Vec<_>>()
            .join("/");
        let file = api.read(&format!(
            "{root}/contents/{encoded_path}?ref={}",
            encode_segment(branch)
        ))?;
        ensure!(
            file["encoding"] == "base64",
            "workflow contents unavailable: {path}"
        );
        let content = string(&file, "content")?;
        let bytes = STANDARD
            .decode(content.split_whitespace().collect::<String>())
            .with_context(|| format!("decode workflow {path}"))?;
        workflows.push(Workflow {
            path,
            yaml: String::from_utf8(bytes).context("workflow must be UTF-8")?,
        });
    }
    workflows.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(workflows)
}

#[expect(
    clippy::disallowed_methods,
    reason = "the standalone governance tool's one native process constructor"
)]
fn process(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env_remove("GH_DEBUG");
    command
}

pub fn paged_array(api: &impl Api, endpoint: &str, key: Option<&str>) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    for page in 1..=MAX_PAGES {
        let separator = if endpoint.contains('?') { '&' } else { '?' };
        let response = api.read(&format!(
            "{endpoint}{separator}per_page={PAGE_SIZE}&page={page}"
        ))?;
        let array = key
            .map_or(&response, |key| &response[key])
            .as_array()
            .context("expected paginated API array")?;
        values.extend(array.iter().cloned());
        if array.len() < PAGE_SIZE {
            return Ok(values);
        }
    }
    bail!("pagination did not finish: {endpoint}")
}

pub fn encode_segment(segment: &str) -> String {
    segment
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

pub fn discover(api: &impl Api, owner: &str, include_private: bool) -> Result<Vec<String>> {
    validate_repo_name(&format!("{owner}/placeholder"))?;
    let candidates = paged_array(api, "user/repos?affiliation=owner&sort=full_name", None)?;
    candidates
        .iter()
        .filter(|repo| {
            repo["owner"]["login"]
                .as_str()
                .is_some_and(|login| login.eq_ignore_ascii_case(owner))
                && repo["archived"] == false
                && repo["fork"] == false
                && (include_private || repo["private"] == false)
        })
        .map(|repo| string(repo, "full_name"))
        .collect()
}

fn managed_state(state: &State) -> Result<Value> {
    Ok(
        json!({"repository":state.repository,"immutable":state.immutable,
        "rulesets":state.rulesets.iter().map(|rule| json!({
            "id":rule["id"],"target":rule["target"],"name":rule["name"],
            "enforcement":rule["enforcement"],"conditions":rule["conditions"],
            "rules":rule["rules"],"bypass_actors":rule["bypass_actors"]
        })).collect::<Vec<_>>(),
        "environments":state.environments.iter().map(|env| Ok(json!({
            "name":env.name,"definition":environment_payload(&env.definition)?,
            "branch_policies":env.branch_policies.iter().map(|policy|
                json!({"id":policy["id"],"name":policy["name"],"type":policy["type"]})).collect::<Vec<_>>()
        }))).collect::<Result<Vec<_>>>()?,
        "workflow_permissions":state.workflow_permissions,"workflows":state.workflows,
        "security":state.security,"actions":state.actions}),
    )
}

#[derive(Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ApplyEvent<'a> {
    Applying { change: &'a Change },
    Verified { change: &'a Change },
    Failed { change: &'a Change, error: String },
}

struct CheckedTarget<'a>(&'a Repository);

fn check_target<'a>(
    api: &impl Api,
    previous: &'a Repository,
) -> Result<(CheckedTarget<'a>, Value)> {
    let current = api.read(&format!("repos/{}", previous.full_name))?;
    ensure!(
        current["id"] == previous.id
            && current["owner"]["id"] == previous.owner_id
            && current["default_branch"] == previous.default_branch
            && current["archived"] == false
            && current["fork"] == false
            && current["permissions"]["admin"] == true
            && current["full_name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(&previous.full_name)),
        "repository identity or scope changed before write: {}",
        previous.full_name
    );
    Ok((CheckedTarget(previous), current))
}

impl CheckedTarget<'_> {
    fn execute(self, api: &impl Api, change: &Change, expected: &mut State) -> Result<()> {
        let name = self.0.full_name.as_str();
        let root = format!("repos/{name}");
        match &change.operation {
            Operation::RepositorySettings { after, .. } => {
                api.write(Mutation::RepositorySettings {
                    repository: name,
                    settings: after,
                })?;
                let current = api.read(&root)?;
                ensure!(
                    after
                        .iter()
                        .all(|(key, value)| current.get(key) == Some(value)),
                    "repository setting verification failed: {name}"
                );
                expected.repository.settings.extend(after.clone());
            }
            Operation::EnableImmutableReleases => {
                api.write(Mutation::EnableImmutableReleases { repository: name })?;
                let current = api.read(&format!("{root}/immutable-releases"))?;
                ensure!(
                    current["enabled"] == true,
                    "immutable releases were not enabled: {name}"
                );
                expected.immutable = current;
            }
            Operation::CreateRuleset { definition } => {
                let created = api.write(Mutation::CreateRuleset {
                    repository: name,
                    definition,
                })?;
                let id = number(&created, "id")?;
                let current = api.read(&format!("{root}/rulesets/{id}"))?;
                ensure!(
                    has_protection(std::slice::from_ref(&current), definition)?,
                    "new ruleset verification failed: {name}"
                );
                expected.rulesets.push(current);
                expected
                    .rulesets
                    .sort_by_key(|rule| rule["id"].as_u64().unwrap_or(0));
            }
            Operation::UpdateRuleset { id, before, after } => {
                api.write(Mutation::UpdateRuleset {
                    repository: name,
                    id: *id,
                    before,
                    after,
                })?;
                let current = api.read(&format!("{root}/rulesets/{id}"))?;
                ensure!(
                    current["id"] == *id && queue::payload(&current)? == queue::payload(after)?,
                    "ruleset update verification failed: {name}/{id}"
                );
                *expected
                    .rulesets
                    .iter_mut()
                    .find(|ruleset| ruleset["id"] == *id)
                    .context("ruleset missing from snapshot")? = current;
            }
            Operation::Environment {
                name: environment_name,
                before,
                after,
            } => {
                api.write(Mutation::ProtectEnvironment {
                    repository: name,
                    name: environment_name,
                    previous: before,
                })?;
                let current = api.read(&format!(
                    "{root}/environments/{}",
                    encode_segment(environment_name)
                ))?;
                ensure!(
                    environment_payload(&current)? == *after,
                    "environment protection verification failed: {name}/{environment_name}"
                );
                expected
                    .environments
                    .iter_mut()
                    .find(|env| &env.name == environment_name)
                    .context("environment missing from snapshot")?
                    .definition = current;
            }
            Operation::SecretScanning { after, .. } => {
                api.write(Mutation::SecretScanning { repository: name })?;
                let current = api.read(&root)?["security_and_analysis"].clone();
                ensure!(
                    ["secret_scanning", "secret_scanning_push_protection"]
                        .iter()
                        .all(|key| current[key]["status"] == after[key]["status"]),
                    "secret scanning verification failed: {name}"
                );
                expected.security = current;
            }
            Operation::ActionsPinning { before, after } => {
                api.write(Mutation::ActionsPinning {
                    repository: name,
                    previous: before,
                })?;
                let current = api.read(&format!("{root}/actions/permissions"))?;
                ensure!(current == *after, "SHA pinning verification failed: {name}");
                expected.actions = current;
            }
        }
        Ok(())
    }
}

pub fn apply(
    api: &impl Api,
    input: &Plan,
    mut progress: impl FnMut(&ApplyEvent<'_>) -> Result<()>,
) -> Result<()> {
    ensure!(input.schema_version == 1, "unsupported plan schema");
    let expected = plan_scoped(input.snapshot.clone(), input.profile.clone(), input.scope)?;
    ensure!(
        input == &expected,
        "plan was edited; generate a new plan instead"
    );
    let names: BTreeSet<_> = input
        .changes
        .iter()
        .map(|change| change.repository.as_str())
        .collect();
    for name in &names {
        let previous = input
            .snapshot
            .repositories
            .iter()
            .find(|state| state.repository.full_name == *name)
            .context("plan repository is missing")?;
        let live = audit(api, name)?;
        ensure!(
            managed_state(previous)? == managed_state(&live)?,
            "settings changed since audit: {name}; audit and plan again"
        );
    }
    let mut expected_states: BTreeMap<_, _> = input
        .snapshot
        .repositories
        .iter()
        .map(|state| (state.repository.full_name.clone(), state.clone()))
        .collect();
    for change in &input.changes {
        let expected = expected_states
            .get_mut(&change.repository)
            .context("missing expected repository")?;
        let identity = expected.repository.clone();
        let (checked, current_repository) = check_target(api, &identity)?;
        match &change.operation {
            Operation::RepositorySettings { before, .. } => ensure!(
                before
                    .iter()
                    .all(|(key, value)| current_repository.get(key) == Some(value)),
                "repository settings changed before write: {}",
                change.repository
            ),
            Operation::Environment { name, before, .. } => {
                let endpoint = format!(
                    "repos/{}/environments/{}",
                    change.repository,
                    encode_segment(name)
                );
                let live = api.read(&endpoint)?;
                ensure!(
                    environment_payload(&live)? == *before,
                    "environment changed before write: {}/{}",
                    change.repository,
                    name
                );
                let original = expected
                    .environments
                    .iter()
                    .find(|env| &env.name == name)
                    .context("missing expected environment")?;
                if live["deployment_branch_policy"]["custom_branch_policies"] == true {
                    let policies = paged_array(
                        api,
                        &format!("{endpoint}/deployment-branch-policies"),
                        Some("branch_policies"),
                    )?;
                    ensure!(
                        policies == original.branch_policies,
                        "environment ref policies changed before write: {}/{}",
                        change.repository,
                        name
                    );
                }
            }
            Operation::UpdateRuleset { id, before, .. } => {
                let live = audit(api, &change.repository)?;
                ensure!(
                    managed_state(expected)? == managed_state(&live)?,
                    "settings or workflows changed before ruleset update: {}",
                    change.repository
                );
                let current = api.read(&format!("repos/{}/rulesets/{id}", change.repository))?;
                ensure!(
                    queue::payload(&current)? == queue::payload(before)? && current["id"] == *id,
                    "ruleset changed before write: {id}"
                );
            }
            Operation::SecretScanning { before, .. } => ensure!(
                current_repository["security_and_analysis"] == *before,
                "secret scanning settings changed before write: {}",
                change.repository
            ),
            Operation::ActionsPinning { before, .. } => ensure!(
                api.read(&format!("repos/{}/actions/permissions", change.repository))? == *before,
                "Actions permissions changed before write: {}",
                change.repository
            ),
            Operation::EnableImmutableReleases | Operation::CreateRuleset { .. } => {}
        }
        progress(&ApplyEvent::Applying { change })?;
        if let Err(error) = checked.execute(api, change, expected) {
            progress(&ApplyEvent::Failed {
                change,
                error: format!("{error:#}"),
            })?;
            return Err(error);
        }
        progress(&ApplyEvent::Verified { change })?;
    }
    for name in names {
        let live = audit(api, name)?;
        let expected = expected_states
            .get(name)
            .context("missing expected final state")?;
        ensure!(
            managed_state(expected)? == managed_state(&live)?,
            "existing protections changed during apply: {name}"
        );
        let fresh = Snapshot {
            schema_version: 1,
            repositories: vec![live],
        };
        ensure!(
            plan_scoped(fresh, input.profile.clone(), input.scope)?
                .changes
                .is_empty(),
            "repository still differs from the selected scope: {name}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Mutation, WriteMethod, queue};
    use anyhow::Result;
    use serde_json::{Value, json};

    fn live_ruleset() -> Value {
        json!({
            "id": 1, "name": "main", "target": "branch", "enforcement": "active",
            "conditions": {"ref_name": {"include": ["~DEFAULT_BRANCH"], "exclude": []}},
            "bypass_actors": [{"actor_id": 9, "actor_type": "Integration", "bypass_mode": "pull_request"}],
            "rules": [
                {"type": "deletion"},
                {"type": "required_status_checks", "parameters": {
                    "required_status_checks": [{"context": "actual-ci", "integration_id": 15368}],
                    "strict_required_status_checks_policy": true
                }}
            ],
            "created_at": "2026-10-01T00:00:00Z"
        })
    }

    #[test]
    fn queue_update_request_puts_the_complete_writable_ruleset() -> Result<()> {
        let before = live_ruleset();
        let after = queue::queued_ruleset(&before, true)?;
        let (method, endpoint, body) = Mutation::UpdateRuleset {
            repository: "owner/project",
            id: 1,
            before: &before,
            after: &after,
        }
        .request()?;
        assert!(matches!(method, WriteMethod::Put));
        assert_eq!(endpoint, "repos/owner/project/rulesets/1");
        let mut expected = after;
        let object = expected.as_object_mut().unwrap();
        object.remove("id");
        object.remove("created_at");
        assert_eq!(body, Some(expected));
        Ok(())
    }

    #[test]
    fn queue_update_request_refuses_other_changes_and_wrong_identities() -> Result<()> {
        let before = live_ruleset();
        let after = queue::queued_ruleset(&before, true)?;
        for key in [
            "name",
            "target",
            "enforcement",
            "conditions",
            "bypass_actors",
            "rules",
        ] {
            let mut changed = after.clone();
            changed[key] = Value::Null;
            assert!(
                Mutation::UpdateRuleset {
                    repository: "owner/project",
                    id: 1,
                    before: &before,
                    after: &changed,
                }
                .request()
                .is_err(),
                "unexpected change to {key}"
            );
        }
        for id in [0, 2] {
            assert!(
                Mutation::UpdateRuleset {
                    repository: "owner/project",
                    id,
                    before: &before,
                    after: &after,
                }
                .request()
                .is_err(),
                "unexpected ruleset identity {id}"
            );
        }
        Ok(())
    }
}
