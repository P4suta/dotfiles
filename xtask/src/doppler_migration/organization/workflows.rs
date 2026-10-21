use super::{Consumer, FETCH_ACTION, Purpose, Report, Repository, Result, Tool, VERIFY_WORKFLOW};
use anyhow::{Context, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;
use std::process::Stdio;
use yaml_rust2::{Yaml, YamlLoader};

const VERIFY_NAME: &str = "Verify credential access";
const WORKFLOW_COMPONENTS: usize = 3;
const VERIFY_STEP_INDENT: usize = 6;
const APP_ACTION: &str = "actions/create-github-app-token@bcd2ba49218906704ab6c1aa796996da409d3eb1";
const APP_IDS: [&str; 8] = [
    "RELEASE_PLZ_APP_CLIENT_ID",
    "RELEASE_PLZ_APP_ID",
    "RELEASE_PLEASE_APP_CLIENT_ID",
    "RELEASE_PLEASE_CLIENT_ID",
    "RELEASE_PLEASE_APP_ID",
    "RELEASE_APP_CLIENT_ID",
    "GITHUB_APP_CLIENT_ID",
    "GITHUB_APP_ID",
];

fn contains(value: &Yaml, expression: &str) -> bool {
    match value {
        Yaml::String(s) => s.contains(expression),
        Yaml::Array(rows) => rows.iter().any(|v| contains(v, expression)),
        Yaml::Hash(rows) => rows
            .iter()
            .any(|(k, v)| contains(k, expression) || contains(v, expression)),
        _ => false,
    }
}

fn job_range(
    lines: &[String],
    name: &str,
    jobs: &yaml_rust2::yaml::Hash,
) -> Result<std::ops::Range<usize>> {
    let label = format!("{name}:");
    let start = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == label)
        .min_by_key(|(_, line)| line.len() - line.trim_start().len())
        .map(|(i, _)| i)
        .context("literal job boundary missing")?;
    let indent = lines.get(start).context("job start missing")?.len() - label.len();
    let labels: BTreeSet<_> = jobs
        .keys()
        .filter_map(Yaml::as_str)
        .map(|key| format!("{key}:"))
        .collect();
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| {
            (!line.is_empty() && !line.starts_with([' ', '#']))
                || (line.len() - line.trim_start().len() == indent && labels.contains(line.trim()))
        })
        .map_or(lines.len(), |(i, _)| i);
    Ok(start..end)
}

fn selected(step: &Yaml, consumer: &Consumer) -> bool {
    consumer
        .aliases
        .keys()
        .any(|name| contains(step, &format!("secrets.{name}")))
}

fn fetch(
    consumer: &Consumer,
    id: &str,
    indent: usize,
    condition: Option<&str>,
) -> Result<Vec<String>> {
    let prefix = " ".repeat(indent);
    let mut rows = vec![
        format!(
            "{prefix}- name: Fetch {} credentials",
            consumer.purpose.config()
        ),
        format!("{prefix}  id: {id}"),
    ];
    if let Some(condition) = condition {
        rows.push(format!(
            "{prefix}  if: {}",
            serde_json::to_string(condition)?
        ));
    }
    rows.extend([
        format!("{prefix}  uses: {FETCH_ACTION}"),
        format!("{prefix}  with:"),
        format!("{prefix}    auth-method: oidc"),
        format!(
            "{prefix}    doppler-identity-id: ${{{{ vars.{} }}}}",
            consumer.purpose.variable()
        ),
        format!("{prefix}    doppler-project: {}", consumer.project),
        format!("{prefix}    doppler-config: {}", consumer.purpose.config()),
        format!("{prefix}    inject-env-vars: false"),
    ]);
    let invalid = consumer
        .purpose
        .fields()
        .iter()
        .map(|name| {
            format!(
                "steps.{id}.outputs.{name} == '' || startsWith(steps.{id}.outputs.{name}, '${{')"
            )
        })
        .collect::<Vec<_>>()
        .join(" || ");
    rows.extend([
        format!(
            "{prefix}- name: Reject missing or unresolved {} credentials",
            consumer.purpose.config()
        ),
        format!("{prefix}  if: >-"),
        format!("{prefix}    ${{{{ steps.{id}.outcome == 'success' && ({invalid}) }}}}"),
        format!("{prefix}  shell: bash"),
        format!("{prefix}  run: exit 1"),
        String::new(),
    ]);
    Ok(rows)
}

fn permission_lines(job: &Yaml, document: &Yaml, indent: usize) -> Result<Vec<String>> {
    let value = if matches!(job["permissions"], Yaml::BadValue) {
        &document["permissions"]
    } else {
        &job["permissions"]
    };
    let mut permissions = BTreeMap::new();
    match value {
        Yaml::BadValue => {
            permissions.insert("contents".to_owned(), "read".to_owned());
        }
        Yaml::Hash(entries) => {
            for (key, value) in entries {
                permissions.insert(
                    key.as_str().context("invalid permission name")?.to_owned(),
                    value
                        .as_str()
                        .context("invalid permission value")?
                        .to_owned(),
                );
            }
        }
        _ => anyhow::bail!("broad or dynamic permissions require an explicit job policy"),
    }
    permissions.insert("id-token".to_owned(), "write".to_owned());
    let prefix = " ".repeat(indent);
    let mut lines = vec![format!("{prefix}permissions:")];
    lines.extend(
        permissions
            .into_iter()
            .map(|(key, value)| format!("{prefix}  {key}: {value}")),
    );
    Ok(lines)
}

fn replace_step(lines: &[String], step: &Yaml, consumer: &Consumer, id: &str) -> Vec<String> {
    let mut rows = lines.to_vec();
    let app = step["uses"]
        .as_str()
        .is_some_and(|action| action.starts_with("actions/create-github-app-token@"));
    for row in &mut rows {
        for (old, new) in &consumer.aliases {
            *row = row.replace(
                &format!("secrets.{old}"),
                &format!("steps.{id}.outputs.{new}"),
            );
        }
        if consumer.purpose == Purpose::Automation {
            for name in APP_IDS {
                for origin in ["secrets", "vars"] {
                    *row = row.replace(
                        &format!("{origin}.{name}"),
                        &format!("steps.{id}.outputs.GITHUB_APP_CLIENT_ID"),
                    );
                }
            }
            if app && row.trim_start().starts_with("app-id:") {
                *row = row.replacen("app-id:", "client-id:", 1);
            }
        }
        if consumer.purpose == Purpose::SignWindows {
            *row = row.replace("secrets.MSIX_PUBLISHER", "vars.MSIX_PUBLISHER");
        }
    }
    rows
}

fn condition(step: &Yaml) -> Result<Option<String>> {
    match &step["if"] {
        Yaml::BadValue => Ok(None),
        Yaml::String(value) => Ok(Some(value.clone())),
        Yaml::Boolean(value) => Ok(Some(value.to_string())),
        _ => anyhow::bail!("unsupported step condition"),
    }
}

fn availability_flag(value: &str, consumers: &[&Consumer]) -> Result<Option<String>> {
    if !consumers.iter().any(|consumer| {
        consumer
            .aliases
            .keys()
            .any(|name| value.contains(&format!("secrets.{name}")))
    }) {
        return Ok(None);
    }
    let expression = value
        .trim()
        .strip_prefix("${{")
        .and_then(|value| value.strip_suffix("}}"))
        .context("credential readiness needs an explicit expression")?;
    let mut terms = Vec::new();
    for term in expression.split("&&") {
        let term = term.trim();
        let words: Vec<_> = term.split_whitespace().collect();
        let replacement = if let [reference, "!=", "''" | "\"\""] = words.as_slice() {
            if let Some(name) = reference.strip_prefix("secrets.") {
                consumers
                    .iter()
                    .find(|c| c.aliases.contains_key(name))
                    .map_or_else(
                        || term.to_owned(),
                        |consumer| format!("vars.{} != ''", consumer.purpose.variable()),
                    )
            } else {
                term.to_owned()
            }
        } else {
            ensure!(
                !term.contains("secrets."),
                "credential readiness must check named nonempty fields"
            );
            term.to_owned()
        };
        if !terms.contains(&replacement) {
            terms.push(replacement);
        }
    }
    Ok(Some(format!("${{{{ {} }}}}", terms.join(" && "))))
}

fn availability_flags(header: &mut Vec<String>, job: &Yaml, consumers: &[&Consumer]) -> Result<()> {
    if let Some(environment) = job["env"].as_hash() {
        for (name, value) in environment {
            let Some(value) = value.as_str() else {
                continue;
            };
            let Some(replacement) = availability_flag(value, consumers)? else {
                continue;
            };
            let label = format!("{}:", name.as_str().context("invalid environment field")?);
            let start = header
                .iter()
                .position(|line| line.trim_start().starts_with(&label))
                .context("environment field boundary missing")?;
            let line = header.get(start).context("environment line missing")?;
            let indent = line.len() - line.trim_start().len();
            let end = header
                .iter()
                .enumerate()
                .skip(start + 1)
                .find(|(_, line)| {
                    !line.trim().is_empty() && line.len() - line.trim_start().len() <= indent
                })
                .map_or(header.len(), |(index, _)| index);
            header.splice(
                start..end,
                [format!("{}{label} {replacement}", " ".repeat(indent))],
            );
        }
    }
    ensure!(
        !consumers.iter().any(|c| c.aliases.keys().any(|name| header
            .iter()
            .any(|line| line.contains(&format!("secrets.{name}"))))),
        "job header still needs a legacy credential; migrate that boundary explicitly"
    );
    Ok(())
}

fn job_header(
    lines: &[String],
    steps_line: usize,
    job: &Yaml,
    document: &Yaml,
) -> Result<Vec<String>> {
    let first = lines.first().context("job is empty")?;
    let permission_indent = first.len() - first.trim_start().len() + 2;
    let mut header = lines
        .get(..=steps_line)
        .context("steps header missing")?
        .to_vec();
    if let Some(start) = header.iter().position(|line| {
        line.len() - line.trim_start().len() == permission_indent
            && line.trim_start().starts_with("permissions:")
    }) {
        let end = header
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, line)| {
                !line.trim().is_empty()
                    && !line.trim_start().starts_with('#')
                    && line.len() - line.trim_start().len() <= permission_indent
            })
            .map_or(header.len(), |(i, _)| i);
        let replacement = permission_lines(job, document, permission_indent)?;
        if job["permissions"]
            .as_hash()
            .is_some_and(|permissions| !permissions.is_empty())
            && header[start].trim() == "permissions:"
        {
            if let Some(index) = header
                .iter()
                .enumerate()
                .take(end)
                .skip(start + 1)
                .find(|(_, line)| line.trim_start().starts_with("id-token:"))
                .map(|(index, _)| index)
            {
                let value = job["permissions"]["id-token"]
                    .as_str()
                    .context("invalid OIDC permission")?;
                let line = header
                    .get_mut(index)
                    .context("OIDC permission line missing")?;
                let original = format!("id-token: {value}");
                ensure!(
                    line.contains(&original),
                    "OIDC permission needs a literal value"
                );
                *line = line.replacen(&original, "id-token: write", 1);
            } else {
                header.insert(
                    start + 1,
                    format!("{}id-token: write", " ".repeat(permission_indent + 2)),
                );
            }
        } else {
            header.splice(start..end, replacement);
        }
    } else {
        header.splice(1..1, permission_lines(job, document, permission_indent)?);
    }
    Ok(header)
}

fn transform_job(
    lines: &[String],
    job: &Yaml,
    document: &Yaml,
    consumers: &[&Consumer],
) -> Result<Vec<String>> {
    let steps = job["steps"]
        .as_vec()
        .context("credential job needs inline steps")?;
    let steps_line = lines
        .iter()
        .position(|line| line.trim() == "steps:")
        .context("steps boundary missing")?;
    let sequence_indent = lines
        .iter()
        .skip(steps_line + 1)
        .find(|line| line.trim_start().starts_with("- "))
        .map(|line| line.len() - line.trim_start().len())
        .context("step sequence missing")?;
    let starts: Vec<_> = lines
        .iter()
        .enumerate()
        .skip(steps_line + 1)
        .filter(|(_, line)| {
            line.len() - line.trim_start().len() == sequence_indent
                && line.trim_start().starts_with("- ")
        })
        .map(|(i, _)| i)
        .collect();
    ensure!(
        starts.len() == steps.len(),
        "text boundaries differ from parsed steps"
    );
    let mut result = job_header(lines, steps_line, job, document)?;
    availability_flags(&mut result, job, consumers)?;
    let mut existing_outputs = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let start = *starts.get(index).context("step start missing")?;
        let end = starts.get(index + 1).copied().unwrap_or(lines.len());
        let original = lines.get(start..end).context("step range missing")?;
        let existing: Vec<_> = consumers
            .iter()
            .copied()
            .filter(|consumer| super::model::fetches(step, consumer))
            .collect();
        ensure!(
            existing.len() <= 1,
            "an existing fetch mixes credential purposes"
        );
        if let Some(consumer) = existing.first() {
            ensure!(
                step["with"]["inject-env-vars"].as_bool() == Some(false)
                    || step["with"]["inject-env-vars"].as_str() == Some("false"),
                "global credential injection requires an explicit consumer migration"
            );
            let id = step["id"]
                .as_str()
                .context("existing fetch requires an output step ID")?;
            let condition = condition(step)?;
            result.extend(fetch(consumer, id, sequence_indent, condition.as_deref())?);
            existing_outputs.push((id.to_owned(), *consumer));
            continue;
        }
        let matched: Vec<_> = consumers
            .iter()
            .copied()
            .filter(|consumer| selected(step, consumer))
            .collect();
        ensure!(
            matched.len() <= 1,
            "one step mixes credential purposes; split its consumer first"
        );
        if let Some(consumer) = matched.first() {
            let id = format!("doppler_{}_{}", consumer.purpose.config(), index);
            ensure!(
                !contains(job, &format!("id: {id}"))
                    && !steps.iter().any(|s| s["id"].as_str() == Some(&id)),
                "generated step ID conflicts"
            );
            let condition = condition(step)?;
            result.extend(fetch(consumer, &id, sequence_indent, condition.as_deref())?);
            result.extend(replace_step(original, step, consumer, &id));
        } else {
            result.extend(original.iter().cloned());
        }
    }
    for (id, consumer) in existing_outputs {
        for line in &mut result {
            for (old, new) in &consumer.aliases {
                *line = line.replace(
                    &format!("steps.{id}.outputs.{old}"),
                    &format!("steps.{id}.outputs.{new}"),
                );
            }
        }
    }
    if consumers.iter().any(|c| c.purpose == Purpose::SignWindows) {
        for line in &mut result {
            *line = line.replace("secrets.MSIX_PUBLISHER", "vars.MSIX_PUBLISHER");
        }
    }
    Ok(result)
}

fn transform(source: &str, path: &str, consumers: &[&Consumer]) -> Result<String> {
    let documents = YamlLoader::load_from_str(source)?;
    let [document] = documents.as_slice() else {
        anyhow::bail!("one workflow document required");
    };
    let jobs = document["jobs"]
        .as_hash()
        .context("workflow jobs missing")?;
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let mut changes = Vec::new();
    for (name, job) in jobs {
        let name = name.as_str().context("job ID missing")?;
        let selected: Vec<_> = consumers
            .iter()
            .copied()
            .filter(|c| c.jobs.iter().any(|j| j.workflow == path && j.name == name))
            .collect();
        if selected.is_empty() {
            continue;
        }
        let range = job_range(&lines, name, jobs)?;
        let rewritten = transform_job(
            lines.get(range.clone()).context("job range missing")?,
            job,
            document,
            &selected,
        )?;
        changes.push((range, rewritten));
    }
    changes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in changes {
        lines.splice(range, replacement);
    }
    let result = format!("{}\n", lines.join("\n"));
    YamlLoader::load_from_str(&result)?;
    Ok(result)
}

fn baseline(repository: &Repository, path: &str) -> Result<String> {
    ensure!(
        path.starts_with(".github/workflows/")
            && path.split('/').count() == WORKFLOW_COMPONENTS
            && Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("yml")
                    || extension.eq_ignore_ascii_case("yaml")),
        "invalid workflow path"
    );
    let output = Tool::Git
        .command()
        .current_dir(&repository.directory)
        .args(["show", &format!("{}:{path}", repository.revision)])
        .stdin(Stdio::null())
        .output()?;
    ensure!(output.status.success(), "baseline workflow unavailable");
    String::from_utf8(output.stdout).context("workflow is not UTF-8")
}

fn write_checked(path: &Path, baseline: Option<&str>, replacement: &str) -> Result<()> {
    if path.exists() {
        let current = std::fs::read_to_string(path)?;
        ensure!(
            Some(current.as_str()) == baseline || current == replacement,
            "unrelated local edit conflicts at {}",
            path.display()
        );
        if current == replacement {
            return Ok(());
        }
    } else {
        ensure!(baseline.is_none(), "tracked workflow disappeared");
        std::fs::create_dir_all(path.parent().context("output parent missing")?)?;
    }
    std::fs::write(path, replacement)
        .with_context(|| format!("writing {} failed", path.display()))?;
    Ok(())
}

fn verification(report: &Report, consumers: &[&Consumer]) -> Result<String> {
    let mut source = format!(
        "name: {VERIFY_NAME}\n\non:\n  workflow_dispatch:\n\npermissions: {{}}\n\nconcurrency:\n  group: verify-credentials\n  cancel-in-progress: false\n\njobs:\n"
    );
    for consumer in consumers {
        for (index, environment) in consumer
            .jobs
            .iter()
            .map(|j| &j.environment)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .enumerate()
        {
            write!(
                source,
                "  {}_{index}:\n    name: Verify {} access\n    if: github.ref == 'refs/heads/main'\n    runs-on: ubuntu-24.04\n    timeout-minutes: 5\n    environment: {environment}\n    permissions:\n      id-token: write\n    steps:\n",
                consumer.purpose.config(),
                consumer.purpose.config()
            )?;
            source.push_str(&fetch(consumer, "credentials", VERIFY_STEP_INDENT, None)?.join("\n"));
            source.push('\n');
            if consumer.purpose == Purpose::Automation {
                write!(
                    source,
                    "      - name: Verify a read-only repository token\n        uses: {APP_ACTION}\n        with:\n          client-id: ${{{{ steps.credentials.outputs.GITHUB_APP_CLIENT_ID }}}}\n          private-key: ${{{{ steps.credentials.outputs.GITHUB_APP_PRIVATE_KEY }}}}\n          owner: ${{{{ github.repository_owner }}}}\n          repositories: ${{{{ github.event.repository.name }}}}\n          permission-contents: read\n\n"
                )?;
            }
            let forbidden = report
                .consumers
                .iter()
                .find(|c| c.project == consumer.project && c.purpose != consumer.purpose)
                .or_else(|| {
                    report
                        .consumers
                        .iter()
                        .find(|c| c.project != consumer.project)
                })
                .context("a negative access target is required")?;
            write!(
                source,
                "      - name: Attempt unrelated credential access\n        id: forbidden\n        continue-on-error: true\n        uses: {FETCH_ACTION}\n        with:\n          auth-method: oidc\n          doppler-identity-id: ${{{{ vars.{} }}}}\n          doppler-project: {}\n          doppler-config: {}\n          inject-env-vars: false\n\n      - name: Reject unrelated credential access\n        if: ${{{{ always() && steps.forbidden.outcome != 'failure' }}}}\n        shell: bash\n        run: exit 1\n\n",
                consumer.purpose.variable(),
                forbidden.project,
                forbidden.purpose.config()
            )?;
        }
    }
    YamlLoader::load_from_str(&source)?;
    Ok(source)
}

pub(super) fn rewrite(report: &Report) -> Result<()> {
    for repository in &report.repositories {
        let consumers: Vec<_> = report
            .consumers
            .iter()
            .filter(|c| c.repository == repository.name)
            .collect();
        if consumers.is_empty() {
            continue;
        }
        for (path, current) in super::model::workflow_files(repository)? {
            let docs = YamlLoader::load_from_str(&current)?;
            let document = docs.first().context("empty workflow")?;
            if let Some(watched) = document["on"]["workflow_run"]["workflows"].as_vec() {
                ensure!(
                    !watched
                        .iter()
                        .filter_map(Yaml::as_str)
                        .any(|name| name == "*" || name == VERIFY_NAME),
                    "authentication verification would trigger another workflow"
                );
            }
            if path == VERIFY_WORKFLOW {
                continue;
            }
            if !consumers
                .iter()
                .any(|c| c.jobs.iter().any(|job| job.workflow == path))
            {
                continue;
            }
            let original = baseline(repository, &path)?;
            let replacement = transform(&original, &path, &consumers)?;
            if original != replacement {
                write_checked(
                    &repository.directory.join(&path),
                    Some(&original),
                    &replacement,
                )?;
            }
        }
        let path = repository.directory.join(VERIFY_WORKFLOW);
        let tracked = super::git(
            &repository.directory,
            &[
                "ls-tree",
                "--name-only",
                &repository.revision,
                "--",
                VERIFY_WORKFLOW,
            ],
        )?;
        let original = if tracked.is_empty() {
            None
        } else {
            Some(baseline(repository, VERIFY_WORKFLOW)?)
        };
        write_checked(
            &path,
            original.as_deref(),
            &verification(report, &consumers)?,
        )?;
        write_checked(
            &repository.directory.join("docs/credential-delivery.md"),
            None,
            &documentation(&consumers)?,
        )?;
        println!(
            "Prepared OIDC workflows in {}; nothing committed or dispatched",
            repository.name
        );
    }
    Ok(())
}

fn documentation(consumers: &[&Consumer]) -> Result<String> {
    let mut document = String::new();
    if let Some(automation) = consumers.iter().find(|c| c.purpose == Purpose::Automation) {
        let app = automation
            .shared
            .strip_prefix("shared-automation.")
            .context("unknown App source")?;
        write!(document, "---\ndoppler_automation_app: {app}\n---\n\n")?;
    }
    document.push_str("# Credential delivery\n\nGitHub Actions exchanges its workload identity for a short-lived Doppler token at the credential-consuming step.\nEach purpose has its own read-only service account, config, and environment variable.\nShared signing credentials stay in the canonical signing project and are referenced from the consumer config.\nThe shared credential still carries the signing provider's authority after retrieval.\n\n| Purpose | Doppler config | GitHub environment variable | Environments |\n| --- | --- | --- | --- |\n");
    for consumer in consumers {
        let environments = consumer
            .jobs
            .iter()
            .map(|j| j.environment.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            document,
            "| {} | `{}.{}` | `{}` | {} |",
            consumer.purpose.config(),
            consumer.project,
            consumer.purpose.config(),
            consumer.purpose.variable(),
            environments
        )?;
    }
    document.push_str("\nThe manual **Verify credential access** workflow checks required fields and rejects access to an unrelated config.\nAutomation also creates a repository-scoped App token with contents read access.\nThis workflow performs no signing, notarization, registry publication, release creation, or deployment.\nExisting environment protections remain in force; their approval does not grant authority for another operation.\n\nAfter the reviewed workflows reach the default branch, run the credential-access check for that exact revision.\nKeep the current credential delivery until every purpose and environment succeeds and the denial checks pass.\nThen disconnect the corresponding Doppler-to-GitHub sync while preserving its target fields, check for remaining consumers, and remove the obsolete GitHub fields and legacy configs.\nAn expired or revoked registry key is a retirement candidate and must not be copied into a replacement config.\nUse the registry's native trusted publisher where supported; an unconfigured publisher stays unavailable until its owner configures the exact binding.\n\nStore only field names, identity metadata, source revisions, and verification outcomes in issues and logs.\nKeep credential values and native authentication state out of the repository.\n");
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_a_new_document_creates_its_parent_and_preserves_unrelated_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("docs/credential-delivery.md");
        write_checked(&path, None, "prepared").unwrap();
        write_checked(&path, None, "prepared").unwrap();
        std::fs::write(&path, "owner edit").unwrap();
        assert!(write_checked(&path, None, "prepared").is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "owner edit");
    }

    fn consumer() -> Consumer {
        Consumer {
            project: "fixture".to_owned(),
            source: "release".to_owned(),
            purpose: Purpose::SignWindows,
            repository: "owner/fixture".to_owned(),
            shared: "shared-signing.windows".to_owned(),
            aliases: BTreeMap::from([("ES_PASSWORD".to_owned(), "SSLDOTCOM_PASSWORD".to_owned())]),
            jobs: vec![super::super::Job {
                workflow: ".github/workflows/release.yml".to_owned(),
                name: "sign".to_owned(),
                environment: "release".to_owned(),
                refs: BTreeSet::new(),
                events: BTreeSet::new(),
            }],
        }
    }

    #[test]
    fn job_availability_flags_use_the_identity_instead_of_retired_github_copies() {
        let source = "on: workflow_dispatch\npermissions: {}\njobs:\n  sign:\n    environment: release\n    env:\n      HAVE_SIGNING: ${{ secrets.ES_PASSWORD != '' && vars.MSIX_PUBLISHER != '' }}\n    steps:\n      - if: env.HAVE_SIGNING == 'true'\n        run: sign\n        env:\n          PASSWORD: ${{ secrets.ES_PASSWORD }}\n";
        let consumer = consumer();
        let result = transform(source, ".github/workflows/release.yml", &[&consumer]).unwrap();
        assert!(!result.contains("secrets.ES_PASSWORD"));
        assert!(
            result.contains(
                "vars.DOPPLER_SIGN_WINDOWS_IDENTITY_ID != '' && vars.MSIX_PUBLISHER != ''"
            )
        );
        assert!(result.contains("env.HAVE_SIGNING == 'true'"));
    }

    #[test]
    fn fetch_precedes_only_its_conditioned_consumer_and_keeps_effects_and_permissions() {
        let source = "name: Fixture\non: workflow_dispatch\npermissions:\n  contents: read\njobs:\n  sign:\n    environment: release\n    steps:\n      - name: Inspect artifact\n        run: verify-artifact\n      # Preserve the signing boundary.\n      - name: Sign artifact\n        if: runner.os == 'Windows'\n        env:\n          PASSWORD: ${{ secrets.ES_PASSWORD }}\n        run: sign-artifact\n";
        let consumer = consumer();
        let result = transform(source, ".github/workflows/release.yml", &[&consumer]).unwrap();
        assert!(
            result.find("verify-artifact").unwrap() < result.find("Fetch sign_windows").unwrap()
        );
        assert!(
            result.find("Fetch sign_windows").unwrap()
                < result.find("name: Sign artifact").unwrap()
        );
        assert!(result.contains("# Preserve the signing boundary."));
        let docs = YamlLoader::load_from_str(&result).unwrap();
        let job = &docs[0]["jobs"]["sign"];
        assert_eq!(job["permissions"]["contents"].as_str(), Some("read"));
        assert_eq!(job["permissions"]["id-token"].as_str(), Some("write"));
        let steps = job["steps"].as_vec().unwrap();
        assert_eq!(steps[1]["if"].as_str(), Some("runner.os == 'Windows'"));
        assert_eq!(steps[3]["run"].as_str(), Some("sign-artifact"));
        assert!(result.contains("steps.doppler_sign_windows_1.outputs.SSLDOTCOM_PASSWORD"));
        assert!(!result.contains("secrets.ES_PASSWORD"));
    }

    #[test]
    fn multiple_steps_fetch_at_their_own_boundary_without_a_global_environment() {
        let source = "on: workflow_dispatch\npermissions: {}\njobs:\n  sign:\n    permissions: {}\n    environment: release\n    steps:\n      - run: first\n        env:\n          PASSWORD: ${{ secrets.ES_PASSWORD }}\n      - run: second\n        env:\n          PASSWORD: ${{ secrets.ES_PASSWORD }}\n";
        let consumer = consumer();
        let result = transform(source, ".github/workflows/release.yml", &[&consumer]).unwrap();
        assert_eq!(result.matches("auth-method: oidc").count(), 2);
        assert_eq!(result.matches("inject-env-vars: false").count(), 2);
        assert!(result.contains("doppler_sign_windows_0.outputs.SSLDOTCOM_PASSWORD"));
        assert!(result.contains("doppler_sign_windows_1.outputs.SSLDOTCOM_PASSWORD"));
    }

    #[test]
    fn an_existing_oidc_fetch_uses_the_same_normalized_contract_and_keeps_output_references() {
        let source = "on: workflow_dispatch\npermissions: {}\njobs:\n  sign:\n    environment: release\n    steps:\n      - id: doppler\n        uses: dopplerhq/secrets-fetch-action@fixture\n        with:\n          auth-method: oidc\n          doppler-project: fixture\n          doppler-config: windows\n          doppler-identity-id: ${{ vars.LEGACY_ID }}\n          inject-env-vars: false\n      - run: sign-artifact\n        env:\n          PASSWORD: ${{ steps.doppler.outputs.ES_PASSWORD }}\n";
        let consumer = Consumer {
            source: "windows".to_owned(),
            ..consumer()
        };
        let result = transform(source, ".github/workflows/release.yml", &[&consumer]).unwrap();
        assert_eq!(result.matches("auth-method: oidc").count(), 1);
        assert!(result.contains("doppler-config: sign_windows"));
        assert!(result.contains("vars.DOPPLER_SIGN_WINDOWS_IDENTITY_ID"));
        assert!(result.contains("steps.doppler.outputs.SSLDOTCOM_PASSWORD"));
        assert!(!result.contains("LEGACY_ID"));
        assert!(result.contains("run: sign-artifact"));
    }
}
