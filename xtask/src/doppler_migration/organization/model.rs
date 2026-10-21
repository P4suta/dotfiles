use super::{
    Client, ConfigNames, Consumer, Job, Path, Purpose, Report, Repository, Result, Value, gh, git,
    inventory, names, text,
};
use anyhow::{Context, ensure};
use std::collections::{BTreeMap, BTreeSet};
use yaml_rust2::{Yaml, YamlLoader};

const APP_KEYS: [&str; 5] = [
    "RELEASE_PLZ_APP_PRIVATE_KEY",
    "RELEASE_PLEASE_APP_PRIVATE_KEY",
    "RELEASE_PLEASE_PRIVATE_KEY",
    "RELEASE_APP_PRIVATE_KEY",
    "GITHUB_APP_PRIVATE_KEY",
];
const WINDOWS: [(&str, &str); 9] = [
    ("ES_USERNAME", "SSLDOTCOM_USERNAME"),
    ("ES_PASSWORD", "SSLDOTCOM_PASSWORD"),
    ("ES_TOTP_SECRET", "SSLDOTCOM_TOTP_SECRET"),
    ("CREDENTIAL_ID", "SSLDOTCOM_CREDENTIAL_ID"),
    ("ES_CREDENTIAL_ID", "SSLDOTCOM_CREDENTIAL_ID"),
    ("SSLDOTCOM_USERNAME", "SSLDOTCOM_USERNAME"),
    ("SSLDOTCOM_PASSWORD", "SSLDOTCOM_PASSWORD"),
    ("SSLDOTCOM_TOTP_SECRET", "SSLDOTCOM_TOTP_SECRET"),
    ("SSLDOTCOM_CREDENTIAL_ID", "SSLDOTCOM_CREDENTIAL_ID"),
];
const REGISTRY: [&str; 3] = ["CARGO_TOKEN", "CARGO_REGISTRY_TOKEN", "HEXPM_API_KEY"];

impl Purpose {
    pub(super) const fn config(self) -> &'static str {
        match self {
            Self::Automation => "automation",
            Self::SignWindows => "sign_windows",
            Self::SignMacos => "sign_macos",
            Self::NotarizeMacos => "notarize_macos",
        }
    }

    pub(super) fn variable(self) -> String {
        format!("DOPPLER_{}_IDENTITY_ID", self.config().to_ascii_uppercase())
    }

    pub(super) fn fields(self) -> BTreeSet<String> {
        let fields: &[&str] = match self {
            Self::Automation => &["GITHUB_APP_CLIENT_ID", "GITHUB_APP_PRIVATE_KEY"],
            Self::SignWindows => &[
                "SSLDOTCOM_USERNAME",
                "SSLDOTCOM_PASSWORD",
                "SSLDOTCOM_CREDENTIAL_ID",
                "SSLDOTCOM_TOTP_SECRET",
            ],
            Self::SignMacos => &[
                "APPLE_CERTIFICATE",
                "APPLE_CERTIFICATE_PASSWORD",
                "APPLE_SIGNING_IDENTITY",
                "APPLE_INSTALLER_CERTIFICATE",
                "APPLE_INSTALLER_CERTIFICATE_PASSWORD",
                "APPLE_INSTALLER_IDENTITY",
                "APPLE_NOTARY_KEY",
                "APPLE_NOTARY_KEY_ID",
                "APPLE_NOTARY_ISSUER_ID",
            ],
            Self::NotarizeMacos => &[
                "APPLE_NOTARY_KEY",
                "APPLE_NOTARY_KEY_ID",
                "APPLE_NOTARY_ISSUER_ID",
            ],
        };
        fields.iter().map(|s| (*s).to_owned()).collect()
    }
}

fn repository(directory: &Path) -> Result<Repository> {
    ensure!(
        std::fs::symlink_metadata(directory)?.is_dir(),
        "checkout must be a native directory"
    );
    let remote = git(directory, &["remote", "get-url", "origin"])?;
    let name = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix(super::GITHUB_ORIGIN))
        .context("only exact github.com remotes are supported")?
        .trim_end_matches(".git");
    ensure!(
        name.split('/').count() == 2
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b)),
        "invalid repository identity"
    );
    let response = gh(&format!("repos/{name}"))?;
    let id = response
        .get("id")
        .and_then(Value::as_u64)
        .context("repository ID missing")?;
    let owner_id = response
        .pointer("/owner/id")
        .and_then(Value::as_u64)
        .context("owner ID missing")?;
    let oidc = gh(&format!("repos/{name}/actions/oidc/customization/sub"))?;
    let result = Repository {
        name: name.to_owned(),
        id,
        owner_id,
        revision: git(directory, &["rev-parse", "HEAD"])?,
        directory: directory.to_owned(),
        subject: text(&oidc, "sub_claim_prefix")?.to_owned(),
    };
    verify_repository(&result)?;
    Ok(result)
}

pub(super) fn verify_repository(repository: &Repository) -> Result<()> {
    let actual = gh(&format!("repos/{}", repository.name))?;
    ensure!(
        actual.get("id").and_then(Value::as_u64) == Some(repository.id)
            && actual.pointer("/owner/id").and_then(Value::as_u64) == Some(repository.owner_id),
        "repository identity drifted for {}",
        repository.name
    );
    let branch = text(&actual, "default_branch")?;
    ensure!(
        branch == "main",
        "an explicit policy is required for a different default branch"
    );
    let current = gh(&format!("repos/{}/git/ref/heads/{branch}", repository.name))?;
    ensure!(
        current.pointer("/object/sha").and_then(Value::as_str) == Some(&repository.revision),
        "default branch moved for {}; create a fresh organization plan",
        repository.name
    );
    ensure!(
        git(&repository.directory, &["rev-parse", "HEAD"])? == repository.revision,
        "checkout revision differs"
    );
    Ok(())
}

fn has(value: &Yaml, reference: &str) -> bool {
    match value {
        Yaml::String(s) => s.contains(reference),
        Yaml::Array(rows) => rows.iter().any(|v| has(v, reference)),
        Yaml::Hash(rows) => rows
            .iter()
            .any(|(k, v)| has(k, reference) || has(v, reference)),
        _ => false,
    }
}

pub(super) fn fetches(step: &Yaml, consumer: &Consumer) -> bool {
    step["uses"].as_str().is_some_and(|action| {
        action
            .to_ascii_lowercase()
            .starts_with("dopplerhq/secrets-fetch-action@")
    }) && step["with"]["doppler-project"].as_str() == Some(&consumer.project)
        && step["with"]["doppler-config"].as_str() == Some(&consumer.source)
}

pub(super) fn workflow_files(repository: &Repository) -> Result<Vec<(String, String)>> {
    let directory = repository.directory.join(".github/workflows");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "yml" || e == "yaml") {
            ensure!(
                std::fs::symlink_metadata(&path)?.is_file(),
                "workflow must be a native file"
            );
            let relative = path
                .strip_prefix(&repository.directory)?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, std::fs::read_to_string(path)?));
        }
    }
    files.sort();
    Ok(files)
}

fn refs(document: &Yaml) -> BTreeSet<String> {
    let mut refs = BTreeSet::from(["refs/heads/main".to_owned()]);
    if let Some(tags) = document["on"]["push"]["tags"].as_vec() {
        refs.extend(
            tags.iter()
                .filter_map(Yaml::as_str)
                .map(|tag| format!("refs/tags/{tag}")),
        );
    }
    refs
}

fn events(document: &Yaml) -> BTreeSet<String> {
    let allowed = [
        "push",
        "workflow_dispatch",
        "workflow_run",
        "schedule",
        "release",
    ];
    let on = &document["on"];
    allowed
        .into_iter()
        .filter(|name| {
            !matches!(on[*name], Yaml::BadValue)
                || on.as_str() == Some(*name)
                || on
                    .as_vec()
                    .is_some_and(|v| v.iter().any(|e| e.as_str() == Some(*name)))
        })
        .map(str::to_owned)
        .collect()
}

fn jobs(consumer: &Consumer, repository: &Repository) -> Result<Vec<Job>> {
    let mut result = Vec::new();
    for (path, source) in workflow_files(repository)? {
        if path == super::VERIFY_WORKFLOW {
            continue;
        }
        let docs = YamlLoader::load_from_str(&source)?;
        let [document] = docs.as_slice() else {
            anyhow::bail!("one workflow document required");
        };
        let rows = document["jobs"]
            .as_hash()
            .context("workflow jobs missing")?;
        for (name, job) in rows {
            let matched = consumer
                .aliases
                .keys()
                .any(|key| has(job, &format!("secrets.{key}")))
                || job["steps"]
                    .as_vec()
                    .is_some_and(|steps| steps.iter().any(|step| fetches(step, consumer)));
            let certificate =
                has(job, "secrets.APPLE_CERTIFICATE") || has(job, ".outputs.APPLE_CERTIFICATE");
            if !matched
                || (consumer.purpose == Purpose::NotarizeMacos
                    && consumer.source != Purpose::NotarizeMacos.config()
                    && certificate)
            {
                continue;
            }
            if consumer.purpose == Purpose::SignMacos && !certificate {
                continue;
            }
            let environment = job["environment"]
                .as_str()
                .or_else(|| job["environment"]["name"].as_str())
                .context("credential consumer needs a literal protected environment")?;
            ensure!(
                !environment.contains("${{"),
                "dynamic environment needs an explicit binding"
            );
            result.push(Job {
                workflow: path.clone(),
                name: name.as_str().context("invalid job ID")?.to_owned(),
                environment: environment.to_owned(),
                refs: refs(document),
                events: events(document),
            });
        }
    }
    Ok(result)
}

fn consumers(config: &ConfigNames, repository: &Repository) -> Result<Vec<Consumer>> {
    let mut rows = Vec::new();
    let found_app = APP_KEYS.iter().find(|key| config.names.contains(**key));
    if let Some(key) = found_app {
        let app = if *key == "GITHUB_APP_PRIVATE_KEY" {
            let document =
                std::fs::read_to_string(repository.directory.join("docs/credential-delivery.md"))
                    .context("canonical automation needs its documented App binding")?;
            let frontmatter = document
                .strip_prefix("---\n")
                .and_then(|body| body.split_once("\n---\n").map(|(header, _)| header))
                .context("App binding front matter missing")?;
            let documents = YamlLoader::load_from_str(frontmatter)?;
            match documents
                .first()
                .and_then(|d| d["doppler_automation_app"].as_str())
            {
                Some("release_plz") => "release_plz",
                Some("release_please") => "release_please",
                _ => anyhow::bail!("unknown canonical App binding"),
            }
        } else if config.names.iter().any(|key| key.contains("RELEASE_PLZ")) {
            "release_plz"
        } else {
            "release_please"
        };
        let mut aliases =
            BTreeMap::from([((*key).to_owned(), "GITHUB_APP_PRIVATE_KEY".to_owned())]);
        for name in &config.names {
            if name.ends_with("CLIENT_ID") || name.ends_with("APP_ID") {
                aliases.insert(name.clone(), "GITHUB_APP_CLIENT_ID".to_owned());
            }
        }
        rows.push(Consumer {
            project: config.project.clone(),
            source: config.config.clone(),
            purpose: Purpose::Automation,
            repository: repository.name.clone(),
            shared: format!("shared-automation.{app}"),
            aliases,
            jobs: Vec::new(),
        });
    }
    let aliases: BTreeMap<_, _> = WINDOWS
        .iter()
        .filter(|(old, _)| config.names.contains(*old))
        .map(|(old, new)| ((*old).to_owned(), (*new).to_owned()))
        .collect();
    if !aliases.is_empty() {
        ensure!(
            aliases.values().collect::<BTreeSet<_>>().len() == Purpose::SignWindows.fields().len(),
            "partial Windows credential set"
        );
        rows.push(Consumer {
            project: config.project.clone(),
            source: config.config.clone(),
            purpose: Purpose::SignWindows,
            repository: repository.name.clone(),
            shared: "shared-signing.windows".to_owned(),
            aliases,
            jobs: Vec::new(),
        });
    }
    if config.names.contains("APPLE_CERTIFICATE") || config.names.contains("APPLE_NOTARY_KEY") {
        let purposes: &[Purpose] = if config.names.contains("APPLE_CERTIFICATE") {
            &[Purpose::SignMacos, Purpose::NotarizeMacos]
        } else {
            &[Purpose::NotarizeMacos]
        };
        for &purpose in purposes {
            let aliases = purpose
                .fields()
                .into_iter()
                .map(|name| (name.clone(), name))
                .collect();
            rows.push(Consumer {
                project: config.project.clone(),
                source: config.config.clone(),
                purpose,
                repository: repository.name.clone(),
                shared: "shared-signing.macos".to_owned(),
                aliases,
                jobs: Vec::new(),
            });
        }
    }
    for row in &mut rows {
        row.jobs = jobs(row, repository)?;
    }
    rows.retain(|row| !row.jobs.is_empty());
    Ok(rows)
}

pub(super) fn inspect(client: &Client, workplace: &str, checkouts: &Path) -> Result<Report> {
    let mut repositories = Vec::new();
    for entry in std::fs::read_dir(checkouts)? {
        let path = entry?.path();
        if path.join(".git").exists() {
            repositories.push(repository(&path)?);
        }
    }
    repositories.sort_by(|a, b| a.name.cmp(&b.name));
    ensure!(!repositories.is_empty(), "no consumer checkouts found");
    let projects = inventory(client)?;
    let mut report = Report {
        workplace: workplace.to_owned(),
        checkouts: checkouts.to_owned(),
        repositories,
        configs: Vec::new(),
        consumers: Vec::new(),
        legacy_registry: Vec::new(),
    };
    for project in projects {
        let matched: Vec<_> = report
            .repositories
            .iter()
            .filter(|repo| {
                repo.name.split('/').next_back().is_some_and(|name| {
                    name.to_ascii_lowercase().replace('_', "-") == project.project.id
                })
            })
            .collect();
        ensure!(
            matched.len() <= 1,
            "ambiguous project to repository mapping"
        );
        for config in project.configs {
            let config = ConfigNames {
                names: names(client, &project.project.id, &config.name)?,
                project: project.project.id.clone(),
                config: config.name,
            };
            if config
                .names
                .iter()
                .any(|key| REGISTRY.contains(&key.as_str()))
            {
                report.legacy_registry.push(config.clone());
            }
            if let Some(repository) = matched.first() {
                report.consumers.extend(consumers(&config, repository)?);
            }
            report.configs.push(config);
        }
    }
    let mut unique = BTreeSet::new();
    ensure!(
        report
            .consumers
            .iter()
            .all(|c| unique.insert((c.project.clone(), c.purpose.config()))),
        "multiple configs for one purpose need an explicit reconciliation"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_oidc_delivery_is_a_consumer_without_github_secret_expressions() {
        let directory = tempfile::tempdir().unwrap();
        let workflows = directory.path().join(".github/workflows");
        std::fs::create_dir_all(&workflows).unwrap();
        std::fs::write(workflows.join("release.yml"), "on: workflow_dispatch\njobs:\n  prepare:\n    environment: release-please\n    steps:\n      - id: doppler\n        uses: dopplerhq/secrets-fetch-action@fixture\n        with:\n          auth-method: oidc\n          doppler-project: fixture\n          doppler-config: automation\n          inject-env-vars: false\n      - run: consume\n        env:\n          PRIVATE_KEY: ${{ steps.doppler.outputs.RELEASE_PLEASE_PRIVATE_KEY }}\n").unwrap();
        let repository = Repository {
            name: "owner/fixture".to_owned(),
            id: 1,
            owner_id: 2,
            revision: "fixture".to_owned(),
            directory: directory.path().to_owned(),
            subject: "repo:owner/fixture".to_owned(),
        };
        let config = ConfigNames {
            project: "fixture".to_owned(),
            config: "automation".to_owned(),
            names: BTreeSet::from([
                "RELEASE_PLEASE_CLIENT_ID".to_owned(),
                "RELEASE_PLEASE_PRIVATE_KEY".to_owned(),
            ]),
        };
        let consumers = consumers(&config, &repository).unwrap();
        assert_eq!(consumers.len(), 1);
        assert_eq!(consumers[0].jobs.len(), 1);
        assert_eq!(consumers[0].jobs[0].environment, "release-please");
    }
}
