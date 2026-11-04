//! Drops the check runs of a head that a newer run of the same check replaced, so a cancelled duplicate or a failed attempt before a rerun no longer blocks.
//! An incomplete or unavailable answer from GitHub keeps every reported run, which keeps such a failure blocking.

use crate::gh;
use rules::check_selection_rules::{Identity, Run, superseded_ids};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::Write as _;

/// Every check suite of the head with its app, workflow run, and check runs, including earlier attempts.
const QUERY: &str = "query($owner:String!,$name:String!,$oid:GitObjectID!){repository(owner:$owner,name:$name){object(oid:$oid){... on Commit{checkSuites(first:100){pageInfo{hasNextPage} nodes{databaseId app{databaseId} workflowRun{databaseId event workflow{databaseId}} checkRuns(first:100,filterBy:{checkType:ALL}){pageInfo{hasNextPage} nodes{databaseId name}}}}}}}}";
/// The path segment before the check run number in a workflow job's address.
const JOB_SEGMENT: &str = "/job/";

#[derive(Deserialize)]
struct Response {
    data: Option<Data>,
}

#[derive(Deserialize)]
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    object: Option<Commit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    check_suites: Page<Suite>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page<T> {
    page_info: PageInfo,
    nodes: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Suite {
    database_id: u64,
    app: Option<App>,
    workflow_run: Option<WorkflowRun>,
    check_runs: Page<CheckRun>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct App {
    database_id: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRun {
    database_id: u64,
    event: String,
    workflow: Option<Workflow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Workflow {
    database_id: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckRun {
    database_id: u64,
    name: String,
}

/// The runs of a complete answer, or `None` when a page is missing.
fn runs(commit: Commit) -> Option<Vec<Run>> {
    if commit.check_suites.page_info.has_next_page {
        return None;
    }
    let mut runs = Vec::new();
    for suite in commit.check_suites.nodes {
        if suite.check_runs.page_info.has_next_page {
            return None;
        }
        let (identity, order) = match (&suite.app, &suite.workflow_run) {
            (Some(app), Some(run)) => (
                run.workflow.as_ref().map(|workflow| Identity {
                    app: app.database_id,
                    workflow: Some(workflow.database_id),
                    event: Some(run.event.clone()),
                }),
                run.database_id,
            ),
            (Some(app), None) => (
                Some(Identity {
                    app: app.database_id,
                    workflow: None,
                    event: None,
                }),
                suite.database_id,
            ),
            (None, _) => (None, suite.database_id),
        };
        runs.extend(suite.check_runs.nodes.into_iter().map(|check| Run {
            identity: identity.clone(),
            name: check.name,
            order,
            id: check.database_id,
        }));
    }
    Some(runs)
}

/// The check runs of `head` that a newer run of the same check replaced, or `None` when GitHub's answer is unavailable or incomplete.
fn superseded(repository: &str, head: &str) -> Option<BTreeSet<u64>> {
    let (owner, name) = repository.split_once('/')?;
    let mut input = tempfile::NamedTempFile::new().ok()?;
    serde_json::to_writer(
        input.as_file_mut(),
        &serde_json::json!({"query": QUERY, "variables": {"owner": owner, "name": name, "oid": head}}),
    )
    .ok()?;
    input.as_file_mut().flush().ok()?;
    let args: Vec<OsString> = vec![
        "api".into(),
        "graphql".into(),
        "--hostname".into(),
        "github.com".into(),
        "--input".into(),
        input.path().as_os_str().to_owned(),
    ];
    let response: Response = serde_json::from_str(&gh(&args).ok()?).ok()?;
    let commit = response.data?.repository?.object?;
    Some(superseded_ids(&runs(commit)?).into_iter().collect())
}

/// The check run number in a workflow job's address, such as `.../actions/runs/1/job/2`.
fn check_run(details_url: &str) -> Option<u64> {
    let (_, tail) = details_url.rsplit_once(JOB_SEGMENT)?;
    let digits = tail
        .split(|character: char| !character.is_ascii_digit())
        .next()?;
    digits.parse().ok()
}

/// Removes from `rollup` every check run of `head` that a newer run of the same check replaced.
pub(crate) fn retain_current(repository: &str, head: &str, rollup: &mut Vec<crate::ReportedCheck>) {
    let Some(superseded) = superseded(repository, head) else {
        return;
    };
    rollup.retain(|check| {
        check
            .details_url
            .as_deref()
            .and_then(check_run)
            .is_none_or(|id| !superseded.contains(&id))
    });
}

#[cfg(test)]
mod tests {
    use super::{Commit, check_run, runs};
    use rules::check_selection_rules::superseded_ids;

    fn commit(suites: &serde_json::Value, more: bool) -> Commit {
        serde_json::from_value(serde_json::json!({
            "checkSuites": {"pageInfo": {"hasNextPage": more}, "nodes": suites}
        }))
        .unwrap()
    }

    fn suite(
        id: u64,
        run: u64,
        event: &str,
        checks: &[(u64, &str)],
        more: bool,
    ) -> serde_json::Value {
        serde_json::json!({
            "databaseId": id,
            "app": {"databaseId": 15368},
            "workflowRun": {"databaseId": run, "event": event, "workflow": {"databaseId": 7}},
            "checkRuns": {
                "pageInfo": {"hasNextPage": more},
                "nodes": checks.iter().map(|(id, name)| serde_json::json!({"databaseId": id, "name": name})).collect::<Vec<_>>()
            }
        })
    }

    #[test]
    fn a_cancelled_duplicate_run_is_superseded_by_the_later_run_of_its_workflow() {
        let suites = serde_json::json!([
            suite(1, 41_751_624_966, "pull_request", &[(10, "Checks")], false),
            suite(
                2,
                41_751_625_187,
                "pull_request",
                &[(20, "Checks"), (21, "Rust")],
                false
            ),
            suite(3, 41_751_600_000, "push", &[(30, "Checks")], false),
        ]);
        let runs = runs(commit(&suites, false)).unwrap();
        assert_eq!(superseded_ids(&runs), vec![10]);
    }

    #[test]
    fn a_missing_page_or_workflow_identity_supersedes_nothing() {
        let suites = serde_json::json!([suite(1, 1, "pull_request", &[(10, "Checks")], false)]);
        assert!(runs(commit(&suites, true)).is_none());
        let suites = serde_json::json!([suite(1, 1, "pull_request", &[(10, "Checks")], true)]);
        assert!(runs(commit(&suites, false)).is_none());
        let mut anonymous = suite(1, 1, "pull_request", &[(10, "Checks")], false);
        anonymous["workflowRun"]["workflow"] = serde_json::Value::Null;
        let suites = serde_json::json!([
            anonymous,
            suite(2, 2, "pull_request", &[(20, "Checks")], false)
        ]);
        assert_eq!(
            superseded_ids(&runs(commit(&suites, false)).unwrap()),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn a_workflow_job_address_names_its_check_run() {
        assert_eq!(
            check_run(
                "https://github.com/P4suta/project/actions/runs/41751624966/job/123456?pr=205"
            ),
            Some(123_456)
        );
        assert_eq!(check_run("https://ci.example.com/build/9"), None);
        assert_eq!(
            check_run("https://github.com/o/r/actions/runs/1/job/"),
            None
        );
    }
}
