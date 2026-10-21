use super::{Client, ProjectSnapshot, SecretSnapshot, Tool, decode, field};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;

const ACTIONS_PREFIX: &str = "Actions: ";
const GITHUB_TYPE: &str = "github-actions";
const GITHUB_ISSUER: &str = "https://token.actions.githubusercontent.com";

#[derive(Deserialize)]
struct Integration {
    slug: String,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    enabled: bool,
    syncs: Vec<Sync>,
}

#[derive(Deserialize)]
struct Sync {
    slug: String,
    project: String,
    config: String,
    description: String,
    enabled: bool,
    status: String,
}

struct Target {
    owner: String,
    repository: String,
    environment: Option<String>,
}

fn sync_target(description: &str) -> Result<Target> {
    let text = description
        .strip_prefix(ACTIONS_PREFIX)
        .context("only GitHub Actions syncs are supported")?;
    let parts: Vec<_> = text.split(" / ").collect();
    let (owner, repository, environment) = match parts.as_slice() {
        [owner, repository] => (*owner, *repository, None),
        [owner, repository, environment] => (*owner, *repository, Some((*environment).to_owned())),
        _ => anyhow::bail!("unsupported GitHub sync description"),
    };
    ensure!(
        [owner, repository]
            .into_iter()
            .chain(environment.as_deref())
            .all(|part| !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))),
        "invalid GitHub target component"
    );
    Ok(Target {
        owner: owner.to_owned(),
        repository: repository.to_owned(),
        environment,
    })
}

fn gh_json(endpoint: &str) -> Result<Value> {
    let output = Tool::Gh
        .command()
        .args(["api", endpoint])
        .stdin(Stdio::null())
        .output()
        .context("start GitHub metadata lookup")?;
    ensure!(
        output.status.success(),
        "GitHub metadata lookup failed; response withheld"
    );
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow::anyhow!("invalid GitHub metadata; response withheld"))?;
    if let Some(count) = value.get("total_count").and_then(Value::as_u64) {
        ensure!(
            count <= super::PAGE_SIZE as u64,
            "GitHub metadata pagination exceeds supported scope"
        );
    }
    Ok(value)
}

fn names(value: &Value, kind: &str) -> Result<BTreeSet<String>> {
    field(value, kind)?
        .as_array()
        .context("GitHub names unavailable")?
        .iter()
        .map(|row| {
            Ok(field(row, "name")?
                .as_str()
                .context("GitHub name invalid")?
                .to_owned())
        })
        .collect()
}

fn sync_payload(
    sync: &Sync,
    integration: &Integration,
    snapshot: &SecretSnapshot,
) -> Result<Value> {
    let target = sync_target(&sync.description)?;
    ensure!(
        target.owner == integration.name,
        "sync owner differs from connected GitHub account"
    );
    let repository = gh_json(&format!("repos/{}/{}", target.owner, target.repository))?;
    ensure!(
        field(field(&repository, "owner")?, "login")?
            .as_str()
            .is_some_and(|owner| owner.eq_ignore_ascii_case(&target.owner)),
        "GitHub repository owner differs"
    );
    ensure!(
        field(&repository, "id")?.as_u64().is_some(),
        "GitHub repository identity missing"
    );
    let base = if let Some(environment) = &target.environment {
        let response = gh_json(&format!(
            "repos/{}/{}/environments/{environment}",
            target.owner, target.repository
        ))?;
        ensure!(
            field(&response, "name")?.as_str() == Some(environment),
            "GitHub environment identity differs"
        );
        format!(
            "repos/{}/{}/environments/{environment}",
            target.owner, target.repository
        )
    } else {
        format!("repos/{}/{}/actions", target.owner, target.repository)
    };
    let secret_names = names(
        &gh_json(&format!("{base}/secrets?per_page=100"))?,
        "secrets",
    )?;
    let variable_names = names(
        &gh_json(&format!("{base}/variables?per_page=100"))?,
        "variables",
    )?;
    let values = snapshot
        .get(&(sync.project.clone(), sync.config.clone()))
        .context("sync source config missing")?;
    let mut variable_mode = None;
    for (name, value) in values {
        if value.computed_visibility == "unmasked" {
            let stored_as_variable = variable_names.contains(name);
            ensure!(
                stored_as_variable != secret_names.contains(name),
                "ambiguous unmasked GitHub storage in {}",
                sync.description
            );
            ensure!(
                variable_mode.is_none_or(|previous| previous == stored_as_variable),
                "mixed unmasked GitHub storage"
            );
            variable_mode = Some(stored_as_variable);
        } else {
            ensure!(
                secret_names.contains(name),
                "existing GitHub sync is missing a field in {}",
                sync.description
            );
        }
    }
    let mut payload = json!({"feature":"actions","sync_target":"repo","repo_name":target.repository,"sync_unmasked_as_variables":variable_mode.unwrap_or(false)});
    if let Some(environment) = target.environment {
        payload["environment_name"] = json!(environment);
    }
    Ok(payload)
}

fn integrations(client: &Client) -> Result<Vec<Integration>> {
    let value = client.read("/v3/integrations", &[])?;
    decode(field(&value, "integrations")?.clone(), "integrations")
}

fn copy_syncs(source: &Client, destination: &Client, snapshot: &SecretSnapshot) -> Result<()> {
    let sources = integrations(source)?;
    let targets = integrations(destination)?;
    let mut plan = Vec::new();
    for integration in &sources {
        ensure!(
            integration.kind == GITHUB_TYPE && integration.enabled,
            "unsupported or disabled source integration"
        );
        let matches: Vec<_> = targets
            .iter()
            .filter(|target| {
                target.kind == integration.kind && target.name == integration.name && target.enabled
            })
            .collect();
        let [target] = matches.as_slice() else {
            anyhow::bail!("exactly one matching destination GitHub connection required");
        };
        for sync in &integration.syncs {
            ensure!(
                sync.enabled && sync.status == "synced",
                "source sync is not healthy"
            );
            let payload = sync_payload(sync, integration, snapshot)?;
            let existing: Vec<_> = target
                .syncs
                .iter()
                .filter(|s| {
                    s.project == sync.project
                        && s.config == sync.config
                        && s.description == sync.description
                })
                .collect();
            ensure!(existing.len() <= 1, "duplicate destination sync");
            if let Some(existing) = existing.first() {
                ensure!(
                    existing.enabled && existing.status == "synced",
                    "destination sync is not healthy"
                );
            }
            plan.push((sync, *target, payload, existing.is_empty()));
        }
    }
    for (sync, target, payload, missing) in &plan {
        if *missing {
            let result = destination.write("/v3/configs/config/syncs", &[("project", &sync.project), ("config", &sync.config)], &json!({"integration":target.slug,"data":payload,"import_option":"none","await_initial_sync":true}))?;
            let created = field(&result, "sync")?;
            ensure!(
                field(created, "enabled")?.as_bool() == Some(true),
                "created sync is disabled"
            );
        }
        println!("Connected {}", sync.description);
    }
    let verified = integrations(destination)?;
    for (sync, target, _, _) in &plan {
        let integration = verified
            .iter()
            .find(|i| i.slug == target.slug)
            .context("destination integration disappeared")?;
        let matches: Vec<_> = integration
            .syncs
            .iter()
            .filter(|s| {
                s.project == sync.project
                    && s.config == sync.config
                    && s.description == sync.description
            })
            .collect();
        let [new] = matches.as_slice() else {
            anyhow::bail!("destination sync verification is ambiguous");
        };
        ensure!(
            new.enabled && new.status == "synced" && !new.slug.is_empty(),
            "first destination sync has not succeeded"
        );
    }
    println!(
        "Verified {} destination GitHub syncs; source syncs remain enabled",
        plan.len()
    );
    Ok(())
}

fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    field(value, name)?
        .as_str()
        .with_context(|| format!("invalid metadata field {name}"))
}

fn account_path(slug: &str) -> Result<String> {
    ensure!(
        !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "invalid service account slug"
    );
    Ok(format!(
        "/v3/workplace/service_accounts/service_account/{slug}"
    ))
}

fn role(role: &Value) -> Result<Value> {
    ensure!(
        role.get("is_custom_role").and_then(Value::as_bool) != Some(true),
        "custom roles require a separate adapter"
    );
    Ok(json!({"identifier":field(role,"identifier")?,"permissions":role.get("permissions")}))
}

fn copy_accounts(
    source: &Client,
    destination: &Client,
    projects: &[ProjectSnapshot],
) -> Result<()> {
    let accounts: Vec<Value> =
        source.list("/v3/workplace/service_accounts", &[], "service_accounts")?;
    let existing: Vec<Value> =
        destination.list("/v3/workplace/service_accounts", &[], "service_accounts")?;
    let mut mapping = BTreeMap::new();
    for account in &accounts {
        let name = text(account, "name")?;
        let slug = text(account, "slug")?;
        let path = account_path(slug)?;
        let tokens: Vec<Value> = source.list(&format!("{path}/tokens"), &[], "api_tokens")?;
        ensure!(
            tokens.is_empty(),
            "fixed service account tokens require consumer rotation"
        );
        let workplace_role = field(account, "workplace_role")?;
        ensure!(
            text(workplace_role, "identifier")? == "no_access",
            "only accounts without workplace access are supported"
        );
        let matches: Vec<_> = existing
            .iter()
            .filter(|row| row.get("name").and_then(Value::as_str) == Some(name))
            .collect();
        ensure!(matches.len() <= 1, "duplicate destination service account");
        let created;
        let target = if let Some(target) = matches.first() {
            *target
        } else {
            created = destination.write(
                "/v3/workplace/service_accounts",
                &[],
                &json!({"name":name,"workplace_role":{"identifier":"no_access"}}),
            )?;
            field(&created, "service_account")?
        };
        ensure!(
            role(field(target, "workplace_role")?)? == role(workplace_role)?,
            "destination workplace permissions differ"
        );
        let new_slug = text(target, "slug")?.to_owned();
        mapping.insert(slug.to_owned(), new_slug.clone());
        let new_path = account_path(&new_slug)?;
        let identities: Vec<Value> =
            source.list(&format!("{path}/identities"), &[], "identities")?;
        let target_identities: Vec<Value> =
            destination.list(&format!("{new_path}/identities"), &[], "identities")?;
        ensure!(
            target_identities.len() <= identities.len(),
            "destination has additional identities"
        );
        for identity in identities {
            ensure!(
                text(&identity, "method")? == "oidc"
                    && text(field(&identity, "config")?, "discovery_url")? == GITHUB_ISSUER,
                "only GitHub OIDC identities are supported"
            );
            let payload = json!({"name":field(&identity,"name")?,"method":field(&identity,"method")?,"config":field(&identity,"config")?,"ttl_seconds":field(&identity,"ttl_seconds")?});
            let matches: Vec<_> = target_identities
                .iter()
                .filter(|row| row.get("name") == identity.get("name"))
                .collect();
            ensure!(matches.len() <= 1, "duplicate destination identity");
            let response;
            let new_identity = if let Some(existing) = matches.first() {
                *existing
            } else {
                response = destination.write(&format!("{new_path}/identities"), &[], &payload)?;
                field(&response, "identity")?
            };
            for key in ["name", "method", "config", "ttl_seconds"] {
                ensure!(
                    field(new_identity, key)? == field(&payload, key)?,
                    "destination OIDC constraints differ"
                );
            }
            println!(
                "Identity mapping: {}",
                json!({"account":name,"source_identity":field(&identity,"slug")?,"destination_identity":field(new_identity,"slug")?})
            );
        }
    }
    copy_memberships(source, destination, projects, &mapping)?;
    println!(
        "Verified {} service accounts with their existing environment grants",
        accounts.len()
    );
    Ok(())
}

fn copy_memberships(
    source: &Client,
    destination: &Client,
    projects: &[ProjectSnapshot],
    mapping: &BTreeMap<String, String>,
) -> Result<()> {
    for project in projects {
        let query = [("project", project.project.id.as_str())];
        let members: Vec<Value> = source.list("/v3/projects/project/members", &query, "members")?;
        let target_members: Vec<Value> =
            destination.list("/v3/projects/project/members", &query, "members")?;
        for member in members
            .iter()
            .filter(|row| row.get("type").and_then(Value::as_str) == Some("service_account"))
        {
            let old_slug = text(member, "slug")?;
            let new_slug = mapping
                .get(old_slug)
                .context("source membership account missing")?;
            let role_id = text(field(member, "role")?, "identifier")?;
            let role_path = format!("/v3/projects/roles/role/{role_id}");
            ensure!(
                role(field(&source.read(&role_path, &[])?, "role")?)?
                    == role(field(&destination.read(&role_path, &[])?, "role")?)?,
                "project role permissions differ"
            );
            if let Some(existing) = target_members
                .iter()
                .find(|row| row.get("slug").and_then(Value::as_str) == Some(new_slug))
            {
                for key in ["role", "environments", "access_all_environments"] {
                    ensure!(
                        field(existing, key)? == field(member, key)?,
                        "destination membership differs"
                    );
                }
            } else {
                let payload = json!({"project":project.project.id,"type":"service_account","slug":new_slug,"role":role_id,"environments":field(member,"environments")?});
                let created = destination.write("/v3/projects/project/members", &[], &payload)?;
                for key in ["role", "environments", "access_all_environments"] {
                    ensure!(
                        field(field(&created, "member")?, key)? == field(member, key)?,
                        "created membership differs"
                    );
                }
            }
        }
    }
    Ok(())
}

pub(super) fn run(
    source: &Client,
    destination: &Client,
    projects: &[ProjectSnapshot],
    snapshot: &SecretSnapshot,
) -> Result<()> {
    copy_accounts(source, destination, projects)?;
    copy_syncs(source, destination, snapshot)
}

#[cfg(test)]
mod tests {
    #[test]
    fn sync_targets_keep_repository_case_and_environment_scope() {
        let target = super::sync_target("Actions: P4suta / Snaply / release-please").unwrap();
        assert_eq!(target.owner, "P4suta");
        assert_eq!(target.repository, "Snaply");
        assert_eq!(target.environment.as_deref(), Some("release-please"));
        assert!(
            super::sync_target("Actions: P4suta / example")
                .unwrap()
                .environment
                .is_none()
        );
        for invalid in [
            "Codespaces: P4suta / example",
            "Actions: P4suta / ../example",
            "Actions: P4suta / example / env / extra",
        ] {
            assert!(super::sync_target(invalid).is_err());
        }
    }
}
