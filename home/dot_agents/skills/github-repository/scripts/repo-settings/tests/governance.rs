use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use repo_settings::{
    Api, Environment, Operation, OwnerType, Profile, Repository, Snapshot, State, Workflow, apply,
    plan,
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

fn state() -> State {
    State {
        metadata: json!({"current": true}),
        repository: Repository {
            id: 7,
            full_name: "owner/project".into(),
            owner_id: 9,
            owner_type: OwnerType::Organization,
            default_branch: "main".into(),
            archived: false,
            fork: false,
            admin: true,
            settings: Profile::baseline().unwrap().repository,
        },
        immutable: json!({"enabled":true,"enforced_by_owner":false}),
        rulesets: vec![
            json!({"id":1,"name":"main","target":"branch","enforcement":"active","bypass_actors":[],
                "conditions":{"ref_name":{"include":["~DEFAULT_BRANCH"],"exclude":[]}},
                "rules":[{"type":"deletion"},{"type":"non_fast_forward"},{"type":"required_signatures"},
                    {"type":"required_linear_history"},{"type":"pull_request","parameters":{
                        "required_approving_review_count":2,"dismiss_stale_reviews_on_push":true,
                        "require_code_owner_review":true,"required_review_thread_resolution":true}},
                    {"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"actual-ci","integration_id":15368}],"strict_required_status_checks_policy":false}},
                    {"type":"merge_queue","parameters":{"merge_method":"SQUASH"}}]}),
            json!({"id":2,"name":"tags","target":"tag","enforcement":"active","bypass_actors":[],
                "conditions":{"ref_name":{"include":["refs/tags/v*"],"exclude":[]}},
                "rules":[{"type":"update"},{"type":"deletion"},{"type":"non_fast_forward"}]}),
        ],
        environments: Vec::new(),
        workflows: vec![Workflow {
            path: QUEUE_WORKFLOW.into(),
            yaml: "on: [pull_request, merge_group]\njobs:\n  actual-ci: {}\n".into(),
        }],
        workflow_permissions: json!({"default_workflow_permissions":"read","can_approve_pull_request_reviews":false}),
        security: json!({"secret_scanning":{"status":"enabled"},"secret_scanning_push_protection":{"status":"enabled"}}),
        actions: json!({"enabled":true,"allowed_actions":"selected","sha_pinning_required":true}),
    }
}

#[test]
fn missing_or_drifted_generated_metadata_is_a_finding() -> Result<()> {
    for metadata in [
        Value::Null,
        json!({"current":false,"evidence":"repository.generated refused"}),
    ] {
        let mut original = state();
        original.metadata = metadata;
        let planned = plan(snapshot(original), Profile::baseline()?)?;
        assert!(
            planned
                .findings
                .iter()
                .any(|finding| finding.message.contains("dotfiles repository generate"))
        );
        assert!(planned.changes.is_empty());
    }
    Ok(())
}

#[test]
fn missing_secret_scanning_and_pinning_are_planned_and_open_actions_are_a_finding() -> Result<()> {
    let mut original = state();
    original.security["secret_scanning_push_protection"]["status"] = json!("disabled");
    original.actions["sha_pinning_required"] = json!(false);
    original.actions["allowed_actions"] = json!("all");
    let planned = repo_settings::plan_scoped(
        snapshot(original),
        Profile::baseline()?,
        repo_settings::Scope::Security,
    )?;
    assert_eq!(planned.changes.len(), 2);
    let Operation::SecretScanning { after, .. } = &planned.changes[0].operation else {
        panic!("expected secret scanning");
    };
    assert_eq!(
        after["secret_scanning_push_protection"]["status"],
        "enabled"
    );
    let Operation::ActionsPinning { after, .. } = &planned.changes[1].operation else {
        panic!("expected action pinning");
    };
    assert_eq!(after["sha_pinning_required"], true);
    assert_eq!(after["allowed_actions"], "all");
    assert!(
        planned
            .findings
            .iter()
            .any(|finding| finding.message.contains("allow list"))
    );
    Ok(())
}

#[test]
fn applying_the_security_scope_verifies_each_setting() -> Result<()> {
    let mut previous = state();
    previous.security["secret_scanning"]["status"] = json!("disabled");
    previous.actions["sha_pinning_required"] = json!(false);
    let planned = repo_settings::plan_scoped(
        snapshot(previous.clone()),
        Profile::baseline()?,
        repo_settings::Scope::Security,
    )?;
    let api = MemoryApi {
        current: RefCell::new(previous.clone()),
        remove_required_check_on_enable: false,
        ruleset_update_fault: None,
    };
    apply(&api, &planned, |_| Ok(()))?;
    let after = api.current.borrow();
    assert_eq!(after.security["secret_scanning"]["status"], "enabled");
    assert_eq!(after.actions["sha_pinning_required"], true);
    assert_eq!(
        after.actions["allowed_actions"],
        previous.actions["allowed_actions"]
    );
    assert_eq!(after.rulesets, previous.rulesets);
    Ok(())
}

#[test]
fn profiles_cannot_turn_off_secret_scanning_or_pinning() {
    let mut profile = Profile::baseline().unwrap();
    profile.security.push_protection = false;
    assert!(profile.validate().is_err());
    let mut profile = Profile::baseline().unwrap();
    profile.security.sha_pinning_required = false;
    assert!(profile.validate().is_err());
}

fn snapshot(state: State) -> Snapshot {
    Snapshot {
        schema_version: 1,
        repositories: vec![state],
    }
}

const QUEUE_WORKFLOW: &str = ".github/workflows/ci.yml";
const SECOND_WORKFLOW: &str = ".github/workflows/secondary.yaml";

fn queue_state(workflows: &[(&str, &str)]) -> Result<State> {
    let mut value = serde_json::to_value(state())?;
    value["workflows"] = json!(
        workflows
            .iter()
            .map(|(path, yaml)| { json!({"path": path, "yaml": yaml}) })
            .collect::<Vec<_>>()
    );
    value["rulesets"][0]["rules"]
        .as_array_mut()
        .unwrap()
        .retain(|rule| rule["type"] != "merge_queue");
    value["rulesets"][0]["rules"][5]["parameters"]["strict_required_status_checks_policy"] =
        json!(true);
    Ok(serde_json::from_value(value)?)
}

fn ready_queue_state() -> Result<State> {
    queue_state(&[(
        QUEUE_WORKFLOW,
        "on: [pull_request, merge_group]\njobs:\n  actual-ci:\n    runs-on: ubuntu-latest\n    steps: []\n",
    )])
}

fn queue_updates(planned: &repo_settings::Plan) -> Result<Vec<Value>> {
    Ok(planned
        .changes
        .iter()
        .map(|change| serde_json::to_value(&change.operation))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|operation| operation["kind"] == "update_ruleset")
        .collect())
}

#[test]
fn queue_without_required_checks_is_never_planned() -> Result<()> {
    let mut original = ready_queue_state()?;
    original.rulesets[0]["rules"]
        .as_array_mut()
        .unwrap()
        .retain(|rule| rule["type"] != "required_status_checks");
    let planned = plan(snapshot(original), Profile::baseline()?)?;
    assert!(planned.changes.is_empty());
    assert!(
        planned
            .findings
            .iter()
            .any(|finding| finding.message.contains("never invents a check name"))
    );
    Ok(())
}

#[test]
fn queue_requires_the_trigger_in_every_reporting_workflow() -> Result<()> {
    let original = queue_state(&[
        (
            QUEUE_WORKFLOW,
            "on: pull_request\njobs:\n  test:\n    name: actual-ci\n",
        ),
        (
            SECOND_WORKFLOW,
            "on: [push, pull_request]\njobs:\n  actual-ci: {}\n",
        ),
    ])?;
    let planned = plan(snapshot(original), Profile::baseline()?)?;
    assert!(planned.changes.is_empty());
    for path in [QUEUE_WORKFLOW, SECOND_WORKFLOW] {
        assert!(
            planned
                .findings
                .iter()
                .any(|finding| finding.message.contains(path)
                    && finding.message.contains("merge_group")),
            "missing trigger finding for {path}"
        );
    }
    Ok(())
}

#[test]
fn user_owned_repositories_keep_strict_checks_without_queue_findings() -> Result<()> {
    for yaml in [
        "on: [pull_request, merge_group]\njobs:\n  actual-ci: {}\n",
        "on: pull_request\njobs:\n  actual-ci: {}\n",
        "on: [invalid yaml",
    ] {
        for required_checks in [true, false] {
            let mut previous = queue_state(&[(QUEUE_WORKFLOW, yaml)])?;
            if !required_checks {
                previous.rulesets[0]["rules"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|rule| rule["type"] != "required_status_checks");
            }
            let mut api = ReadOnlyApi::new(&previous);
            api.responses.get_mut("repos/owner/project").unwrap()["owner"]["type"] = json!("User");
            let original = repo_settings::audit(&api, "owner/project")?;
            let planned = plan(snapshot(original.clone()), Profile::baseline()?)?;
            assert!(planned.changes.is_empty(), "{:#?}", planned.changes);
            assert!(planned.findings.is_empty(), "{:#?}", planned.findings);
            assert_eq!(planned.snapshot.repositories[0], original);
            assert_eq!(api.writes.get(), 0);
        }
    }
    Ok(())
}

#[test]
fn legacy_snapshots_default_to_user_owned_and_skip_queue_planning() -> Result<()> {
    let mut value = serde_json::to_value(snapshot(ready_queue_state()?))?;
    value["repositories"][0]["repository"]
        .as_object_mut()
        .unwrap()
        .remove("owner_type");
    let original: Snapshot = serde_json::from_value(value)?;
    assert_eq!(
        original.repositories[0].repository.owner_type,
        OwnerType::User
    );
    let planned = plan(original.clone(), Profile::baseline()?)?;
    assert!(planned.changes.is_empty(), "{:#?}", planned.changes);
    assert!(planned.findings.is_empty(), "{:#?}", planned.findings);
    assert_eq!(planned.snapshot, original);
    Ok(())
}

#[test]
fn audit_records_the_owner_type() -> Result<()> {
    for owner_type in ["User", "Organization"] {
        let mut api = ReadOnlyApi::new(&ready_queue_state()?);
        api.responses.get_mut("repos/owner/project").unwrap()["owner"]["type"] = json!(owner_type);
        let audited = repo_settings::audit(&api, "owner/project")?;
        assert_eq!(
            serde_json::to_value(audited.repository)?["owner_type"],
            owner_type
        );
        assert_eq!(api.writes.get(), 0);
    }
    Ok(())
}

#[test]
fn audit_rejects_missing_or_unknown_owner_types() -> Result<()> {
    for owner_type in [None, Some(json!("Bot")), Some(Value::Null)] {
        let mut api = ReadOnlyApi::new(&ready_queue_state()?);
        let owner = api.responses.get_mut("repos/owner/project").unwrap()["owner"]
            .as_object_mut()
            .unwrap();
        if let Some(owner_type) = owner_type {
            owner.insert("type".into(), owner_type);
        } else {
            owner.remove("type");
        }
        assert!(repo_settings::audit(&api, "owner/project").is_err());
        assert_eq!(api.writes.get(), 0);
    }
    Ok(())
}

#[test]
fn organization_owned_queue_ready_workflows_plan_one_squash_update_and_drop_strict_checks()
-> Result<()> {
    let api = ReadOnlyApi::new(&ready_queue_state()?);
    let original = repo_settings::audit(&api, "owner/project")?;
    let planned = plan(snapshot(original.clone()), Profile::baseline()?)?;
    assert!(planned.findings.is_empty());
    assert_eq!(planned.changes.len(), 1);
    let updates = queue_updates(&planned)?;
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["id"], original.rulesets[0]["id"]);
    assert_eq!(updates[0]["before"]["rules"], original.rulesets[0]["rules"]);
    let rules = updates[0]["after"]["rules"].as_array().unwrap();
    let queue = rules
        .iter()
        .find(|rule| rule["type"] == "merge_queue")
        .unwrap();
    assert_eq!(
        queue["parameters"],
        json!({
            "merge_method": "SQUASH", "grouping_strategy": "ALLGREEN",
            "max_entries_to_build": 1, "min_entries_to_merge": 1, "max_entries_to_merge": 1,
            "min_entries_to_merge_wait_minutes": 0, "check_response_timeout_minutes": 60
        })
    );
    let checks = rules
        .iter()
        .find(|rule| rule["type"] == "required_status_checks")
        .unwrap();
    assert_eq!(
        checks["parameters"]["strict_required_status_checks_policy"],
        false
    );
    Ok(())
}

#[test]
fn queue_already_present_is_left_unchanged() -> Result<()> {
    let mut original = ready_queue_state()?;
    original.rulesets[0]["rules"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "merge_queue", "parameters": {"merge_method": "SQUASH"}}));
    let planned = plan(snapshot(original), Profile::baseline()?)?;
    assert!(planned.changes.is_empty());
    assert!(planned.findings.is_empty());
    Ok(())
}

#[test]
fn queue_does_not_create_a_second_baseline_ruleset() -> Result<()> {
    let original = ready_queue_state()?;
    let planned = plan(snapshot(original), Profile::baseline()?)?;
    assert!(
        planned
            .changes
            .iter()
            .all(|change| !matches!(change.operation, Operation::CreateRuleset { .. }))
    );
    assert_eq!(queue_updates(&planned)?.len(), 1);
    Ok(())
}

#[test]
fn queue_update_preserves_the_complete_live_ruleset() -> Result<()> {
    let mut original = ready_queue_state()?;
    let protection = original.rulesets[0].clone();
    original.rulesets[0]["name"] = json!("custom protected branch");
    original.rulesets[0]["conditions"]["ref_name"]["include"] =
        json!(["~DEFAULT_BRANCH", "refs/heads/stable/*"]);
    original.rulesets[0]["bypass_actors"] =
        json!([{"actor_id": 9, "actor_type": "Integration", "bypass_mode": "pull_request"}]);
    original.rulesets.push(protection);
    original.rulesets[0]["id"] = json!(3);
    original.rulesets[0]["rules"].as_array_mut().unwrap().push(json!({
        "type": "commit_message_pattern", "parameters": {"operator": "starts_with", "pattern": "feat"}
    }));
    original.rulesets[0]["created_at"] = json!("2026-10-01T00:00:00Z");
    let planned = plan(snapshot(original.clone()), Profile::baseline()?)?;
    let updates = queue_updates(&planned)?;
    let update = updates
        .iter()
        .find(|update| update["id"] == original.rulesets[0]["id"])
        .unwrap();
    let mut expected = update["before"].clone();
    assert_eq!(expected, original.rulesets[0]);
    expected["rules"][5]["parameters"]["strict_required_status_checks_policy"] = json!(false);
    let added_queue = update["after"]["rules"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(added_queue["type"], "merge_queue");
    expected["rules"].as_array_mut().unwrap().push(added_queue);
    assert_eq!(update["after"], expected);
    Ok(())
}

#[test]
fn merge_settings_scope_cannot_change_release_or_approval_gates() -> Result<()> {
    let mut original = state();
    original
        .repository
        .settings
        .insert("allow_update_branch".into(), json!(false));
    original.immutable["enabled"] = json!(false);
    original.rulesets.clear();
    let planned = repo_settings::plan_scoped(
        snapshot(original),
        Profile::baseline()?,
        repo_settings::Scope::MergeSettings,
    )?;
    assert_eq!(planned.changes.len(), 1);
    assert!(matches!(
        planned.changes[0].operation,
        Operation::RepositorySettings { .. }
    ));
    Ok(())
}

#[test]
fn stronger_reviews_and_real_required_checks_are_preserved() {
    let original = state();
    let result = plan(snapshot(original.clone()), Profile::baseline().unwrap()).unwrap();
    assert!(result.changes.is_empty());
    assert!(result.findings.is_empty());
    assert_eq!(result.snapshot.repositories[0], original);
}

#[test]
fn missing_immutability_only_plans_enabling_it() {
    let mut original = state();
    original.immutable["enabled"] = json!(false);
    let result = plan(snapshot(original), Profile::baseline().unwrap()).unwrap();
    assert_eq!(result.changes.len(), 1);
    assert!(matches!(
        result.changes[0].operation,
        Operation::EnableImmutableReleases
    ));
}

#[test]
fn profiles_cannot_disable_protection_or_add_unrelated_repository_writes() {
    let mut profile = Profile::baseline().unwrap();
    profile.immutable_releases = false;
    assert!(profile.validate().is_err());
    let mut profile = Profile::baseline().unwrap();
    profile
        .repository
        .insert("allow_merge_commit".into(), json!(true));
    assert!(profile.validate().is_err());
    let mut profile = Profile::baseline().unwrap();
    profile
        .repository
        .insert("default_branch".into(), json!("unprotected"));
    assert!(profile.validate().is_err());
}

#[test]
fn protection_with_a_bypass_or_a_ref_exclusion_does_not_satisfy_the_baseline() {
    for restriction in ["bypass", "exclusion", "disabled"] {
        let mut original = state();
        match restriction {
            "bypass" => {
                original.rulesets[1]["bypass_actors"] =
                    json!([{"actor_type":"RepositoryRole","actor_id":5,"bypass_mode":"always"}])
            }
            "exclusion" => {
                original.rulesets[1]["conditions"]["ref_name"]["exclude"] = json!(["refs/tags/v1*"])
            }
            _ => original.rulesets[1]["enforcement"] = json!("disabled"),
        }
        let result = plan(snapshot(original), Profile::baseline().unwrap()).unwrap();
        assert_eq!(result.changes.len(), 1);
        let Operation::CreateRuleset { definition } = &result.changes[0].operation else {
            panic!("expected protection");
        };
        assert_eq!(definition["bypass_actors"], json!([]));
        assert_eq!(definition["conditions"]["ref_name"]["exclude"], json!([]));
    }
}

#[test]
fn removing_admin_bypass_preserves_reviewers_waits_and_ref_limits() {
    let mut original = state();
    original.environments.push(Environment { name: "release".into(),
        definition: json!({"name":"release","can_admins_bypass":true,
            "deployment_branch_policy":{"protected_branches":false,"custom_branch_policies":true},
            "protection_rules":[{"type":"required_reviewers","prevent_self_review":true,
                "reviewers":[{"type":"User","reviewer":{"id":9}},{"type":"Team","reviewer":{"id":12}}]},
                {"type":"wait_timer","wait_timer":30},{"type":"branch_policy"}]}),
        branch_policies: vec![json!({"id":4,"name":"v*.*.*","type":"tag"})] });
    let result = plan(snapshot(original.clone()), Profile::baseline().unwrap()).unwrap();
    let Operation::Environment { before, after, .. } = &result.changes[0].operation else {
        panic!("expected environment");
    };
    let mut expected = before.clone();
    expected["can_admins_bypass"] = json!(false);
    assert_eq!(*after, expected);
    assert_eq!(
        after["reviewers"],
        json!([{"type":"User","id":9},{"type":"Team","id":12}])
    );
    assert_eq!(after["prevent_self_review"], true);
    assert_eq!(after["wait_timer"], 30);
    assert_eq!(
        result.snapshot.repositories[0].environments[0].branch_policies,
        original.environments[0].branch_policies
    );
}

#[test]
fn unrelated_environments_are_left_alone() {
    let mut original = state();
    original.environments.push(Environment { name: "github-pages".into(),
        definition: json!({"name":"github-pages","can_admins_bypass":true,"deployment_branch_policy":null,"protection_rules":[]}),
        branch_policies: Vec::new() });
    assert!(
        plan(snapshot(original), Profile::baseline().unwrap())
            .unwrap()
            .changes
            .is_empty()
    );
}

struct ReadOnlyApi {
    responses: BTreeMap<String, Value>,
    writes: Cell<usize>,
    repository_reads: Cell<usize>,
    change_identity_after_preflight: bool,
    request_counts: RefCell<BTreeMap<String, usize>>,
    response_changes: BTreeMap<String, (usize, Value)>,
}

impl ReadOnlyApi {
    fn new(state: &State) -> Self {
        let root = format!("repos/{}", state.repository.full_name);
        let mut raw = serde_json::to_value(&state.repository.settings).unwrap();
        raw["id"] = json!(state.repository.id);
        raw["full_name"] = json!(state.repository.full_name);
        raw["owner"] = json!({"id":state.repository.owner_id,"type":state.repository.owner_type});
        raw["default_branch"] = json!(state.repository.default_branch);
        raw["archived"] = json!(state.repository.archived);
        raw["fork"] = json!(state.repository.fork);
        raw["permissions"] = json!({"admin":state.repository.admin});
        raw["security_and_analysis"] = state.security.clone();
        let mut responses = BTreeMap::from([
            (root.clone(), raw),
            (
                format!("{root}/immutable-releases"),
                state.immutable.clone(),
            ),
            (
                format!("{root}/rulesets?per_page=100&page=1"),
                json!(state.rulesets),
            ),
            (
                format!("{root}/environments?per_page=100&page=1"),
                json!({"environments":state.environments.iter().map(|env| &env.definition).collect::<Vec<_>>()}),
            ),
            (
                format!("{root}/actions/permissions/workflow"),
                state.workflow_permissions.clone(),
            ),
            (format!("{root}/actions/permissions"), state.actions.clone()),
        ]);
        let branch = repo_settings::encode_segment(&state.repository.default_branch);
        responses.insert(
            format!("{root}/contents?ref={branch}"),
            json!([{"type":"dir","path":".github"}]),
        );
        responses.insert(
            format!("{root}/contents/.github?ref={branch}"),
            json!([{"type":"dir","path":".github/workflows"}]),
        );
        responses.insert(
            format!("{root}/contents/.github/workflows?ref={branch}"),
            json!(
                state
                    .workflows
                    .iter()
                    .map(|workflow| { json!({"type":"file", "path":workflow.path}) })
                    .collect::<Vec<_>>()
            ),
        );
        for workflow in &state.workflows {
            let path = workflow
                .path
                .split('/')
                .map(repo_settings::encode_segment)
                .collect::<Vec<_>>()
                .join("/");
            responses.insert(
                format!("{root}/contents/{path}?ref={branch}"),
                json!({"encoding":"base64", "content":STANDARD.encode(&workflow.yaml)}),
            );
        }
        for rule in &state.rulesets {
            responses.insert(format!("{root}/rulesets/{}", rule["id"]), rule.clone());
        }
        for env in &state.environments {
            let endpoint = format!(
                "{root}/environments/{}",
                repo_settings::encode_segment(&env.name)
            );
            responses.insert(endpoint.clone(), env.definition.clone());
            responses.insert(
                format!("{endpoint}/deployment-branch-policies?per_page=100&page=1"),
                json!({"branch_policies":env.branch_policies}),
            );
        }
        Self {
            responses,
            writes: Cell::new(0),
            repository_reads: Cell::new(0),
            change_identity_after_preflight: false,
            request_counts: RefCell::new(BTreeMap::new()),
            response_changes: BTreeMap::new(),
        }
    }
}

impl Api for ReadOnlyApi {
    fn metadata(&self, _: &str) -> Result<Value> {
        Ok(json!({"current": true}))
    }
    fn read(&self, endpoint: &str) -> Result<Value> {
        let mut response = self
            .responses
            .get(endpoint)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unconfigured request {endpoint}"))?;
        let mut counts = self.request_counts.borrow_mut();
        let count = counts.entry(endpoint.to_owned()).or_default();
        *count += 1;
        if let Some((after, changed)) = self.response_changes.get(endpoint)
            && *count >= *after
        {
            response = changed.clone();
        }
        if endpoint == "repos/owner/project" {
            self.repository_reads.set(self.repository_reads.get() + 1);
            if self.change_identity_after_preflight && self.repository_reads.get() > 1 {
                response["id"] = json!(700);
            }
        }
        Ok(response)
    }

    fn write(&self, _: repo_settings::Mutation<'_>) -> Result<Value> {
        self.writes.set(self.writes.get() + 1);
        bail!("unexpected write")
    }
}

#[test]
fn stale_repository_identity_or_settings_fail_before_any_write() {
    for field in ["id", "owner", "default_branch", "setting"] {
        let mut previous = state();
        previous.immutable["enabled"] = json!(false);
        let planned = plan(snapshot(previous.clone()), Profile::baseline().unwrap()).unwrap();
        let mut live = previous;
        match field {
            "id" => live.repository.id += 1,
            "owner" => live.repository.owner_id += 1,
            "default_branch" => live.repository.default_branch = "changed".into(),
            _ => {
                live.repository
                    .settings
                    .insert("allow_auto_merge".into(), json!(false));
            }
        }
        let api = ReadOnlyApi::new(&live);
        assert!(apply(&api, &planned, |_| Ok(())).is_err());
        assert_eq!(api.writes.get(), 0);
    }
}

#[test]
fn edited_plan_is_rejected_before_network_calls() {
    let original = state();
    let mut planned = plan(snapshot(original.clone()), Profile::baseline().unwrap()).unwrap();
    planned.changes.push(repo_settings::Change {
        repository: "owner/project".into(),
        operation: Operation::EnableImmutableReleases,
    });
    let api = ReadOnlyApi {
        responses: BTreeMap::new(),
        writes: Cell::new(0),
        repository_reads: Cell::new(0),
        change_identity_after_preflight: false,
        request_counts: RefCell::new(BTreeMap::new()),
        response_changes: BTreeMap::new(),
    };
    assert!(
        apply(&api, &planned, |_| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("plan was edited")
    );
    assert_eq!(api.writes.get(), 0);
}

#[test]
fn repository_recreated_after_preflight_is_rejected_before_a_write() {
    let mut previous = state();
    previous.immutable["enabled"] = json!(false);
    let planned = plan(snapshot(previous.clone()), Profile::baseline().unwrap()).unwrap();
    let mut api = ReadOnlyApi::new(&previous);
    api.change_identity_after_preflight = true;
    assert!(apply(&api, &planned, |_| Ok(())).is_err());
    assert_eq!(
        api.writes.get(),
        0,
        "a stale name must never receive a mutation"
    );
}

#[test]
fn invalid_repository_targets_and_archive_or_fork_scopes_are_rejected() {
    for name in [
        "owner/project?x=1",
        "owner/../project",
        "/owner/project",
        "owner/project#tag",
    ] {
        assert!(repo_settings::validate_repo_name(name).is_err());
    }
    for restriction in ["archive", "fork", "nonadmin"] {
        let mut original = state();
        match restriction {
            "archive" => original.repository.archived = true,
            "fork" => original.repository.fork = true,
            _ => original.repository.admin = false,
        }
        assert!(plan(snapshot(original), Profile::baseline().unwrap()).is_err());
    }
}

#[test]
fn absent_ci_is_a_finding_and_never_an_invented_required_check() {
    let mut original = state();
    original.rulesets = Vec::new();
    let planned = plan(snapshot(original), Profile::baseline().unwrap()).unwrap();
    assert_eq!(planned.findings.len(), 1);
    for change in planned.changes {
        if let Operation::CreateRuleset { definition } = change.operation {
            assert!(
                definition["rules"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|rule| rule["type"] != "required_status_checks")
            );
        }
    }
}

struct MemoryApi {
    current: RefCell<State>,
    remove_required_check_on_enable: bool,
    ruleset_update_fault: Option<RulesetUpdateFault>,
}

#[derive(Clone, Copy)]
enum RulesetUpdateFault {
    Ignore,
    DropRequiredCheck,
    WrongIdentity,
}

#[test]
fn environment_scope_preserves_merge_settings_rulesets_and_immutability() -> Result<()> {
    let mut previous = state();
    previous
        .repository
        .settings
        .insert("allow_update_branch".into(), json!(false));
    previous.immutable["enabled"] = json!(false);
    previous.environments.push(Environment {
        name: "release".into(),
        definition: json!({"name":"release","can_admins_bypass":true,
            "deployment_branch_policy":{"protected_branches":false,"custom_branch_policies":true},
            "protection_rules":[{"type":"required_reviewers","prevent_self_review":true,
                "reviewers":[{"type":"User","reviewer":{"id":9}}]}]}),
        branch_policies: vec![json!({"id":4,"name":"v*","type":"tag"})],
    });
    let planned = repo_settings::plan_scoped(
        snapshot(previous.clone()),
        Profile::baseline()?,
        repo_settings::Scope::EnvironmentProtections,
    )?;
    assert_eq!(planned.changes.len(), 1);
    assert!(matches!(
        planned.changes[0].operation,
        Operation::Environment { .. }
    ));
    let api = MemoryApi {
        current: RefCell::new(previous.clone()),
        remove_required_check_on_enable: true,
        ruleset_update_fault: None,
    };
    apply(&api, &planned, |_| Ok(()))?;
    let after = api.current.borrow();
    assert_eq!(after.repository, previous.repository);
    assert_eq!(after.immutable, previous.immutable);
    assert_eq!(after.rulesets, previous.rulesets);
    assert_eq!(
        after.environments[0].branch_policies,
        previous.environments[0].branch_policies
    );
    assert_eq!(
        after.environments[0].definition["protection_rules"],
        previous.environments[0].definition["protection_rules"]
    );
    assert_eq!(after.environments[0].definition["can_admins_bypass"], false);
    let readonly = ReadOnlyApi::new(&previous);
    let mut edited = planned;
    edited.changes.push(repo_settings::Change {
        repository: previous.repository.full_name.clone(),
        operation: Operation::EnableImmutableReleases,
    });
    assert!(apply(&readonly, &edited, |_| Ok(())).is_err());
    assert_eq!(readonly.writes.get(), 0);
    Ok(())
}

#[test]
fn immutability_scope_cannot_change_repository_or_approval_gates() -> Result<()> {
    let mut previous = state();
    previous
        .repository
        .settings
        .insert("allow_update_branch".into(), json!(false));
    previous.immutable["enabled"] = json!(false);
    let planned = repo_settings::plan_scoped(
        snapshot(previous.clone()),
        Profile::baseline()?,
        repo_settings::Scope::ReleaseImmutability,
    )?;
    assert_eq!(planned.changes.len(), 1);
    assert!(matches!(
        planned.changes[0].operation,
        Operation::EnableImmutableReleases
    ));
    let api = MemoryApi {
        current: RefCell::new(previous.clone()),
        remove_required_check_on_enable: false,
        ruleset_update_fault: None,
    };
    apply(&api, &planned, |_| Ok(()))?;
    let after = api.current.borrow();
    assert_eq!(after.immutable["enabled"], true);
    assert_eq!(after.repository, previous.repository);
    assert_eq!(after.rulesets, previous.rulesets);
    assert_eq!(after.environments, previous.environments);
    let readonly = ReadOnlyApi::new(&previous);
    let mut edited = planned;
    edited.changes.push(repo_settings::Change {
        repository: previous.repository.full_name.clone(),
        operation: Operation::CreateRuleset {
            definition: json!({}),
        },
    });
    assert!(apply(&readonly, &edited, |_| Ok(())).is_err());
    assert_eq!(readonly.writes.get(), 0);
    Ok(())
}

#[test]
fn applying_merge_defaults_verifies_only_that_scope_and_preserves_other_gates() -> Result<()> {
    let mut previous = state();
    previous
        .repository
        .settings
        .insert("allow_update_branch".into(), json!(false));
    previous.immutable["enabled"] = json!(false);
    let planned = repo_settings::plan_scoped(
        snapshot(previous.clone()),
        Profile::baseline()?,
        repo_settings::Scope::MergeSettings,
    )?;
    let api = MemoryApi {
        current: RefCell::new(previous.clone()),
        remove_required_check_on_enable: false,
        ruleset_update_fault: None,
    };
    apply(&api, &planned, |_| Ok(()))?;
    let after = api.current.borrow();
    assert_eq!(after.immutable, previous.immutable);
    assert_eq!(after.rulesets, previous.rulesets);
    assert_eq!(after.environments, previous.environments);
    assert!(
        repo_settings::plan_scoped(
            snapshot(after.clone()),
            Profile::baseline()?,
            repo_settings::Scope::MergeSettings
        )?
        .changes
        .is_empty()
    );
    Ok(())
}

impl Api for MemoryApi {
    fn metadata(&self, _: &str) -> Result<Value> {
        Ok(self.current.borrow().metadata.clone())
    }
    fn read(&self, endpoint: &str) -> Result<Value> {
        let state = self.current.borrow();
        if matches!(
            self.ruleset_update_fault,
            Some(RulesetUpdateFault::WrongIdentity)
        ) && endpoint == "repos/owner/project/rulesets/1"
            && state.rulesets[0]["id"] != 1
        {
            return Ok(state.rulesets[0].clone());
        }
        ReadOnlyApi::new(&state).read(endpoint)
    }

    fn write(&self, mutation: repo_settings::Mutation<'_>) -> Result<Value> {
        use repo_settings::Mutation;
        let mut state = self.current.borrow_mut();
        match mutation {
            Mutation::RepositorySettings { settings, .. } => {
                state.repository.settings.extend(settings.clone());
                Ok(json!({}))
            }
            Mutation::EnableImmutableReleases { .. } => {
                state.immutable["enabled"] = json!(true);
                if self.remove_required_check_on_enable {
                    state.rulesets[0]["rules"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|rule| rule["type"] != "required_status_checks");
                }
                Ok(Value::Null)
            }
            Mutation::ProtectEnvironment { name, .. } => {
                state
                    .environments
                    .iter_mut()
                    .find(|env| env.name == name)
                    .unwrap()
                    .definition["can_admins_bypass"] = json!(false);
                Ok(Value::Null)
            }
            Mutation::CreateRuleset { .. } => bail!("unexpected ruleset creation"),
            Mutation::UpdateRuleset {
                id, before, after, ..
            } => {
                let ruleset = state
                    .rulesets
                    .iter_mut()
                    .find(|ruleset| ruleset["id"] == id)
                    .unwrap();
                assert_eq!(ruleset, before);
                match self.ruleset_update_fault {
                    None => *ruleset = after.clone(),
                    Some(RulesetUpdateFault::Ignore) => {}
                    Some(RulesetUpdateFault::DropRequiredCheck) => {
                        *ruleset = after.clone();
                        ruleset["rules"]
                            .as_array_mut()
                            .unwrap()
                            .retain(|rule| rule["type"] != "required_status_checks");
                    }
                    Some(RulesetUpdateFault::WrongIdentity) => {
                        *ruleset = after.clone();
                        ruleset["id"] = json!(id + 1);
                    }
                }
                Ok(ruleset.clone())
            }
            Mutation::SecretScanning { .. } => {
                state.security["secret_scanning"]["status"] = json!("enabled");
                state.security["secret_scanning_push_protection"]["status"] = json!("enabled");
                Ok(json!({}))
            }
            Mutation::ActionsPinning { previous, .. } => {
                let mut next = previous.clone();
                next["sha_pinning_required"] = json!(true);
                state.actions = next;
                Ok(Value::Null)
            }
        }
    }
}

#[test]
fn apply_verifies_settings_and_preserves_existing_gates() {
    let mut previous = state();
    previous.repository.settings.insert(
        "squash_merge_commit_title".into(),
        json!("COMMIT_OR_PR_TITLE"),
    );
    previous.immutable["enabled"] = json!(false);
    previous.environments.push(Environment {
        name: "release".into(),
        definition: json!({"name":"release","can_admins_bypass":true,
            "deployment_branch_policy":{"protected_branches":false,"custom_branch_policies":true},
            "protection_rules":[{"type":"required_reviewers","prevent_self_review":false,
                "reviewers":[{"type":"User","reviewer":{"id":9}}]},{"type":"branch_policy"}]}),
        branch_policies: vec![json!({"id":4,"name":"v*.*.*","type":"tag"})],
    });
    let planned = plan(snapshot(previous.clone()), Profile::baseline().unwrap()).unwrap();
    let api = MemoryApi {
        current: RefCell::new(previous.clone()),
        remove_required_check_on_enable: false,
        ruleset_update_fault: None,
    };
    let mut phases = Vec::new();
    apply(&api, &planned, |event| {
        phases.push(
            serde_json::to_value(event).unwrap()["phase"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        phases,
        [
            "applying", "verified", "applying", "verified", "applying", "verified"
        ]
    );
    let after = api.current.borrow();
    assert_eq!(after.rulesets, previous.rulesets);
    assert_eq!(
        after.environments[0].branch_policies,
        previous.environments[0].branch_policies
    );
    assert_eq!(
        after.environments[0].definition["protection_rules"],
        previous.environments[0].definition["protection_rules"]
    );
    assert_eq!(after.environments[0].definition["can_admins_bypass"], false);
    assert_eq!(after.immutable["enabled"], true);
}

#[test]
fn final_verification_detects_a_lost_ci_gate_even_when_the_baseline_passes() {
    let mut previous = state();
    previous.immutable["enabled"] = json!(false);
    let planned = plan(snapshot(previous.clone()), Profile::baseline().unwrap()).unwrap();
    let api = MemoryApi {
        current: RefCell::new(previous),
        remove_required_check_on_enable: true,
        ruleset_update_fault: None,
    };
    let error = apply(&api, &planned, |_| Ok(())).unwrap_err();
    assert!(error.to_string().contains("existing protections changed"));
}

#[test]
fn failed_mutation_is_reported_and_never_recorded_as_verified() {
    let mut previous = state();
    previous.immutable["enabled"] = json!(false);
    let planned = plan(snapshot(previous.clone()), Profile::baseline().unwrap()).unwrap();
    let api = ReadOnlyApi::new(&previous);
    let mut phases = Vec::new();
    assert!(
        apply(&api, &planned, |event| {
            phases.push(
                serde_json::to_value(event).unwrap()["phase"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
            Ok(())
        })
        .is_err()
    );
    assert_eq!(phases, ["applying", "failed"]);
}

#[test]
fn applying_queue_update_preserves_state_and_verifies_the_complete_ruleset() -> Result<()> {
    for update_settings_first in [false, true] {
        let mut previous = ready_queue_state()?;
        if update_settings_first {
            previous
                .repository
                .settings
                .insert("allow_update_branch".into(), json!(false));
            previous.immutable["enabled"] = json!(false);
        }
        let planned = plan(snapshot(previous.clone()), Profile::baseline()?)?;
        let Operation::UpdateRuleset {
            after: expected, ..
        } = &planned.changes.last().unwrap().operation
        else {
            panic!("expected ruleset update");
        };
        let api = MemoryApi {
            current: RefCell::new(previous.clone()),
            remove_required_check_on_enable: false,
            ruleset_update_fault: None,
        };
        let mut events = Vec::new();
        apply(&api, &planned, |event| {
            events.push(serde_json::to_value(event)?);
            Ok(())
        })?;
        assert_eq!(events.len(), planned.changes.len() * 2);
        for (events, change) in events.as_chunks::<2>().0.iter().zip(&planned.changes) {
            assert_eq!(events[0]["phase"], "applying");
            assert_eq!(events[1]["phase"], "verified");
            assert_eq!(events[0]["change"], serde_json::to_value(change)?);
            assert_eq!(events[1]["change"], serde_json::to_value(change)?);
        }
        let after = api.current.borrow();
        previous.rulesets[0] = expected.clone();
        previous.repository.settings = Profile::baseline()?.repository;
        previous.immutable["enabled"] = json!(true);
        assert_eq!(*after, previous);
    }
    Ok(())
}

#[test]
fn queue_update_write_failure_is_reported_without_verification() -> Result<()> {
    let previous = ready_queue_state()?;
    let planned = plan(snapshot(previous.clone()), Profile::baseline()?)?;
    let api = ReadOnlyApi::new(&previous);
    let mut events = Vec::new();
    let error = apply(&api, &planned, |event| {
        events.push(serde_json::to_value(event)?);
        Ok(())
    })
    .unwrap_err();
    assert!(error.to_string().contains("unexpected write"));
    assert_eq!(api.writes.get(), 1);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["phase"], "applying");
    assert_eq!(events[1]["phase"], "failed");
    assert_eq!(events[1]["change"]["operation"]["kind"], "update_ruleset");
    assert!(
        events[1]["error"]
            .as_str()
            .unwrap()
            .contains("unexpected write")
    );
    Ok(())
}

#[test]
fn queue_update_refuses_a_stale_ruleset_or_workflow_before_any_write() -> Result<()> {
    let previous = ready_queue_state()?;
    let planned = plan(snapshot(previous.clone()), Profile::baseline()?)?;
    for workflow_changed in [false, true] {
        let mut live = previous.clone();
        if workflow_changed {
            live.workflows[0].yaml = "on: pull_request\njobs:\n  actual-ci: {}\n".into();
        } else {
            live.rulesets[0]["bypass_actors"] = json!([{"actor_id": 9, "actor_type": "Integration", "bypass_mode": "pull_request"}]);
        }
        let api = ReadOnlyApi::new(&live);
        let mut events = Vec::new();
        let error = apply(&api, &planned, |event| {
            events.push(serde_json::to_value(event)?);
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("settings changed since audit"));
        assert_eq!(api.writes.get(), 0);
        assert!(events.is_empty());
    }
    Ok(())
}

#[test]
fn queue_update_rechecks_workflows_and_rulesets_immediately_before_writing() -> Result<()> {
    let previous = ready_queue_state()?;
    let planned = plan(snapshot(previous.clone()), Profile::baseline()?)?;
    let workflow_endpoint = format!("repos/owner/project/contents/{QUEUE_WORKFLOW}?ref=main");
    let ruleset_endpoint = "repos/owner/project/rulesets/1";
    let mut changed_ruleset = previous.rulesets[0].clone();
    changed_ruleset["name"] = json!("changed since preflight");
    for (endpoint, after, response, expected_error) in [
        (
            workflow_endpoint.as_str(),
            2,
            json!({"encoding":"base64", "content":STANDARD.encode("on: pull_request\njobs:\n  actual-ci: {}\n")}),
            "settings or workflows changed before ruleset update",
        ),
        (
            ruleset_endpoint,
            2,
            changed_ruleset.clone(),
            "settings or workflows changed before ruleset update",
        ),
        (
            ruleset_endpoint,
            3,
            changed_ruleset,
            "ruleset changed before write",
        ),
    ] {
        let mut api = ReadOnlyApi::new(&previous);
        api.response_changes
            .insert(endpoint.into(), (after, response));
        let mut events = Vec::new();
        let error = apply(&api, &planned, |event| {
            events.push(serde_json::to_value(event)?);
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains(expected_error), "{error:#}");
        assert_eq!(api.writes.get(), 0);
        assert!(events.is_empty());
    }
    Ok(())
}

#[test]
fn queue_update_verification_rejects_an_ignored_or_damaged_ruleset() -> Result<()> {
    let previous = ready_queue_state()?;
    let planned = plan(snapshot(previous.clone()), Profile::baseline()?)?;
    for fault in [
        RulesetUpdateFault::Ignore,
        RulesetUpdateFault::DropRequiredCheck,
        RulesetUpdateFault::WrongIdentity,
    ] {
        let api = MemoryApi {
            current: RefCell::new(previous.clone()),
            remove_required_check_on_enable: false,
            ruleset_update_fault: Some(fault),
        };
        let mut events = Vec::new();
        let error = apply(&api, &planned, |event| {
            events.push(serde_json::to_value(event)?);
            Ok(())
        })
        .unwrap_err();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["phase"], "applying");
        assert_eq!(events[1]["phase"], "failed");
        assert!(
            error
                .to_string()
                .contains("ruleset update verification failed"),
            "{error:#}"
        );
    }
    Ok(())
}
