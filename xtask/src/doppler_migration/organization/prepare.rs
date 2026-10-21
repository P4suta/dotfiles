use super::{
    Consumer, IDENTITY_LIMIT, IDENTITY_TTL, ISSUER, Purpose, Report, Result, Session,
    VERIFY_WORKFLOW, Value, evidence, field, gh, json, names, text,
};
use anyhow::{Context, ensure};
use rules::doppler_organization_rules::Change;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::Stdio;

const VARIABLE_LIMIT: u64 = 100;
const READ_BACK_ATTEMPTS: u32 = 5;
const HTTP_OK: u16 = 200;
const HTTP_NOT_FOUND: u16 = 404;
const HTTP_RATE_LIMIT: u16 = 429;
const HTTP_SERVER_ERRORS: std::ops::RangeInclusive<u16> = 500..=599;

fn required<'a>(values: &'a super::super::Secrets, name: &str) -> Result<&'a super::super::Secret> {
    let value = values
        .get(name)
        .context("required canonical credential is absent")?;
    let computed = value
        .computed
        .as_deref()
        .context("canonical credential is unreadable")?;
    ensure!(
        !(computed.trim().is_empty() || computed.starts_with("${") && computed.ends_with('}')),
        "canonical credential is empty or unresolved; value withheld"
    );
    Ok(value)
}

fn private_transfer_check(session: &Session, consumer: &Consumer) -> Result<super::super::Secrets> {
    let (project, config) = consumer
        .shared
        .split_once('.')
        .context("invalid canonical config")?;
    let canonical = super::super::secrets(&session.client, project, config)?;
    for name in consumer.purpose.fields() {
        required(&canonical, &name)?;
    }
    let existing = super::super::secrets(&session.client, &consumer.project, &consumer.source)?;
    for (old, name) in &consumer.aliases {
        let canonical_name = if consumer.purpose == Purpose::Automation && old.ends_with("APP_ID") {
            "GITHUB_APP_ID"
        } else {
            name
        };
        ensure!(
            required(&existing, old)?.computed == required(&canonical, canonical_name)?.computed,
            "existing consumer differs from the canonical credential at {}/{}/{}; values withheld",
            consumer.project,
            consumer.source,
            old
        );
    }
    Ok(canonical)
}

fn config(session: &Session, consumer: &Consumer, canonical: &super::super::Secrets) -> Result<()> {
    let config = consumer.purpose.config();
    let query = [("project", consumer.project.as_str())];
    let environments: Vec<Value> =
        session
            .client
            .list("/v3/environments", &query, "environments")?;
    if !environments
        .iter()
        .any(|e| e.get("id").and_then(Value::as_str) == Some(config))
    {
        session.change(Change::Prepare, evidence(), "/v3/environments", &[], &json!({"project":consumer.project,"slug":config,"name":config.replace('_'," "),"personal_configs":false}))?;
    }
    let current = super::super::secrets(&session.client, &consumer.project, config)?;
    let fields = consumer.purpose.fields();
    let allowed: BTreeSet<_> = fields
        .iter()
        .cloned()
        .chain(if consumer.source == config {
            consumer.aliases.keys().cloned().collect::<Vec<_>>()
        } else {
            Vec::new()
        })
        .collect();
    ensure!(
        current.keys().all(|key| allowed.contains(key)),
        "unexpected fields in target purpose config"
    );
    let mut changes = Vec::new();
    for name in &fields {
        let reference = format!("${{{}.{name}}}", consumer.shared);
        let canonical = required(canonical, name)?;
        if let Some(existing) = current.get(name) {
            ensure!(
                existing.computed == canonical.computed,
                "target purpose config conflicts; values withheld"
            );
            if existing.raw.as_deref() == Some(&reference) {
                continue;
            }
        }
        changes.push(json!({"name":name,"value":reference,"visibility":"masked","valueType":canonical.raw_value_type,"originalName":null}));
    }
    if !changes.is_empty() {
        session.change(
            Change::Prepare,
            evidence(),
            "/v3/configs/config/secrets",
            &[("project", &consumer.project), ("config", config)],
            &json!({"change_requests":changes}),
        )?;
    }
    let copied = super::super::secrets(&session.client, &consumer.project, config)?;
    for name in fields {
        let expected = format!("${{{}.{name}}}", consumer.shared);
        let actual = required(&copied, &name)?;
        ensure!(
            actual.raw.as_deref() == Some(&expected)
                && actual.computed == required(canonical, &name)?.computed,
            "purpose reference verification failed; values withheld"
        );
    }
    lock_config(session, &consumer.project, config)?;
    Ok(())
}

fn lock_config(session: &Session, project: &str, config: &str) -> Result<()> {
    let query = [("project", project)];
    let configs: Vec<super::super::Config> =
        session.client.list("/v3/configs", &query, "configs")?;
    let current = configs
        .iter()
        .find(|c| c.name == config)
        .context("purpose config disappeared")?;
    if !current.locked {
        session.change(
            Change::Prepare,
            evidence(),
            "/v3/configs/config/lock",
            &[("project", project), ("config", config)],
            &json!({}),
        )?;
    }
    let verified: Vec<super::super::Config> =
        session.client.list("/v3/configs", &query, "configs")?;
    ensure!(
        verified.iter().any(|c| c.name == config && c.locked),
        "config lock read-back differs"
    );
    Ok(())
}

fn account_name(consumer: &Consumer) -> String {
    format!(
        "{}-oidc-{}",
        consumer.project,
        consumer.purpose.config().replace('_', "-")
    )
}

fn account_path(slug: &str) -> Result<String> {
    ensure!(
        !slug.is_empty() && slug.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "invalid service account identity"
    );
    Ok(format!(
        "/v3/workplace/service_accounts/service_account/{slug}"
    ))
}

fn claims(report: &Report, consumer: &Consumer) -> Result<Value> {
    let repository = report
        .repositories
        .iter()
        .find(|r| r.name == consumer.repository)
        .context("consumer repository missing")?;
    let mut workflows = BTreeSet::from([format!(
        "{}/{VERIFY_WORKFLOW}@refs/heads/main",
        repository.name
    )]);
    let mut refs = BTreeSet::from(["refs/heads/main".to_owned()]);
    let mut events = BTreeSet::from(["workflow_dispatch".to_owned()]);
    let mut subjects = BTreeSet::new();
    for job in &consumer.jobs {
        ensure!(
            !job.refs.is_empty() && !job.events.is_empty(),
            "consumer has no bounded ref or event policy"
        );
        for reference in &job.refs {
            ensure!(
                reference == "refs/heads/main" || reference.starts_with("refs/tags/v"),
                "unsupported ref scope needs an explicit policy"
            );
            workflows.insert(format!("{}/{}@{reference}", repository.name, job.workflow));
        }
        refs.extend(job.refs.iter().cloned());
        events.extend(job.events.iter().cloned());
        subjects.insert(format!(
            "{}:environment:{}",
            repository.subject,
            job.environment.replace(':', "%3A")
        ));
    }
    let owner = repository
        .name
        .split('/')
        .next()
        .context("repository owner missing")?;
    Ok(
        json!({"aud":[format!("{}{owner}", super::GITHUB_ORIGIN)],"sub":subjects,"repository_id":[repository.id.to_string()],"repository_owner_id":[repository.owner_id.to_string()],"workflow_ref":workflows,"ref":refs,"event_name":events}),
    )
}

fn identity_payload(report: &Report, consumer: &Consumer) -> Result<Value> {
    let claims = claims(report, consumer)?;
    let wildcard = consumer
        .jobs
        .iter()
        .any(|job| job.refs.iter().any(|reference| reference.contains('*')));
    let claims_type = if wildcard { "wildcard" } else { "exact" };
    Ok(
        json!({"name":"GitHub Actions","method":"oidc","config":{"discovery_url":ISSUER,"claims_type":claims_type,"claims":claims},"ttl_seconds":IDENTITY_TTL}),
    )
}

fn account(session: &Session, consumer: &Consumer, accounts: &mut Vec<Value>) -> Result<String> {
    let name = account_name(consumer);
    let matches: Vec<_> = accounts
        .iter()
        .filter(|a| a.get("name").and_then(Value::as_str) == Some(&name))
        .collect();
    ensure!(matches.len() <= 1, "duplicate purpose service account");
    let account = if let Some(existing) = matches.first() {
        (*existing).clone()
    } else {
        let created = session.change(
            Change::Prepare,
            evidence(),
            "/v3/workplace/service_accounts",
            &[],
            &json!({"name":name,"workplace_role":{"identifier":"no_access"}}),
        )?;
        let account = field(&created, "service_account")?.clone();
        accounts.push(account.clone());
        account
    };
    ensure!(
        account
            .pointer("/workplace_role/identifier")
            .and_then(Value::as_str)
            == Some("no_access"),
        "purpose account has workplace access"
    );
    Ok(text(&account, "slug")?.to_owned())
}

fn grant(session: &Session, consumer: &Consumer, slug: &str) -> Result<()> {
    let query = [("project", consumer.project.as_str())];
    let members: Vec<Value> =
        session
            .client
            .list("/v3/projects/project/members", &query, "members")?;
    let expected = json!([consumer.purpose.config()]);
    if let Some(member) = members
        .iter()
        .find(|m| m.get("slug").and_then(Value::as_str) == Some(slug))
    {
        ensure!(
            member.pointer("/role/identifier").and_then(Value::as_str) == Some("viewer")
                && member.get("environments") == Some(&expected)
                && member
                    .get("access_all_environments")
                    .and_then(Value::as_bool)
                    == Some(false),
            "purpose account grant differs"
        );
    } else {
        session.change(Change::Prepare, evidence(), "/v3/projects/project/members", &[], &json!({"project":consumer.project,"type":"service_account","slug":slug,"role":"viewer","environments":expected}))?;
    }
    Ok(())
}

fn identity(session: &Session, report: &Report, consumer: &Consumer, slug: &str) -> Result<String> {
    let path = format!("{}/identities", account_path(slug)?);
    let identities: Vec<Value> = session.client.list(&path, &[], "identities")?;
    ensure!(
        identities.len() <= 1,
        "purpose account has extra identities"
    );
    let expected = identity_payload(report, consumer)?;
    let identity = if let Some(existing) = identities.first() {
        existing.clone()
    } else {
        let created = session.change(Change::Prepare, evidence(), &path, &[], &expected)?;
        field(&created, "identity")?.clone()
    };
    for key in ["name", "method", "config", "ttl_seconds"] {
        ensure!(
            identity.get(key) == expected.get(key),
            "purpose OIDC policy differs for {}/{}",
            consumer.project,
            consumer.purpose.config()
        );
    }
    Ok(text(&identity, "slug")?.to_owned())
}

fn gh_write(method: &str, path: &str, body: &Value) -> Result<()> {
    let mut child = super::Tool::Gh
        .command()
        .args(["api", "--method", method, path, "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().context("GitHub input unavailable")?;
    let sent = input.write_all(&serde_json::to_vec(body)?);
    drop(input);
    let output = child.wait_with_output()?;
    ensure!(
        sent.is_ok() && output.status.success(),
        "GitHub metadata write failed; output withheld"
    );
    Ok(())
}

fn variable_read_back(path: &str) -> Result<Value> {
    for attempt in 0..READ_BACK_ATTEMPTS {
        let output = super::Tool::Gh
            .command()
            .args(["api", "--include", path])
            .stdin(Stdio::null())
            .output()?;
        let response = String::from_utf8(output.stdout)?;
        let (headers, body) = response
            .split_once("\r\n\r\n")
            .or_else(|| response.split_once("\n\n"))
            .context("GitHub read-back response unavailable; output withheld")?;
        let code: u16 = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .context("GitHub status missing")?
            .parse()?;
        if code == HTTP_OK && output.status.success() {
            return serde_json::from_str(body)
                .map_err(|_| anyhow::anyhow!("invalid GitHub variable metadata; output withheld"));
        }
        ensure!(
            matches!(code, HTTP_NOT_FOUND | HTTP_RATE_LIMIT) || HTTP_SERVER_ERRORS.contains(&code),
            "GitHub read-back returned HTTP {code}; output withheld"
        );
        if attempt + 1 < READ_BACK_ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_secs(1 << attempt));
        }
    }
    anyhow::bail!("GitHub variable did not become readable after five checks")
}

fn variable(repository: &str, environment: &str, name: &str, value: &str) -> Result<()> {
    let path = format!("repos/{repository}/environments/{environment}/variables");
    let current = gh(&format!("{path}?per_page=100"))?;
    ensure!(
        current
            .get("total_count")
            .and_then(Value::as_u64)
            .is_some_and(|n| n <= VARIABLE_LIMIT),
        "variable list exceeds the supported scope"
    );
    let rows = field(&current, "variables")?
        .as_array()
        .context("variable rows unavailable")?;
    if let Some(existing) = rows
        .iter()
        .find(|v| v.get("name").and_then(Value::as_str) == Some(name))
    {
        ensure!(
            existing.get("value").and_then(Value::as_str) == Some(value),
            "existing identity variable conflicts"
        );
    } else {
        gh_write("POST", &path, &json!({"name":name,"value":value}))?;
    }
    let verified = variable_read_back(&format!("{path}/{name}"))?;
    ensure!(
        verified.get("value").and_then(Value::as_str) == Some(value),
        "identity variable read-back differs"
    );
    Ok(())
}

fn allow_fetch(repository: &str) -> Result<()> {
    let permissions = gh(&format!("repos/{repository}/actions/permissions"))?;
    ensure!(
        permissions.get("enabled").and_then(Value::as_bool) == Some(true),
        "repository Actions is disabled"
    );
    if text(&permissions, "allowed_actions")? == "all" {
        return Ok(());
    }
    ensure!(
        text(&permissions, "allowed_actions")? == "selected",
        "Action permission policy requires an explicit choice"
    );
    let path = format!("repos/{repository}/actions/permissions/selected-actions");
    let mut allowed = gh(&path)?;
    let patterns = allowed
        .get_mut("patterns_allowed")
        .and_then(Value::as_array_mut)
        .context("Action allowlist missing")?;
    if !patterns.iter().filter_map(Value::as_str).any(|pattern| {
        pattern.eq_ignore_ascii_case(super::FETCH_ACTION)
            || pattern.eq_ignore_ascii_case("dopplerhq/secrets-fetch-action@*")
    }) {
        patterns.push(json!(super::FETCH_ACTION));
        gh_write("PUT", &path, &allowed)?;
    }
    ensure!(gh(&path)? == allowed, "Action allowlist read-back differs");
    Ok(())
}

fn verify_variables(consumer: &Consumer, identity: &Value) -> Result<()> {
    for environment in consumer
        .jobs
        .iter()
        .map(|j| &j.environment)
        .collect::<BTreeSet<_>>()
    {
        let variable = gh(&format!(
            "repos/{}/environments/{environment}/variables/{}",
            consumer.repository,
            consumer.purpose.variable()
        ))?;
        ensure!(
            variable.get("value") == identity.get("slug"),
            "GitHub identity variable differs"
        );
    }
    Ok(())
}

fn capacity(session: &Session, report: &Report, accounts: &[Value]) -> Result<()> {
    let mut count = 0usize;
    let mut configured = BTreeSet::new();
    for account in accounts {
        let path = format!("{}/identities", account_path(text(account, "slug")?)?);
        let identities = session.client.list::<Value>(&path, &[], "identities")?;
        count += identities.len();
        let name = text(account, "name")?;
        if report.consumers.iter().any(|c| account_name(c) == name) {
            ensure!(
                identities.len() <= 1,
                "purpose account has extra identities"
            );
            if !identities.is_empty() {
                configured.insert(name.to_owned());
            }
        }
    }
    let missing = report
        .consumers
        .iter()
        .filter(|c| !configured.contains(&account_name(c)))
        .count();
    ensure!(
        count
            .checked_add(missing)
            .is_some_and(|total| total <= IDENTITY_LIMIT),
        "current identities and migration overlap exceed plan capacity"
    );
    Ok(())
}

pub(super) fn run(session: &Session, report: &Report) -> Result<()> {
    let mut accounts: Vec<Value> =
        session
            .client
            .list("/v3/workplace/service_accounts", &[], "service_accounts")?;
    capacity(session, report, &accounts)?;
    let mut canonical = BTreeMap::new();
    for consumer in &report.consumers {
        canonical.insert(
            (consumer.project.clone(), consumer.purpose.config()),
            private_transfer_check(session, consumer)?,
        );
    }
    for repository in report
        .consumers
        .iter()
        .map(|c| &c.repository)
        .collect::<BTreeSet<_>>()
    {
        allow_fetch(repository)?;
    }
    for consumer in &report.consumers {
        let expected = canonical
            .get(&(consumer.project.clone(), consumer.purpose.config()))
            .context("canonical transfer evidence missing")?;
        config(session, consumer, expected)?;
        let slug = account(session, consumer, &mut accounts)?;
        grant(session, consumer, &slug)?;
        let identity = identity(session, report, consumer, &slug)?;
        for environment in consumer
            .jobs
            .iter()
            .map(|job| &job.environment)
            .collect::<BTreeSet<_>>()
        {
            variable(
                &consumer.repository,
                environment,
                &consumer.purpose.variable(),
                &identity,
            )?;
            if consumer.purpose == Purpose::SignWindows
                && report.configs.iter().any(|config| {
                    config.project == consumer.project
                        && config.config == consumer.source
                        && config.names.contains("MSIX_PUBLISHER")
                })
            {
                let source =
                    super::super::secrets(&session.client, &consumer.project, &consumer.source)?;
                let publisher = required(&source, "MSIX_PUBLISHER")?
                    .computed
                    .as_deref()
                    .context("publisher metadata missing")?;
                variable(
                    &consumer.repository,
                    environment,
                    "MSIX_PUBLISHER",
                    publisher,
                )?;
            }
        }
        println!(
            "Prepared {}/{}: scoped identity and GitHub variables",
            consumer.project,
            consumer.purpose.config()
        );
    }
    verify(session, report)
}

pub(super) fn verify(session: &Session, report: &Report) -> Result<()> {
    let accounts: Vec<Value> =
        session
            .client
            .list("/v3/workplace/service_accounts", &[], "service_accounts")?;
    let mut all_members = BTreeMap::new();
    for project in super::inventory(&session.client)? {
        let members: Vec<Value> = session.client.list(
            "/v3/projects/project/members",
            &[("project", &project.project.id)],
            "members",
        )?;
        all_members.insert(project.project.id, members);
    }
    for consumer in &report.consumers {
        let name = account_name(consumer);
        let matched: Vec<_> = accounts
            .iter()
            .filter(|a| a.get("name").and_then(Value::as_str) == Some(&name))
            .collect();
        let [account] = matched.as_slice() else {
            anyhow::bail!("one purpose account required for {name}");
        };
        ensure!(
            account
                .pointer("/workplace_role/identifier")
                .and_then(Value::as_str)
                == Some("no_access"),
            "workplace grant is too broad"
        );
        let path = account_path(text(account, "slug")?)?;
        let tokens: Vec<Value> =
            session
                .client
                .list(&format!("{path}/tokens"), &[], "api_tokens")?;
        ensure!(tokens.is_empty(), "purpose account has fixed API tokens");
        let identities: Vec<Value> =
            session
                .client
                .list(&format!("{path}/identities"), &[], "identities")?;
        let [identity] = identities.as_slice() else {
            anyhow::bail!("one purpose identity required");
        };
        let expected = identity_payload(report, consumer)?;
        for key in ["name", "method", "config", "ttl_seconds"] {
            ensure!(
                identity.get(key) == expected.get(key),
                "OIDC identity differs"
            );
        }
        let actual_names = names(
            &session.client,
            &consumer.project,
            consumer.purpose.config(),
        )?;
        let mut allowed = consumer.purpose.fields();
        if consumer.source == consumer.purpose.config() {
            allowed.extend(consumer.aliases.keys().cloned());
        }
        ensure!(
            consumer.purpose.fields().is_subset(&actual_names) && actual_names.is_subset(&allowed),
            "purpose config has missing or unexpected names"
        );
        ensure!(
            all_members
                .iter()
                .all(|(project, members)| project == &consumer.project
                    || !members.iter().any(|m| m.get("slug") == account.get("slug"))),
            "purpose account has access to another project"
        );
        let members = all_members
            .get(&consumer.project)
            .context("purpose project missing")?;
        let member = members
            .iter()
            .find(|m| m.get("slug") == account.get("slug"))
            .context("purpose membership missing")?;
        ensure!(
            member.pointer("/role/identifier").and_then(Value::as_str) == Some("viewer")
                && member.get("environments") == Some(&json!([consumer.purpose.config()]))
                && member
                    .get("access_all_environments")
                    .and_then(Value::as_bool)
                    == Some(false),
            "purpose membership is too broad"
        );
        verify_variables(consumer, identity)?;
    }
    println!(
        "Verified metadata for {} purpose identities; live CI authentication remains separate",
        report.consumers.len()
    );
    Ok(())
}
