use anyhow::{Result, bail};
use repo_settings::{
    Api, Environment, Operation, Profile, Repository, Snapshot, State, apply, plan,
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

fn state() -> State {
    State {
        repository: Repository {
            id: 7,
            full_name: "owner/project".into(),
            owner_id: 9,
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
                    {"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"actual-ci","integration_id":15368}]}}]}),
            json!({"id":2,"name":"tags","target":"tag","enforcement":"active","bypass_actors":[],
                "conditions":{"ref_name":{"include":["refs/tags/v*"],"exclude":[]}},
                "rules":[{"type":"update"},{"type":"deletion"},{"type":"non_fast_forward"}]}),
        ],
        environments: Vec::new(),
        workflow_permissions: json!({"default_workflow_permissions":"read","can_approve_pull_request_reviews":false}),
    }
}

fn snapshot(state: State) -> Snapshot {
    Snapshot {
        schema_version: 1,
        repositories: vec![state],
    }
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
}

impl ReadOnlyApi {
    fn new(state: &State) -> Self {
        let root = format!("repos/{}", state.repository.full_name);
        let mut raw = serde_json::to_value(&state.repository.settings).unwrap();
        raw["id"] = json!(state.repository.id);
        raw["full_name"] = json!(state.repository.full_name);
        raw["owner"] = json!({"id":state.repository.owner_id});
        raw["default_branch"] = json!(state.repository.default_branch);
        raw["archived"] = json!(state.repository.archived);
        raw["fork"] = json!(state.repository.fork);
        raw["permissions"] = json!({"admin":state.repository.admin});
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
        ]);
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
        }
    }
}

impl Api for ReadOnlyApi {
    fn read(&self, endpoint: &str) -> Result<Value> {
        let mut response = self
            .responses
            .get(endpoint)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unconfigured request {endpoint}"))?;
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
    fn read(&self, endpoint: &str) -> Result<Value> {
        ReadOnlyApi::new(&self.current.borrow()).read(endpoint)
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
