use super::{Github, Target, branch, command, evidence, git_output};
use anyhow::{Context, Result, ensure};
use rules::wip_rules::{StackMember, Workload, admits_creation, workload};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read as _;
use std::path::PathBuf;

const PAGE_SIZE: usize = 100;
const RECORD_KIND: &str = "independent";
const RECORD_VERSION: u32 = 1;
const REASON_BYTES: usize = 1024;
const RECORD_BYTES: u64 = 8192;
const COMMIT_HEX_DIGITS: usize = 40;

#[derive(Deserialize)]
struct Pull {
    number: u64,
    html_url: String,
    user: User,
    head: Head,
    base: Base,
}

#[derive(Deserialize)]
struct User {
    login: String,
}

#[derive(Deserialize)]
struct Head {
    #[serde(rename = "ref")]
    name: String,
    sha: String,
    repo: Option<Repository>,
}

#[derive(Deserialize)]
struct Base {
    #[serde(rename = "ref")]
    name: String,
}

#[derive(Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    schema: u32,
    repository_id: u64,
    login: String,
    source_id: u64,
    source: String,
    branch: String,
    issue: Option<u64>,
    started_at: String,
    pr: Option<u64>,
    reason: String,
}

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Start,
    Create,
}

pub(super) struct Plan {
    base: String,
    stacked: bool,
    record_path: PathBuf,
    declaration: Option<Declaration>,
    source: platform::source_identity::Checked,
    head: String,
    prepared_at: String,
}

impl Plan {
    pub(super) fn base(&self) -> &str {
        &self.base
    }

    pub(super) fn stacked(&self) -> bool {
        self.stacked
    }

    pub(super) fn before_publication(&self) -> Result<()> {
        self.source.require_current()?;
        require_head(&self.head, &self.prepared_at)?;
        if let Some(declaration) = &self.declaration {
            save(&self.record_path, declaration)?;
        }
        Ok(())
    }

    pub(super) fn started(&self) -> Result<()> {
        self.before_publication()?;
        if self.declaration.is_none()
            && load(&self.record_path)?.is_some_and(|record| record.pr.is_none())
        {
            platform::cleanup::remove_file(&self.record_path);
        }
        Ok(())
    }

    pub(super) fn published(&self, number: u64) -> Result<()> {
        if let Some(record) = &self.declaration {
            let mut record = record.clone();
            record.pr = Some(number);
            save(&self.record_path, &record)?;
        }
        Ok(())
    }
}

fn record_path(target: &Target, login: &str, head: &str) -> Result<PathBuf> {
    let digest = Sha256::digest(format!("{}\0{head}", login.to_ascii_lowercase()).as_bytes());
    Ok(
        evidence::directory(&evidence::home()?, RECORD_KIND, &target.repo)?
            .join(format!("{}.json", platform::build_inputs::hex(&digest))),
    )
}

fn load(path: &std::path::Path) -> Result<Option<Declaration>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) => ensure!(
            metadata.is_file(),
            "independent work records must be regular files"
        ),
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(RECORD_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= RECORD_BYTES,
        "independent work record exceeds its bound"
    );
    let record: Declaration =
        serde_json::from_slice(&bytes).context("malformed independent work record")?;
    ensure!(
        record.schema == RECORD_VERSION,
        "unsupported independent work record version"
    );
    ensure!(
        record.repository_id != 0
            && record.source_id != 0
            && record.pr != Some(0)
            && full_oid(&record.started_at),
        "invalid independent work identity"
    );
    branch(&record.branch)?;
    reason(&record.reason).map_err(anyhow::Error::msg)?;
    Ok(Some(record))
}

fn save(path: &std::path::Path, record: &Declaration) -> Result<()> {
    let bytes = serde_json::to_vec(record)?;
    ensure!(
        u64::try_from(bytes.len())? <= RECORD_BYTES,
        "independent work record exceeds its bound"
    );
    evidence::write(path, &bytes)
}

pub(super) fn reason(value: &str) -> Result<String, String> {
    if value.trim().is_empty() || value.len() > REASON_BYTES || value.chars().any(char::is_control)
    {
        Err(
            "an independent work reason must be nonempty, single-line, and at most 1024 bytes"
                .into(),
        )
    } else {
        Ok(value.to_owned())
    }
}

fn branch_head(head: &str) -> Result<String> {
    branch(head)?;
    let reference = format!("refs/heads/{head}");
    Ok(
        git_output(&["rev-parse", "--verify", &format!("{reference}^{{commit}}")])?
            .trim()
            .to_owned(),
    )
}

fn full_oid(value: &str) -> bool {
    value.len() == COMMIT_HEX_DIGITS && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn require_head(head: &str, expected: &str) -> Result<()> {
    ensure!(
        branch_head(head)? == expected,
        "work branch changed after its live inventory check"
    );
    Ok(())
}

fn pulls(github: &Github, target: &Target) -> Result<Vec<Pull>> {
    let mut mine = Vec::new();
    let mut identities = BTreeSet::new();
    for page in 1_u32..=u32::MAX {
        let endpoint = format!(
            "repos/{}/pulls?state=open&per_page={PAGE_SIZE}&page={page}",
            target.repo
        );
        let listed: Vec<Pull> = github.read(&endpoint)?;
        let complete = listed.len() < PAGE_SIZE;
        for pull in listed {
            ensure!(
                pull.number != 0 && identities.insert(pull.number),
                "duplicate or invalid open PR identity"
            );
            if pull.user.login.eq_ignore_ascii_case(&github.login) {
                ensure!(
                    pull.html_url.eq_ignore_ascii_case(&format!(
                        "{}/{}/pull/{}",
                        super::GITHUB,
                        target.repo,
                        pull.number
                    )),
                    "open PR destination differs"
                );
                branch(&pull.head.name)?;
                branch(&pull.base.name)?;
                ensure!(
                    full_oid(&pull.head.sha),
                    "open PR head is not a full commit identity"
                );
                ensure!(
                    pull.head
                        .repo
                        .as_ref()
                        .is_some_and(|repository| repository.id != 0),
                    "open PR source repository is unavailable"
                );
                mine.push(pull);
            }
        }
        if complete {
            return Ok(mine);
        }
    }
    anyhow::bail!("open PR pagination overflow")
}

fn declaration_matches(
    record: &Declaration,
    github: &Github,
    repository_id: u64,
    pull: &Pull,
) -> bool {
    record.repository_id == repository_id
        && record.login.eq_ignore_ascii_case(&github.login)
        && record.branch == pull.head.name
        && record.pr == Some(pull.number)
        && pull.head.repo.as_ref().is_some_and(|source| {
            source.id == record.source_id && source.full_name.eq_ignore_ascii_case(&record.source)
        })
}

struct Inventory {
    work: Workload,
    evidence: Vec<String>,
}

fn inventory(
    github: &Github,
    target: &Target,
    mine: &[Pull],
    active: Option<&Declaration>,
) -> Result<Inventory> {
    let repository_id = github.repository(&target.repo)?.id;
    let mut heads = BTreeSet::new();
    let mut members = Vec::new();
    let mut evidence = Vec::new();
    for pull in mine {
        ensure!(
            heads.insert(&pull.head.name),
            "ambiguous duplicate owned PR branch"
        );
        let stored = load(&record_path(target, &github.login, &pull.head.name)?)?;
        let record = active
            .filter(|record| record.branch == pull.head.name)
            .or(stored.as_ref());
        let independent =
            record.is_some_and(|record| declaration_matches(record, github, repository_id, pull));
        let parent = mine.iter().position(|parent| {
            parent.head.name == pull.base.name
                && parent.head.repo.as_ref().map(|source| source.id)
                    == pull.head.repo.as_ref().map(|source| source.id)
        });
        members.push(StackMember {
            parent,
            independent,
        });
        let mut description = format!(
            "#{} {} ({}) on {}",
            pull.number, pull.html_url, pull.head.name, pull.base.name
        );
        if independent {
            description.push_str("; independent: ");
            description.push_str(&record.context("independent work record missing")?.reason);
        }
        println!("{description}");
        evidence.push(description);
    }
    let work = workload(&members).map_err(|invalid| {
        let mut refusal = super::Refusal::new(
            "pr.work-units",
            format!("open work is not one stack plus declared independent PRs: {invalid:?}"),
            command(&[
                "pr-workflow",
                "start",
                "--repo",
                &target.repo,
                "--independent",
                "<reason>",
            ]),
        );
        for line in &evidence {
            refusal = refusal.evidence(line.clone());
        }
        refusal
    })?;
    println!(
        "Open work: {} unit(s), {} owned PR(s)",
        work.units(),
        mine.len()
    );
    Ok(Inventory { work, evidence })
}

fn pending(
    github: &Github,
    target: &Target,
    head: &str,
    source: &platform::source_identity::Checked,
    independent: Option<&str>,
    operation: Operation,
    prepared_at: &str,
) -> Result<Option<Declaration>> {
    let repository_id = github.repository(&target.repo)?.id;
    let origin = github.repository(source.binding().source())?;
    let stored = load(&record_path(target, &github.login, head)?)?;
    if let Some(reason) = independent {
        return Ok(Some(Declaration {
            schema: RECORD_VERSION,
            repository_id,
            login: github.login.clone(),
            source_id: origin.id,
            source: origin.full_name,
            branch: head.to_owned(),
            issue: target.issue,
            started_at: prepared_at.to_owned(),
            pr: None,
            reason: reason.to_owned(),
        }));
    }
    if matches!(operation, Operation::Create)
        && let Some(record) = stored
        && record.pr.is_none()
    {
        ensure!(
            record.repository_id == repository_id
                && record.login.eq_ignore_ascii_case(&github.login)
                && record.source_id == origin.id
                && record.source.eq_ignore_ascii_case(&origin.full_name)
                && record.branch == head
                && record.issue == target.issue,
            "independent start belongs to another source or issue; run start again for this work"
        );
        git_output(&[
            "merge-base",
            "--is-ancestor",
            &record.started_at,
            prepared_at,
        ])?;
        return Ok(Some(record));
    }
    Ok(None)
}

fn require_tip(
    source: &platform::source_identity::Checked,
    head: &str,
    current: &str,
    tip: &Pull,
) -> Result<()> {
    ensure!(
        tip.head.repo.as_ref().is_some_and(|repository| repository
            .full_name
            .eq_ignore_ascii_case(source.binding().source())),
        "the open stack tip belongs to another checkout source"
    );
    git_output(&["fetch", "--quiet", "origin", &tip.head.sha])?;
    let output = super::Tool::Git
        .command()
        .args(["merge-base", "--is-ancestor", &tip.head.sha, current])
        .output()?;
    if !output.status.success() {
        return Err(super::Refusal::new(
            "pr.stack-tip",
            format!(
                "{head} must include open stack tip #{} ({}) before new work starts",
                tip.number, tip.head.name
            ),
            command(&["git", "merge", "--no-edit", &tip.head.sha]),
        )
        .evidence(format!(
            "tip {} against {head} at {current}; {}: {}",
            tip.head.sha,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }
    Ok(())
}

pub(super) fn prepare(
    github: &Github,
    target: &Target,
    head: &str,
    requested_base: Option<&str>,
    independent: Option<&str>,
    operation: Operation,
) -> Result<Plan> {
    let head = head.rsplit_once(':').map_or(head, |(_, branch)| branch);
    let source = super::source_identity(github, &std::env::current_dir()?, &target.repo)?;
    let prepared_at = branch_head(head)?;
    let mine = pulls(github, target)?;
    let mut active = pending(
        github,
        target,
        head,
        &source,
        independent,
        operation,
        &prepared_at,
    )?;
    let existing = mine.iter().find(|pull| pull.head.name == head);
    if let (Some(record), Some(pull)) = (&mut active, existing) {
        ensure!(
            pull.head
                .repo
                .as_ref()
                .is_some_and(|repository| repository.id == record.source_id),
            "independent declaration belongs to another PR source"
        );
        record.pr = Some(pull.number);
        if matches!(operation, Operation::Start) {
            source.require_current()?;
            require_head(head, &prepared_at)?;
            save(&record_path(target, &github.login, head)?, record)?;
        }
    }
    let Inventory { work, evidence } = inventory(github, target, &mine, active.as_ref())?;
    let tip = work.tip().and_then(|index| mine.get(index));
    let joins_stack = active.is_none() && tip.is_some();
    let limit = github.policy.work_in_progress.limit(&target.repo);
    if !admits_creation(work.units(), limit, joins_stack || existing.is_some()) {
        let mut refusal = super::Refusal::new(
            "pr.wip",
            format!(
                "{} holds {} work units in {}, at its limit of {limit}",
                github.login,
                work.units(),
                target.repo
            ),
            command(&[
                "gh",
                "pr",
                "list",
                "--repo",
                &target.repo,
                "--author",
                &github.login,
            ]),
        );
        for line in evidence {
            refusal = refusal.evidence(line);
        }
        return Err(refusal.into());
    }
    let stacked = joins_stack && existing.is_none();
    let base = if let Some(tip) = tip.filter(|_| stacked) {
        super::require(
            requested_base.is_none_or(|base| base == tip.head.name),
            || {
                super::Refusal::new("pr.stack", "new work must use the open stack tip as its base; declare unrelated work with --independent <reason>",
                command(&["git", "merge", "--no-edit", &tip.head.sha]))
                .evidence(format!("stack tip #{} ({})", tip.number, tip.head.name))
            },
        )?;
        require_tip(&source, head, &prepared_at, tip)?;
        tip.head.name.clone()
    } else {
        requested_base
            .map(str::to_owned)
            .or_else(|| existing.map(|pull| pull.base.name.clone()))
            .or(github.repository(&target.repo)?.default_branch)
            .context("repository has no default branch")?
    };
    require_head(head, &prepared_at)?;
    Ok(Plan {
        base,
        stacked,
        record_path: record_path(target, &github.login, head)?,
        declaration: active,
        source,
        head: head.to_owned(),
        prepared_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        Declaration {
            schema: RECORD_VERSION,
            repository_id: 37,
            login: "Owner".into(),
            source_id: 37,
            source: "owner/project".into(),
            branch: "feature".into(),
            issue: Some(23),
            started_at: "a".repeat(40),
            pr: Some(17),
            reason: "Unrelated urgent correction".into(),
        }
    }

    #[test]
    fn local_records_round_trip_and_refuse_malformed_identity_and_reason() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("record.json");
        let original = declaration();
        save(&path, &original)?;
        let loaded = load(&path)?.context("record was not saved")?;
        assert_eq!(
            serde_json::to_value(&loaded)?,
            serde_json::to_value(&original)?
        );
        for (field, value) in [
            ("schema", serde_json::json!(2)),
            ("repository_id", serde_json::json!(0)),
            ("source_id", serde_json::json!(0)),
            ("pr", serde_json::json!(0)),
            ("started_at", serde_json::json!("--help")),
            ("reason", serde_json::json!("")),
            ("reason", serde_json::json!("\nInjected line")),
            ("reason", serde_json::json!("a".repeat(REASON_BYTES + 1))),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut malformed = serde_json::to_value(&original)?;
            malformed[field] = value;
            fs::write(&path, serde_json::to_vec(&malformed)?)?;
            assert!(load(&path).is_err(), "accepted {field}");
        }
        for invalid in [b"{".as_slice(), b"[]", b"null"] {
            fs::write(&path, invalid)?;
            assert!(load(&path).is_err());
        }
        fs::write(&path, vec![b' '; usize::try_from(RECORD_BYTES)? + 1])?;
        assert!(load(&path).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn local_record_links_never_read_or_replace_the_link_target() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let target = directory.path().join("target.json");
        let link = directory.path().join("record.json");
        let record = declaration();
        save(&target, &record)?;
        let original = fs::read(&target)?;
        std::os::unix::fs::symlink(&target, &link)?;
        assert!(load(&link).is_err());
        assert!(save(&link, &record).is_err());
        assert_eq!(fs::read(&target)?, original);
        Ok(())
    }
}
