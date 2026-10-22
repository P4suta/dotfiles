use super::{
    Consumer, Cutover, Report, Repository, Request, Result, Session, Tool, VERIFY_WORKFLOW, Value,
    field, gh, git, names, retire, text, workflows,
};
use anyhow::{Context, ensure};
use rules::doppler_organization_rules::{Change, ConfigInventory, config_inventory_matches};
use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;
use yaml_rust2::{Yaml, YamlLoader};

const GITHUB_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const MAIN_BRANCH: &str = "main";
const MANUAL_EVENT: &str = "workflow_dispatch";
pub(super) const SYNC_METADATA_FIELDS: [&str; 3] =
    ["DOPPLER_CONFIG", "DOPPLER_ENVIRONMENT", "DOPPLER_PROJECT"];

pub(super) fn main_revision(repository: &Repository) -> Result<String> {
    let actual = gh(&format!("repos/{}", repository.name))?;
    ensure!(
        actual.get("id").and_then(Value::as_u64) == Some(repository.id)
            && actual.pointer("/owner/id").and_then(Value::as_u64) == Some(repository.owner_id)
            && text(&actual, "default_branch")? == MAIN_BRANCH,
        "consumer repository identity or default branch differs"
    );
    let remote = git(&repository.directory, &["remote", "get-url", "origin"])?;
    let name = remote
        .strip_prefix(super::GITHUB_ORIGIN)
        .or_else(|| remote.strip_prefix("git@github.com:"))
        .context("consumer remote must target github.com")?
        .trim_end_matches(".git");
    ensure!(name == repository.name, "consumer checkout remote differs");
    let current = gh(&format!(
        "repos/{}/git/ref/heads/{MAIN_BRANCH}",
        repository.name
    ))?;
    Ok(text(field(&current, "object")?, "sha")?.to_owned())
}

fn private_logs(repository: &Repository, job: u64) -> Result<String> {
    let output = Tool::Gh
        .command()
        .args([
            "api",
            "--hostname",
            "github.com",
            &format!("repos/{}/actions/jobs/{job}/logs", repository.name),
        ])
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        output.status.success() && output.stdout.len() <= GITHUB_OUTPUT_LIMIT,
        "authentication job logs unavailable or oversized; body withheld"
    );
    String::from_utf8(output.stdout)
        .map_err(|_| anyhow::anyhow!("invalid authentication log encoding; body withheld"))
}

fn check_run(
    run: &Value,
    repository: &Repository,
    number: u64,
    attempt: u64,
    revision: &str,
) -> Result<()> {
    ensure!(
        run.get("id").and_then(Value::as_u64) == Some(number)
            && run.get("run_attempt").and_then(Value::as_u64) == Some(attempt)
            && run.pointer("/repository/id").and_then(Value::as_u64) == Some(repository.id)
            && run.pointer("/head_repository/id").and_then(Value::as_u64) == Some(repository.id)
            && text(run, "path")? == VERIFY_WORKFLOW
            && text(run, "event")? == MANUAL_EVENT
            && text(run, "head_branch")? == MAIN_BRANCH
            && text(run, "head_sha")? == revision
            && text(run, "status")? == "completed"
            && text(run, "conclusion")? == "success",
        "authentication run does not match the exact completed manual main verification"
    );
    Ok(())
}

fn check_job(expected: &Yaml, actual: &Value, logs: &str) -> Result<()> {
    ensure!(
        text(actual, "status")? == "completed" && text(actual, "conclusion")? == "success",
        "credential job did not succeed"
    );
    let steps = field(actual, "steps")?
        .as_array()
        .context("job step metadata missing")?;
    for expected in expected["steps"]
        .as_vec()
        .context("expected steps missing")?
    {
        let name = expected["name"]
            .as_str()
            .context("expected step name missing")?;
        let matching: Vec<_> = steps
            .iter()
            .filter(|step| step.get("name").and_then(Value::as_str) == Some(name))
            .collect();
        let [step] = matching.as_slice() else {
            anyhow::bail!("expected credential step is missing or ambiguous");
        };
        let conclusion = if name.starts_with("Reject ") {
            "skipped"
        } else {
            "success"
        };
        ensure!(
            text(step, "conclusion")? == conclusion,
            "credential step did not produce its required outcome"
        );
        if expected["id"].as_str() == Some("forbidden") {
            let input = &expected["with"];
            let project = input["doppler-project"]
                .as_str()
                .context("negative project missing")?;
            let config = input["doppler-config"]
                .as_str()
                .context("negative config missing")?;
            ensure!(
                logs.contains(&format!("Doppler API Error: This token does not have access to requested project '{project}'"))
                    || logs.contains(&format!("Doppler API Error: This token does not have access to requested config '{config}'")),
                "negative check lacks an explicit Doppler project or config access denial; logs withheld"
            );
        }
    }
    Ok(())
}

fn remaining_consumers(
    repository: &Repository,
    revision: &str,
    report: &Report,
    consumers: &[&Consumer],
) -> Result<()> {
    check_unused_fields(repository, revision, &remaining_fields(report, consumers)?)
}

pub(super) fn check_unused_fields(
    repository: &Repository,
    revision: &str,
    fields: &BTreeSet<String>,
) -> Result<()> {
    let paths = git(
        &repository.directory,
        &["ls-tree", "-r", "--name-only", revision, "--", ".github"],
    )?;
    let mut sources = BTreeMap::new();
    for path in paths.lines().filter(|path| {
        (path.starts_with(".github/workflows/") || path.starts_with(".github/actions/"))
            && std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
                })
    }) {
        let source = git(
            &repository.directory,
            &["show", &format!("{revision}:{path}")],
        )?;
        sources.insert(path.to_owned(), source);
    }
    ensure!(!sources.is_empty(), "workflow consumer inventory is empty");
    check_workflow_sources(&sources, fields)
}

fn remaining_fields(report: &Report, consumers: &[&Consumer]) -> Result<BTreeSet<String>> {
    let mut fields: BTreeSet<_> = consumers
        .iter()
        .flat_map(|consumer| consumer.aliases.keys().cloned())
        .collect();
    fields.extend(SYNC_METADATA_FIELDS.map(str::to_owned));
    for consumer in consumers {
        let config = report
            .configs
            .iter()
            .find(|config| config.project == consumer.project && config.config == consumer.source)
            .context("legacy config inventory missing")?;
        fields.extend(config.names.iter().cloned());
        if consumer.source == consumer.purpose.config() {
            fields.extend(consumer.purpose.fields().iter().cloned());
        }
    }
    Ok(fields)
}

fn check_workflow_sources(
    sources: &BTreeMap<String, String>,
    fields: &BTreeSet<String>,
) -> Result<()> {
    for (path, source) in sources {
        check_secret_references(source, fields, sources)
            .with_context(|| format!("remaining consumer review at {path}"))?;
    }
    Ok(())
}

fn check_secret_references(
    source: &str,
    fields: &BTreeSet<String>,
    sources: &BTreeMap<String, String>,
) -> Result<()> {
    let documents = YamlLoader::load_from_str(source)?;
    ensure!(documents.len() == 1, "one workflow document is required");
    check_yaml(&documents[0], YamlScope::Workflow, fields, sources)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum YamlScope {
    Workflow,
    Jobs,
    Job,
    Other,
}

fn check_yaml(
    value: &Yaml,
    scope: YamlScope,
    fields: &BTreeSet<String>,
    sources: &BTreeMap<String, String>,
) -> Result<()> {
    match value {
        Yaml::String(value) => check_expression(value, fields)?,
        Yaml::Array(values) => {
            for value in values {
                check_yaml(value, YamlScope::Other, fields, sources)?;
            }
        }
        Yaml::Hash(entries) => {
            if scope == YamlScope::Workflow
                && let Some(declarations) = value["on"]["workflow_call"]["secrets"].as_hash()
            {
                for (name, declaration) in declarations {
                    if name.as_str().is_some_and(|name| {
                        fields.iter().any(|field| field.eq_ignore_ascii_case(name))
                    }) {
                        ensure!(
                            matches!(
                                declaration["required"],
                                Yaml::BadValue | Yaml::Boolean(false)
                            ),
                            "a reusable workflow still requires a legacy credential"
                        );
                    }
                }
            }
            for (key, child) in entries {
                if key.as_str() == Some("secrets") && child.as_str() == Some("inherit") {
                    ensure!(
                        scope == YamlScope::Job,
                        "secret inheritance must belong to a reviewed reusable job"
                    );
                    let target = value["uses"]
                        .as_str()
                        .context("inherited secret destination is unresolved")?;
                    ensure!(
                        target.starts_with("./.github/workflows/")
                            && target
                                .strip_prefix("./")
                                .is_some_and(|path| sources.contains_key(path)),
                        "inherited secrets leave the reviewed first-party workflow tree"
                    );
                }
                let child_scope = match scope {
                    YamlScope::Workflow if key.as_str() == Some("jobs") => YamlScope::Jobs,
                    YamlScope::Jobs => YamlScope::Job,
                    YamlScope::Workflow | YamlScope::Job | YamlScope::Other => YamlScope::Other,
                };
                check_yaml(key, YamlScope::Other, fields, sources)?;
                check_yaml(child, child_scope, fields, sources)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn check_expression(value: &str, fields: &BTreeSet<String>) -> Result<()> {
    let compact = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    ensure!(
        !compact.contains("secrets["),
        "dynamic secret access needs an explicit consumer review"
    );
    ensure!(
        !compact.contains("tojson(secrets)"),
        "whole-secret forwarding needs an explicit consumer review"
    );
    ensure!(
        fields
            .iter()
            .all(|name| !compact.contains(&format!("secrets.{}", name.to_ascii_lowercase()))),
        "a workflow still uses a legacy GitHub credential"
    );
    Ok(())
}

fn target_matches(sync: &Value, consumer: &Consumer) -> Result<bool> {
    if text(sync, "project")? != consumer.project || text(sync, "config")? != consumer.source {
        return Ok(false);
    }
    let prefix = format!("Actions: {}", consumer.repository.replace('/', " / "));
    let description = text(sync, "description")?;
    Ok(description == prefix
        || description
            .strip_prefix(&format!("{prefix} / "))
            .is_some_and(|environment| {
                consumer
                    .jobs
                    .iter()
                    .any(|job| job.environment == environment)
            }))
}

struct VerifiedConsumer<'a> {
    repository: &'a Repository,
    consumers: Vec<&'a Consumer>,
    revision: String,
    number: u64,
    attempt: u64,
}

fn verify_authentication(
    report: &Report,
    repository: &Repository,
    consumers: &[&Consumer],
    number: u64,
    attempt: u64,
) -> Result<String> {
    let name = &repository.name;
    let revision = main_revision(repository)?;
    let run = gh(&format!("repos/{name}/actions/runs/{number}"))?;
    check_run(&run, repository, number, attempt, &revision)?;
    let source = git(
        &repository.directory,
        &["show", &format!("{revision}:{VERIFY_WORKFLOW}")],
    )?;
    let expected = workflows::checked_verification(report, consumers, &source)?;
    remaining_consumers(repository, &revision, report, consumers)?;
    let response = gh(&format!(
        "repos/{name}/actions/runs/{number}/attempts/{attempt}/jobs?per_page=100"
    ))?;
    let jobs = field(&response, "jobs")?
        .as_array()
        .context("authentication jobs missing")?;
    let expected_jobs = expected["jobs"]
        .as_hash()
        .context("expected jobs missing")?;
    ensure!(
        jobs.len() == expected_jobs.len()
            && response.get("total_count").and_then(Value::as_u64)
                == u64::try_from(jobs.len()).ok(),
        "every expected purpose and environment must complete in one verification attempt"
    );
    let mut matched = BTreeSet::new();
    for expected in expected_jobs.values() {
        let label = expected["name"]
            .as_str()
            .context("expected job label missing")?;
        let matching: Vec<_> = jobs
            .iter()
            .filter(|job| job.get("name").and_then(Value::as_str) == Some(label))
            .collect();
        let [job] = matching.as_slice() else {
            anyhow::bail!("expected purpose or environment is missing or ambiguous");
        };
        let id = job
            .get("id")
            .and_then(Value::as_u64)
            .context("job identity missing")?;
        ensure!(matched.insert(id), "duplicate authentication job identity");
        check_job(expected, job, &private_logs(repository, id)?)?;
    }
    Ok(revision)
}

fn verify_configs(report: &Report, session: &Session, consumers: &[&Consumer]) -> Result<()> {
    for consumer in consumers {
        let old = report
            .configs
            .iter()
            .find(|config| config.project == consumer.project && config.config == consumer.source)
            .context("legacy config inventory missing")?;
        let source = names(&session.client, &consumer.project, &consumer.source)?;
        let expected_names = consumer.purpose.fields().iter().cloned().collect();
        let purpose = (consumer.source != consumer.purpose.config())
            .then(|| {
                names(
                    &session.client,
                    &consumer.project,
                    consumer.purpose.config(),
                )
            })
            .transpose()?;
        check_config_inventory(&old.names, &expected_names, &source, purpose.as_ref())?;
    }
    Ok(())
}

fn check_config_inventory(
    legacy: &BTreeSet<String>,
    canonical: &BTreeSet<String>,
    source: &BTreeSet<String>,
    purpose: Option<&BTreeSet<String>>,
) -> Result<()> {
    let inventory = if let Some(purpose) = purpose {
        ConfigInventory::Distinct {
            legacy: legacy == source,
            canonical: canonical == purpose,
        }
    } else {
        let combined: BTreeSet<_> = legacy.union(canonical).cloned().collect();
        ConfigInventory::Shared {
            canonical: canonical == source,
            combined: &combined == source,
        }
    };
    ensure!(
        config_inventory_matches(inventory),
        "config field inventory differs from the verified contract"
    );
    Ok(())
}

fn retire_writers(
    request: &Request,
    session: &Session,
    verified: &VerifiedConsumer<'_>,
) -> Result<()> {
    let repository = verified.repository;
    let name = &repository.name;
    let consumers = &verified.consumers;
    let revision = verified.revision.as_str();
    let number = verified.number;
    let attempt = verified.attempt;
    let before = retire::syncs(&session.client)?;
    let mut selected = Vec::new();
    for sync in &before {
        for consumer in consumers {
            if target_matches(sync, consumer)? {
                ensure!(
                    sync.get("enabled").and_then(Value::as_bool) == Some(true)
                        && text(sync, "status")? == "synced",
                    "legacy consumer sync is not healthy"
                );
                selected.push(sync);
                break;
            }
        }
    }
    for sync in &selected {
        println!(
            "Verified legacy writer {}/{} -> {}; target fields preserved",
            text(sync, "project")?,
            text(sync, "config")?,
            text(sync, "description")?
        );
    }
    if !request.apply {
        println!(
            "Verified {name} run {number} attempt {attempt} at {revision}; {} writer(s) ready for retirement; no mutation",
            selected.len()
        );
        return Ok(());
    }
    for sync in &selected {
        ensure!(
            main_revision(repository)? == revision,
            "consumer main moved before retirement"
        );
        retire::disconnect(
            session,
            sync,
            Change::RetireConsumer,
            Cutover {
                destination: true,
                consumer: true,
            },
        )?;
        println!(
            "Disconnected {}/{}; GitHub fields preserved",
            text(sync, "project")?,
            text(sync, "config")?
        );
    }
    let after = retire::syncs(&session.client)?;
    let selected_ids: BTreeSet<_> = selected
        .iter()
        .map(|sync| text(sync, "slug"))
        .collect::<Result<_>>()?;
    ensure!(
        after
            .iter()
            .all(|sync| text(sync, "slug").is_ok_and(|id| !selected_ids.contains(id))),
        "selected consumer writers remain"
    );
    for old in before.iter().filter(|sync| {
        sync.get("slug")
            .and_then(Value::as_str)
            .is_some_and(|id| !selected_ids.contains(id))
    }) {
        ensure!(
            after.iter().any(
                |current| ["slug", "project", "config", "description", "enabled"]
                    .iter()
                    .all(|key| old.get(key) == current.get(key))
            ),
            "unrelated writer metadata changed during retirement"
        );
    }
    println!(
        "Retired {} writer(s) for {name}; remove only their obsolete GitHub fields after the remaining-consumer review",
        selected.len()
    );
    Ok(())
}

pub(super) fn run(request: &Request, session: &Session, report: &Report) -> Result<()> {
    let name = request
        .consumer
        .as_deref()
        .context("--consumer OWNER/REPO is required")?;
    let number = request
        .verification_run
        .context("--verification-run is required")?;
    let attempt = request
        .verification_attempt
        .context("--verification-attempt is required")?;
    ensure!(
        number > 0 && attempt > 0,
        "positive run and attempt required"
    );
    let repositories: Vec<_> = report
        .repositories
        .iter()
        .filter(|repository| repository.name == name)
        .collect();
    let [repository] = repositories.as_slice() else {
        anyhow::bail!("one selected repository is required in the organization report");
    };
    let consumers: Vec<_> = report
        .consumers
        .iter()
        .filter(|consumer| consumer.repository == name)
        .collect();
    ensure!(
        !consumers.is_empty(),
        "selected repository has no planned consumers"
    );
    let revision = verify_authentication(report, repository, &consumers, number, attempt)?;
    verify_configs(report, session, &consumers)?;
    retire_writers(
        request,
        session,
        &VerifiedConsumer {
            repository,
            consumers,
            revision,
            number,
            attempt,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_writer_review_includes_its_new_canonical_field_names() {
        let report: Report = serde_json::from_value(json!({
            "workplace":"workplace", "checkouts":"/checkouts", "repositories":[],
            "configs":[{"project":"project", "config":"automation",
                "names":["RELEASE_PLEASE_CLIENT_ID", "RELEASE_PLEASE_PRIVATE_KEY"]}],
            "consumers":[{"project":"project", "source":"automation", "purpose":"automation",
                "repository":"owner/project", "shared":"shared.automation",
                "aliases":{"RELEASE_PLEASE_CLIENT_ID":"GITHUB_APP_CLIENT_ID",
                    "RELEASE_PLEASE_PRIVATE_KEY":"GITHUB_APP_PRIVATE_KEY"}, "jobs":[]}],
            "legacy_registry":[]
        }))
        .unwrap();
        let consumers: Vec<_> = report.consumers.iter().collect();
        let fields = remaining_fields(&report, &consumers).unwrap();
        for name in ["GITHUB_APP_CLIENT_ID", "GITHUB_APP_PRIVATE_KEY"] {
            let source = format!("key: ${{{{ secrets.{name} }}}}");
            assert!(check_secret_references(&source, &fields, &BTreeMap::new()).is_err());
        }
        check_secret_references(
            "key: ${{ steps.credentials.outputs.GITHUB_APP_PRIVATE_KEY }}",
            &fields,
            &BTreeMap::new(),
        )
        .unwrap();
    }

    #[test]
    fn shared_configs_admit_only_complete_migration_states() {
        let legacy = BTreeSet::from([
            "RELEASE_PLEASE_CLIENT_ID".to_owned(),
            "RELEASE_PLEASE_PRIVATE_KEY".to_owned(),
        ]);
        let canonical = BTreeSet::from([
            "GITHUB_APP_CLIENT_ID".to_owned(),
            "GITHUB_APP_PRIVATE_KEY".to_owned(),
        ]);
        let combined = legacy.union(&canonical).cloned().collect();
        check_config_inventory(&legacy, &canonical, &combined, None).unwrap();
        check_config_inventory(&legacy, &canonical, &canonical, None).unwrap();
        for fields in [
            legacy.clone(),
            BTreeSet::new(),
            BTreeSet::from(["GITHUB_APP_CLIENT_ID".to_owned()]),
            canonical
                .union(&BTreeSet::from(["RELEASE_PLEASE_CLIENT_ID".to_owned()]))
                .cloned()
                .collect(),
            canonical
                .union(&BTreeSet::from(["UNEXPECTED".to_owned()]))
                .cloned()
                .collect(),
        ] {
            assert!(check_config_inventory(&legacy, &canonical, &fields, None).is_err());
        }
        check_config_inventory(&legacy, &canonical, &legacy, Some(&canonical)).unwrap();
        assert!(check_config_inventory(&legacy, &canonical, &combined, Some(&canonical)).is_err());
        assert!(check_config_inventory(&legacy, &canonical, &legacy, Some(&combined)).is_err());
    }

    fn repository() -> Repository {
        Repository {
            name: "owner/project".to_owned(),
            id: 7,
            owner_id: 8,
            revision: "old".to_owned(),
            directory: std::path::PathBuf::new(),
            subject: String::new(),
        }
    }

    #[test]
    fn authentication_evidence_rejects_another_attempt_revision_event_or_repository() {
        let repository = repository();
        let run = json!({"id":9,"run_attempt":1,"repository":{"id":7},"head_repository":{"id":7},
            "path":VERIFY_WORKFLOW,"event":"workflow_dispatch","head_branch":"main",
            "head_sha":"current","status":"completed","conclusion":"success"});
        check_run(&run, &repository, 9, 1, "current").unwrap();
        for (pointer, replacement) in [
            ("/id", json!(10)),
            ("/run_attempt", json!(2)),
            ("/repository/id", json!(10)),
            ("/head_repository/id", json!(10)),
            ("/path", json!(".github/workflows/release.yml")),
            ("/event", json!("push")),
            ("/head_branch", json!("feature")),
            ("/head_sha", json!("stale")),
            ("/status", json!("in_progress")),
            ("/conclusion", json!("failure")),
        ] {
            let mut changed = run.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                check_run(&changed, &repository, 9, 1, "current").is_err(),
                "{pointer}"
            );
        }
    }

    #[test]
    fn a_transport_failure_or_another_denial_does_not_prove_config_isolation() {
        let expected = yaml_rust2::YamlLoader::load_from_str("steps:\n  - name: Fetch own credentials\n  - name: Reject missing credentials\n  - name: Attempt unrelated credential access\n    id: forbidden\n    with:\n      doppler-project: unrelated\n      doppler-config: sign_windows\n  - name: Reject unrelated credential access\n").unwrap().remove(0);
        let job = json!({"status":"completed","conclusion":"success","steps":[
            {"name":"Fetch own credentials","conclusion":"success"},
            {"name":"Reject missing credentials","conclusion":"skipped"},
            {"name":"Attempt unrelated credential access","conclusion":"success"},
            {"name":"Reject unrelated credential access","conclusion":"skipped"}]});
        let denial =
            "Doppler API Error: This token does not have access to requested config 'sign_windows'";
        check_job(&expected, &job, denial).unwrap();
        for logs in [
            "",
            "error connecting to api.doppler.com",
            "Doppler API Error: Invalid Auth token",
            "Doppler API Error: This token does not have access to requested config 'different'",
        ] {
            assert!(check_job(&expected, &job, logs).is_err());
        }
        let mut missing = job.clone();
        missing["steps"].as_array_mut().unwrap().remove(0);
        assert!(check_job(&expected, &missing, denial).is_err());
        let mut failed = job;
        failed["steps"][0]["conclusion"] = json!("failure");
        assert!(check_job(&expected, &failed, denial).is_err());
    }

    #[test]
    fn remaining_consumer_checks_include_case_variants_and_whole_secret_forwarding() {
        let fields = BTreeSet::from(["PRIVATE_KEY".to_owned()]);
        check_secret_references(
            "private-key: ${{ steps.credentials.outputs.GITHUB_APP_PRIVATE_KEY }}",
            &fields,
            &BTreeMap::new(),
        )
        .unwrap();
        for source in [
            "key: ${{ secrets.PRIVATE_KEY }}",
            "key: ${{ SECRETS.private_key }}",
            "key: ${{ secrets [ 'PRIVATE_KEY' ] }}",
            "env: ${{ toJSON(secrets) }}",
            "secrets: inherit",
        ] {
            assert!(
                check_secret_references(source, &fields, &BTreeMap::new()).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn local_inheritance_requires_the_complete_first_party_source_tree() {
        let fields = BTreeSet::from(["PRIVATE_KEY".to_owned()]);
        let entry =
            "jobs:\n  publish:\n    uses: ./.github/workflows/publish.yml\n    secrets: inherit\n";
        let publisher =
            "on:\n  workflow_call:\njobs:\n  publish:\n    steps:\n      - run: echo public\n";
        let sources = BTreeMap::from([
            (".github/workflows/release.yml".to_owned(), entry.to_owned()),
            (
                ".github/workflows/publish.yml".to_owned(),
                publisher.to_owned(),
            ),
        ]);
        check_workflow_sources(&sources, &fields).unwrap();
        let mut missing = sources.clone();
        missing.remove(".github/workflows/publish.yml");
        assert!(check_workflow_sources(&missing, &fields).is_err());
        let mut external = sources.clone();
        external.insert(
            ".github/workflows/release.yml".to_owned(),
            entry.replace(
                "./.github/workflows/publish.yml",
                "owner/other/.github/workflows/publish.yml@main",
            ),
        );
        assert!(check_workflow_sources(&external, &fields).is_err());
        let mut reader = sources.clone();
        reader.insert(
            ".github/workflows/publish.yml".to_owned(),
            publisher.replace("echo public", "echo ${{ secrets.PRIVATE_KEY }}"),
        );
        assert!(check_workflow_sources(&reader, &fields).is_err());
        let mut required = sources;
        required.insert(
            ".github/workflows/publish.yml".to_owned(),
            publisher.replace(
                "workflow_call:",
                "workflow_call:\n    secrets:\n      PRIVATE_KEY:\n        required: true",
            ),
        );
        assert!(check_workflow_sources(&required, &fields).is_err());
    }
}
