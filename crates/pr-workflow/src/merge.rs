use super::{Github, Polling, Target, gh, merge_view, sleep_interval};
use anyhow::{Context, Result, ensure};
use platform::refusal::{Refusal, command};
use rules::pr_merge_rules::{Lifecycle, Queue, Settlement, settlement};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::time::{Duration, Instant};

const QUEUE_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){state headRefOid mergeQueueEntry{id}}}}";

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum AsyncStatus {
    Pending,
    Merged,
    Enqueued,
    Failed,
}

#[derive(Deserialize)]
struct AsyncResult {
    status: AsyncStatus,
    details: AsyncDetails,
}

#[derive(Deserialize)]
struct AsyncDetails {
    message: String,
    uuid: Option<String>,
    expected_head_sha: Option<String>,
    merge_action: Option<String>,
}

#[derive(Deserialize)]
struct QueueResponse {
    data: Option<QueueData>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct QueueData {
    repository: Option<QueueRepository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueRepository {
    pull_request: Option<QueuePr>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueuePr {
    state: String,
    head_ref_oid: String,
    merge_queue_entry: Option<QueueEntry>,
}

#[derive(Deserialize)]
struct QueueEntry {
    id: String,
}

#[derive(Serialize)]
struct MemberResult {
    number: u64,
    expected_head: String,
    observed_head: String,
    outcome: &'static str,
}

fn expired(polling: Polling, started: Instant) -> bool {
    started.elapsed()
        >= Duration::from_secs(polling.timeout.saturating_mul(super::SECONDS_PER_MINUTE))
}

fn outcome(
    target: &Target,
    heads: &[(u64, String)],
    result: Settlement,
    members: &[MemberResult],
) -> Result<()> {
    println!(
        "{}",
        serde_json::json!({"repository":target.repo,"outcome":result.key(),"members":members,
        "pull_requests":heads.iter().map(|(pr,head)|serde_json::json!({"number":pr,"head":head})).collect::<Vec<_>>() })
    );
    if result == Settlement::Merged {
        for (pr, head) in heads {
            println!("#{pr} merged at {head}");
        }
        return Ok(());
    }
    Err(Refusal::new(
        "pr.merge",
        format!("merge stopped: {}", result.key()),
        command(&[
            "gh",
            "pr",
            "view",
            &heads.last().context("merge has no heads")?.0.to_string(),
            "--repo",
            &target.repo,
        ]),
    )
    .evidence(format!("outcome {}", result.key()))
    .into())
}

pub(super) fn submit(
    github: &Github,
    target: &Target,
    top: u64,
    heads: &[(u64, String)],
    queue: bool,
    polling: Polling,
    started: Instant,
) -> Result<()> {
    let expected = &heads.last().context("merge has no verified heads")?.1;
    let action = if queue { "merge_queue" } else { "direct_merge" };
    let mut body = serde_json::json!({"sha":expected,"merge_action":action});
    if !queue {
        body["merge_method"] = "squash".into();
    }
    let mut input = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(input.as_file_mut(), &body)?;
    input.as_file().sync_all()?;
    let endpoint = format!("repos/{}/pulls/{top}/merge-async", target.repo);
    let args: Vec<OsString> = vec![
        "api".into(),
        endpoint.clone().into(),
        "--hostname".into(),
        "github.com".into(),
        "--include".into(),
        "--method".into(),
        "PUT".into(),
        "--input".into(),
        input.path().into(),
    ];
    let mut result: AsyncResult =
        github.response_with_status(&gh(&args)?, "core", &["200", "202"])?;
    loop {
        match result.status {
            AsyncStatus::Merged => {
                return await_result(github, target, heads, false, polling, started);
            }
            AsyncStatus::Enqueued => {
                ensure!(queue, "a direct merge unexpectedly entered a queue");
                return await_result(github, target, heads, true, polling, started);
            }
            AsyncStatus::Failed => {
                return Err(Refusal::new(
                    "pr.merge",
                    result.details.message,
                    command(&["gh", "pr", "view", &top.to_string(), "--repo", &target.repo]),
                )
                .into());
            }
            AsyncStatus::Pending => {}
        }
        ensure!(
            result.details.expected_head_sha.as_ref() == Some(expected)
                && result.details.merge_action.as_deref() == Some(action),
            "the pending merge is not bound to the requested head and routing"
        );
        if expired(polling, started) {
            return outcome(target, heads, Settlement::TimedOut, &[]);
        }
        let uuid = result
            .details
            .uuid
            .as_deref()
            .filter(|uuid| !uuid.is_empty())
            .context("pending merge UUID missing")?;
        sleep_interval(polling, started);
        result = github.read(&format!(
            "{endpoint}/{}",
            super::review_policy::path_segment(uuid)?
        ))?;
    }
}

pub(super) fn await_result(
    github: &Github,
    target: &Target,
    heads: &[(u64, String)],
    enqueued: bool,
    polling: Polling,
    started: Instant,
) -> Result<()> {
    loop {
        let mut results = Vec::new();
        let mut members = Vec::new();
        for (pr, expected) in heads {
            let (state, head, present) = if enqueued {
                let (owner, name) = target
                    .repo
                    .split_once('/')
                    .context("repository identity missing")?;
                let response: QueueResponse = github.graphql(
                    QUEUE_QUERY,
                    &serde_json::json!({"owner":owner,"name":name,"number":pr}),
                )?;
                ensure!(
                    response.errors.is_empty(),
                    "GitHub could not read merge queue membership"
                );
                let observed = response
                    .data
                    .and_then(|data| data.repository)
                    .and_then(|repo| repo.pull_request)
                    .context("merge queue PR observation missing")?;
                let present = match observed.merge_queue_entry {
                    Some(entry) => {
                        ensure!(!entry.id.is_empty(), "merge queue entry identity missing");
                        true
                    }
                    None => false,
                };
                (observed.state, observed.head_ref_oid, present)
            } else {
                let view = merge_view(target, *pr)?;
                (view.state, view.head_ref_oid, false)
            };
            let lifecycle = match state.as_str() {
                "OPEN" => Lifecycle::Open,
                "MERGED" => Lifecycle::Merged,
                "CLOSED" => Lifecycle::Closed,
                _ => anyhow::bail!("unknown merge lifecycle"),
            };
            let queue = match (enqueued, present) {
                (false, _) => Queue::NotRequested,
                (true, true) => Queue::Present,
                (true, false) => Queue::Removed,
            };
            let result = settlement(
                head == *expected,
                lifecycle,
                queue,
                expired(polling, started),
            );
            members.push(MemberResult {
                number: *pr,
                expected_head: expected.clone(),
                observed_head: head,
                outcome: result.key(),
            });
            results.push(result);
        }
        if let Some(failure) = results
            .iter()
            .find(|result| !matches!(result, Settlement::Merged | Settlement::Waiting))
        {
            return outcome(target, heads, *failure, &members);
        }
        if results.iter().all(|result| *result == Settlement::Merged) {
            return outcome(target, heads, Settlement::Merged, &members);
        }
        sleep_interval(polling, started);
    }
}
