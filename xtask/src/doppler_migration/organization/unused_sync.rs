use super::{
    ConfigNames, Report, Request, Result, Session, Value, cutover, git, json, names, retire, text,
};
use anyhow::{Context, ensure};

fn planned_config<'a>(
    report: &'a Report,
    repository: &str,
    sync: &Value,
) -> Result<&'a ConfigNames> {
    let project = text(sync, "project")?;
    let config = text(sync, "config")?;
    ensure!(
        repository.split_once('/').map(|(_, name)| name) == Some(project),
        "sync project differs from the selected repository"
    );
    let expected = format!("Actions: {}", repository.replace('/', " / "));
    let description = text(sync, "description")?;
    ensure!(
        description == expected
            || description
                .strip_prefix(&format!("{expected} / "))
                .is_some_and(|env| !env.is_empty()),
        "sync target differs from the selected repository"
    );
    let configs: Vec<_> = report
        .legacy_registry
        .iter()
        .filter(|entry| entry.project == project && entry.config == config)
        .collect();
    let [entry] = configs.as_slice() else {
        anyhow::bail!("one planned registry config is required");
    };
    ensure!(
        !entry.names.is_empty(),
        "planned registry field inventory is empty"
    );
    ensure!(
        sync.get("enabled").and_then(Value::as_bool) == Some(true)
            && text(sync, "status")? == "synced",
        "selected registry mirror is not healthy"
    );
    Ok(entry)
}

pub(super) fn run(request: &Request, session: &Session, report: &Report) -> Result<()> {
    let name = request.consumer.as_deref().context("consumer required")?;
    let slug = request.sync.as_deref().context("sync required")?;
    let repositories: Vec<_> = report
        .repositories
        .iter()
        .filter(|repo| repo.name == name)
        .collect();
    let [repository] = repositories.as_slice() else {
        anyhow::bail!("one selected repository is required");
    };
    let revision = cutover::main_revision(repository)?;
    ensure!(
        git(&repository.directory, &["rev-parse", "HEAD"])? == revision
            && git(&repository.directory, &["status", "--porcelain"])?.is_empty(),
        "unused sync review requires a clean current-main checkout"
    );
    let before = retire::syncs(&session.client)?;
    let targets: Vec<_> = before
        .iter()
        .filter(|sync| sync.get("slug").and_then(Value::as_str) == Some(slug))
        .collect();
    let [sync] = targets.as_slice() else {
        anyhow::bail!("one matching sync ID is required");
    };
    let config = planned_config(report, name, sync)?;
    ensure!(
        names(&session.client, &config.project, &config.config)? == config.names,
        "registry field inventory changed"
    );
    let mut fields = config.names.clone();
    fields.extend(cutover::SYNC_METADATA_FIELDS.map(str::to_owned));
    cutover::check_unused_fields(repository, &revision, &fields)?;
    println!(
        "{}",
        json!({"phase":"retire-unused-sync","repository":name,"revision":revision,
        "project":config.project,"config":config.config,"sync":slug,"target":text(sync,"description")?,
        "obsolete_github_fields":fields,"apply":request.apply,"stored_credentials_preserved":true})
    );
    if !request.apply {
        return Ok(());
    }
    ensure!(
        cutover::main_revision(repository)? == revision
            && names(&session.client, &config.project, &config.config)? == config.names,
        "registry scope moved before retirement"
    );
    retire::disconnect_unused(session, sync)?;
    let after = retire::syncs(&session.client)?;
    ensure!(
        after
            .iter()
            .all(|sync| sync.get("slug").and_then(Value::as_str) != Some(slug)),
        "selected registry sync remains"
    );
    for other in before
        .iter()
        .filter(|sync| sync.get("slug").and_then(Value::as_str) != Some(slug))
    {
        ensure!(
            after.iter().any(
                |current| ["slug", "project", "config", "description", "enabled"]
                    .iter()
                    .all(|key| other.get(key) == current.get(key))
            ),
            "unrelated writer metadata changed"
        );
    }
    ensure!(
        names(&session.client, &config.project, &config.config)? == config.names,
        "stored registry fields changed during retirement"
    );
    println!("Disconnected unused registry mirror; stored credentials and GitHub fields preserved");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        serde_json::from_value(json!({"workplace":"fixture","checkouts":"/fixture",
            "repositories":[],"configs":[],"consumers":[],
            "legacy_registry":[{"project":"project","config":"release","names":["REGISTRY_KEY"]}]}))
        .unwrap()
    }

    fn sync() -> Value {
        json!({"project":"project","config":"release","description":"Actions: owner / project / release","enabled":true,"status":"synced"})
    }

    #[test]
    fn unused_retirement_requires_the_planned_repository_config_and_healthy_mirror() {
        let baseline = report();
        planned_config(&baseline, "owner/project", &sync()).unwrap();
        for (field, value) in [
            ("project", json!("different")),
            ("config", json!("automation")),
            ("description", json!("Actions: owner / different / release")),
            ("enabled", json!(false)),
            ("status", json!("error")),
        ] {
            let mut changed = sync();
            changed[field] = value;
            assert!(
                planned_config(&baseline, "owner/project", &changed).is_err(),
                "{field}"
            );
        }
        let mut ambiguous = report();
        ambiguous
            .legacy_registry
            .push(ambiguous.legacy_registry[0].clone());
        assert!(planned_config(&ambiguous, "owner/project", &sync()).is_err());
        let mut empty = report();
        empty.legacy_registry[0].names.clear();
        assert!(planned_config(&empty, "owner/project", &sync()).is_err());
    }
}
