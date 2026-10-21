use super::{
    Client, Cutover, Report, Request, Result, Session, Tool, Value, decode, evidence, field, json,
    names, text,
};
use anyhow::{Context, ensure};
use rules::doppler_organization_rules::Change;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Stdio;

pub(super) fn syncs(client: &Client) -> Result<Vec<Value>> {
    let response = client.read("/v3/integrations", &[])?;
    let integrations: Vec<Value> =
        decode(field(&response, "integrations")?.clone(), "integrations")?;
    let mut result = Vec::new();
    for integration in integrations {
        ensure!(
            text(&integration, "type")? == "github-actions",
            "unsupported integration kind"
        );
        result.extend(
            field(&integration, "syncs")?
                .as_array()
                .context("sync metadata missing")?
                .iter()
                .cloned(),
        );
    }
    Ok(result)
}

pub(super) fn disconnect(
    session: &Session,
    sync: &Value,
    change: Change,
    cutover: Cutover,
) -> Result<()> {
    let evidence = super::Evidence {
        cutover,
        ..evidence()
    };
    ensure!(
        super::decide(session.mode, change, evidence) == super::Verdict::Allowed,
        "sync retirement was not authorized"
    );
    session.client.request(
        "DELETE",
        "/v3/configs/config/syncs/sync",
        &[
            ("project", text(sync, "project")?),
            ("config", text(sync, "config")?),
            ("sync", text(sync, "slug")?),
            ("delete_from_target", "false"),
        ],
        None,
    )?;
    Ok(())
}

pub(super) fn stop_source(request: &Request, destination: &Session, report: &Report) -> Result<()> {
    let expected = request
        .source
        .as_deref()
        .context("--source is required for source disconnection")?;
    let directory = request
        .source_config_dir
        .as_deref()
        .context("--source-config-dir is required for source disconnection")?;
    ensure!(
        expected != report.workplace,
        "source and destination must differ"
    );
    let source = Session::open(directory, &request.scope, expected, request.apply)?;
    let sources = syncs(&source.client)?;
    let targets = syncs(&destination.client)?;
    for sync in &sources {
        let matched: Vec<_> = targets
            .iter()
            .filter(|target| {
                ["project", "config", "description"]
                    .iter()
                    .all(|key| target.get(key) == sync.get(key))
            })
            .collect();
        let [target] = matched.as_slice() else {
            anyhow::bail!("one matching destination mirror required");
        };
        ensure!(
            target.get("enabled").and_then(Value::as_bool) == Some(true)
                && target.get("status").and_then(Value::as_str) == Some("synced"),
            "destination mirror has not synced"
        );
        let project = text(sync, "project")?;
        let config = text(sync, "config")?;
        let fields = names(&source.client, project, config)?;
        ensure!(
            fields == names(&destination.client, project, config)?,
            "source and destination names differ"
        );
        if !report
            .legacy_registry
            .iter()
            .any(|c| c.project == project && c.config == config)
        {
            ensure!(
                super::super::secrets(&source.client, project, config)?
                    == super::super::secrets(&destination.client, project, config)?,
                "source transfer verification failed; values withheld"
            );
        }
    }
    for sync in &sources {
        disconnect(
            &source,
            sync,
            Change::DisableSourceSync,
            Cutover {
                destination: true,
                consumer: false,
            },
        )?;
        println!(
            "Disconnected source sync {}/{}; GitHub fields preserved",
            text(sync, "project")?,
            text(sync, "config")?
        );
    }
    ensure!(syncs(&source.client)?.is_empty(), "source syncs remain");
    let current = syncs(&destination.client)?;
    ensure!(
        targets.iter().all(
            |old| current.iter().any(|new| old.get("slug") == new.get("slug")
                && new.get("enabled").and_then(Value::as_bool) == Some(true)
                && new.get("status").and_then(Value::as_str) == Some("synced"))
        ),
        "destination coverage changed during source disconnection"
    );
    println!(
        "Verified source disconnection and destination coverage; no GitHub credentials deleted"
    );
    Ok(())
}

fn reference_targets(raw: &str, project: &str, config: &str) -> Result<BTreeSet<(String, String)>> {
    let mut targets = BTreeSet::new();
    let mut remaining = raw;
    while let Some(start) = remaining.find("${") {
        remaining = remaining
            .get(start + 2..)
            .context("invalid reference boundary")?;
        let end = remaining
            .find('}')
            .context("unknown reference syntax prevents cleanup")?;
        let expression = remaining
            .get(..end)
            .context("invalid reference expression")?;
        ensure!(
            !expression.contains(['"', '\\', '\'']),
            "quoted reference syntax requires an explicit cleanup adapter"
        );
        let parts: Vec<_> = expression.split('.').collect();
        let target = match parts.as_slice() {
            [_name] => (project.to_owned(), config.to_owned()),
            [other, _name] => (project.to_owned(), (*other).to_owned()),
            [other_project, other_config, _name] => {
                ((*other_project).to_owned(), (*other_config).to_owned())
            }
            _ => anyhow::bail!("unknown reference syntax prevents cleanup"),
        };
        targets.insert(target);
        remaining = remaining
            .get(end + 1..)
            .context("invalid reference suffix")?;
    }
    Ok(targets)
}

fn local_selections(request: &Request, report: &Report) -> Result<BTreeSet<(String, String)>> {
    let mut directories: BTreeSet<PathBuf> = report
        .repositories
        .iter()
        .map(|r| r.directory.clone())
        .collect();
    directories.insert(request.scope.clone());
    let parent = request
        .checkouts
        .parent()
        .context("checkout parent missing")?;
    for entry in std::fs::read_dir(parent)? {
        let path = entry?.path();
        if path.join(".git").exists() {
            directories.insert(path);
        }
    }
    let mut selected = BTreeSet::new();
    let mut config_dirs = BTreeSet::from([
        request.config_dir.clone(),
        platform::home::home()
            .context("native home unavailable")?
            .join(".doppler"),
    ]);
    config_dirs.extend(request.source_config_dir.iter().cloned());
    for (config_dir, directory) in config_dirs.iter().flat_map(|config_dir| {
        directories
            .iter()
            .map(move |directory| (config_dir, directory))
    }) {
        let output = Tool::Doppler
            .command()
            .args([
                "configure",
                "get",
                "project",
                "config",
                "--json",
                "--no-read-env",
                "--no-check-version",
                "--config-dir",
            ])
            .arg(config_dir)
            .arg("--scope")
            .arg(directory)
            .stdin(Stdio::null())
            .output()?;
        ensure!(
            output.status.success(),
            "local project/config metadata lookup failed; cleanup refused"
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        let project = text(&value, "enclave.project")?;
        let config = text(&value, "enclave.config")?;
        if !project.is_empty() || !config.is_empty() {
            ensure!(
                !project.is_empty() && !config.is_empty(),
                "partial local selection prevents cleanup"
            );
            selected.insert((project.to_owned(), config.to_owned()));
        }
    }
    Ok(selected)
}

fn dependencies(
    session: &Session,
    projects: &[super::super::ProjectSnapshot],
) -> Result<BTreeSet<(String, String)>> {
    let mut referenced = BTreeSet::new();
    for project in projects {
        if project.project.id == "example-project" {
            continue;
        }
        for config in &project.configs {
            let fields = names(&session.client, &project.project.id, &config.name)?;
            if fields.is_empty() {
                continue;
            }
            let values = super::super::secrets(&session.client, &project.project.id, &config.name)?;
            for value in values.values() {
                let raw = value
                    .raw
                    .as_deref()
                    .context("unreadable reference prevents cleanup")?;
                referenced.extend(reference_targets(raw, &project.project.id, &config.name)?);
            }
        }
    }
    Ok(referenced)
}

fn config_unused(
    session: &Session,
    project: &str,
    config: &str,
    referenced: &BTreeSet<(String, String)>,
    local: &BTreeSet<(String, String)>,
    mirrors: &[Value],
    members: &[Value],
) -> Result<bool> {
    if referenced.contains(&(project.to_owned(), config.to_owned()))
        || local.contains(&(project.to_owned(), config.to_owned()))
    {
        return Ok(false);
    }
    if mirrors.iter().any(|s| {
        s.get("project").and_then(Value::as_str) == Some(project)
            && s.get("config").and_then(Value::as_str) == Some(config)
    }) {
        return Ok(false);
    }
    if members.iter().any(|m| {
        m.get("type").and_then(Value::as_str) != Some("workplace_user")
            && (m.get("access_all_environments").and_then(Value::as_bool) == Some(true)
                || m.get("environments")
                    .and_then(Value::as_array)
                    .is_some_and(|e| e.iter().any(|name| name.as_str() == Some(config))))
    }) {
        return Ok(false);
    }
    let tokens = session.client.read(
        "/v3/configs/config/tokens",
        &[("project", project), ("config", config)],
    )?;
    let value = field(&tokens, "tokens")?;
    Ok(value.is_null() || value.as_array().is_some_and(Vec::is_empty))
}

fn unused_environments(
    request: &Request,
    session: &Session,
    report: &Report,
) -> Result<Vec<(String, String)>> {
    let local = local_selections(request, report)?;
    let projects = super::inventory(&session.client)?;
    let referenced = dependencies(session, &projects)?;
    let mirrors = syncs(&session.client)?;
    let mut removable = Vec::new();
    for project in &projects {
        let id = project.project.id.as_str();
        let members: Vec<Value> = session.client.list(
            "/v3/projects/project/members",
            &[("project", id)],
            "members",
        )?;
        let webhooks: Vec<Value> =
            session
                .client
                .list("/v3/webhooks", &[("project", id)], "webhooks")?;
        if !webhooks.is_empty() {
            continue;
        }
        let mut by_environment: BTreeMap<&str, Vec<&super::super::Config>> = BTreeMap::new();
        for config in &project.configs {
            by_environment
                .entry(&config.environment)
                .or_default()
                .push(config);
        }
        for (environment, configs) in by_environment {
            let mut unused = true;
            for config in configs {
                let declared_empty = report
                    .configs
                    .iter()
                    .any(|c| c.project == id && c.config == config.name && c.names.is_empty());
                let demonstration = id == "example-project"
                    && report
                        .configs
                        .iter()
                        .any(|c| c.project == id && c.config == config.name);
                let current_names = names(&session.client, id, &config.name)?;
                let unchanged_demo = demonstration
                    && report.configs.iter().any(|c| {
                        c.project == id && c.config == config.name && c.names == current_names
                    });
                let eligible = unchanged_demo || declared_empty && current_names.is_empty();
                unused &= eligible
                    && config_unused(
                        session,
                        id,
                        &config.name,
                        &referenced,
                        &local,
                        &mirrors,
                        &members,
                    )?;
            }
            if unused {
                removable.push((id.to_owned(), environment.to_owned()));
            }
        }
    }
    Ok(removable)
}

pub(super) fn cleanup(request: &Request, session: &Session, report: &Report) -> Result<()> {
    let mut removable = unused_environments(request, session, report)?;
    let demonstration = removable
        .iter()
        .filter(|(project, _)| project == "example-project")
        .count();
    let projects = super::inventory(&session.client)?;
    let remove_demo = projects.iter().any(|p| {
        p.project.id == "example-project"
            && !p.environments.is_empty()
            && p.environments.len() == demonstration
    });
    removable.retain(|(project, _)| project != "example-project");
    if remove_demo {
        let proof = super::Evidence {
            unused: true,
            ..evidence()
        };
        ensure!(
            super::decide(session.mode, Change::RemoveUnused, proof) == super::Verdict::Allowed,
            "demonstration removal refused"
        );
        session.client.request(
            "DELETE",
            "/v3/projects/project",
            &[],
            Some(&json!({"project":"example-project"})),
        )?;
        println!("Removed the unused demonstration project");
    }
    for (project, environment) in &removable {
        let proof = super::Evidence {
            unused: true,
            ..evidence()
        };
        ensure!(
            super::decide(session.mode, Change::RemoveUnused, proof) == super::Verdict::Allowed,
            "unused environment removal refused"
        );
        session.client.request(
            "DELETE",
            "/v3/environments/environment",
            &[("project", project), ("environment", environment)],
            None,
        )?;
        println!("Removed unused environment {project}/{environment}");
    }
    let after = super::inventory(&session.client)?;
    ensure!(
        removable.iter().all(|(project, environment)| after
            .iter()
            .find(|p| &p.project.id == project)
            .is_none_or(|p| p.environments.iter().all(|e| &e.id != environment))),
        "unused environment removal did not persist"
    );
    ensure!(
        !remove_demo || after.iter().all(|p| p.project.id != "example-project"),
        "demonstration project removal did not persist"
    );
    println!(
        "Removed {} unused environments; active consumer delivery remains until live cutover evidence exists",
        removable.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn cleanup_finds_embedded_cross_project_dependencies_and_refuses_unknown_syntax() {
        let references = super::reference_targets(
            "prefix-${shared-signing.windows.KEY}-${prd.KEY}",
            "project",
            "automation",
        )
        .unwrap();
        assert!(references.contains(&("shared-signing".to_owned(), "windows".to_owned())));
        assert!(references.contains(&("project".to_owned(), "prd".to_owned())));
        assert!(super::reference_targets("${a.\"b.c\".KEY}", "project", "automation").is_err());
    }
}
