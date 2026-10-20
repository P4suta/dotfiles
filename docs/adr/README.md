# Architecture decisions

`just gen` writes this index from the front matter of each record.

| Record | Status | Changed by |
| --- | --- | --- |
| [0001 Shared skills and typed repository governance](0001-shared-skills-and-governance.md) | accepted |  |
| [0002 Enforce a local CodeRabbit review budget](0002-coderabbit-local-review-budget.md) | amended | [0007](0007-reserved-review-capacity.md) |
| [0003 Local shared-skill operations](0003-local-skill-operations.md) | amended | [0012](0012-windows-proofs-in-a-linux-container.md), [0014](0014-per-skill-decision-records.md), [2026-10-07](checked-obligation-manifest.md), [2026-10-13](experimental-skill-maintenance-by-explicit-opt-in.md) |
| [0004 Checked pull request authoring](0004-checked-pull-request-authoring.md) | accepted |  |
| [0005 Native desktop integration assurance](0005-native-desktop-integration-assurance.md) | accepted |  |
| [0006 Global personal issue prerequisite](0006-global-personal-issue-prerequisite.md) | accepted |  |
| [0007 Preserve included review capacity](0007-reserved-review-capacity.md) | amended | [0008](0008-layered-review-capacity.md) |
| [0008 Apply layered review-capacity budgets](0008-layered-review-capacity.md) | amended | [2026-10-13](route-coderabbit-by-current-pr-importance.md) |
| [0009 Public source with native profiles and Rust orchestration](0009-public-native-profiles.md) | accepted |  |
| [0010 Rehearse the native apply and declare every external tool](0010-rehearse-native-application.md) | accepted |  |
| [0011 Run one Git policy on every host](0011-one-git-policy-on-every-host.md) | accepted |  |
| [0012 Run the Kani proofs on Windows in a pinned Linux container](0012-windows-proofs-in-a-linux-container.md) | accepted |  |
| [0013 Mark a PR ready only after every check passes](0013-ready-only-after-passing-checks.md) | accepted |  |
| [0014 Bind tracked skill decisions to each skill revision](0014-per-skill-decision-records.md) | accepted |  |
| [0015 Scope the CodeRabbit owner pause](0015-scoped-coderabbit-pause.md) | amended | [2026-10-13](route-coderabbit-by-current-pr-importance.md), [2026-10-20](keep-workflow-evidence-local.md) |
| [0016 Enforce one writing standard on persisted prose and replies](0016-one-writing-standard.md) | amended | [2026-10-10](persisted-prose-gates.md) |
| [0017 Normalize line endings to LF on every host and agent write](0017-lf-line-endings-everywhere.md) | accepted |  |
| [0018 Turn off persistent agent memory in the native client profiles](0018-agent-memory-off.md) | accepted |  |
| [0019 Recommend the next workflow step from inspected state](0019-state-inspecting-next-action.md) | accepted |  |
| [0020 Require a passing gate on every operating system family before push](0020-os-family-push-evidence.md) | amended | [2026-10-11](profile-evidence-scope.md), [2026-10-11](source-bound-push-validation.md), [2026-10-19](optional-native-verification-before-publication.md) |
| [0021 Check that every agent instruction line has a class](0021-checked-agent-instruction-audit.md) | accepted |  |
| [0022 Refuse installed tools built from stale sources](0022-refuse-tools-built-from-stale-sources.md) | amended | [2026-10-11](source-bound-push-validation.md) |
| [0023 Give the repository a layout that shows what it manages](0023-repository-layout-and-target-architecture.md) | accepted |  |
| [2026-10-05 Reuse the `gh` login for the remote GitHub tool server](github-mcp-uses-the-github-cli-login.md) | accepted |  |
| [2026-10-06 Report every gate refusal as one structured record](structured-gate-refusals.md) | accepted |  |
| [2026-10-07 Enforce formal obligations and deterministic metrics through one repository manifest](checked-obligation-manifest.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-07 Name new decision records after their decision and place them by date](dated-decision-record-names.md) | accepted |  |
| [2026-10-07 Refuse generic anti-patterns through one catalog of checks](generic-anti-pattern-catalog.md) | accepted |  |
| [2026-10-07 Keep every decision in one pure core crate behind its adapters](pure-decision-core-behind-adapters.md) | accepted |  |
| [2026-10-07 Replay the whole reachable state graph of each finite Quint model](state-graph-replay-of-finite-quint-models.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-07 Require verification in proportion to code shape on every repository](verification-proportional-to-code-shape.md) | amended | [2026-10-20](keep-the-assure-owner-map-out-of-projects.md) |
| [2026-10-08 Drop Aeneas extraction for the plan and ordering cores and try Verus next](aeneas-extraction-pilot.md) | accepted |  |
| [2026-10-08 Record rule failures at their execution boundaries](automatic-rule-failure-observations.md) | proposed |  |
| [2026-10-08 Keep bounded harnesses instead of Kani loop contracts for the Git argument scan](bounded-harness-over-kani-loop-contracts.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-08 Bind installed tools to reviewed skills](installed-tools-and-reviewed-skills.md) | proposed |  |
| [2026-10-08 Keep property fuzzing in the fuzz targets instead of bolero](no-bolero-beside-kani.md) | accepted |  |
| [2026-10-08 Generate repository artifacts from checked metadata](repository-metadata-and-generated-artifacts.md) | proposed |  |
| [2026-10-09 Compose the bounded Git argument proofs through a scalar cursor](compose-bounded-git-argument-proofs.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-09 Check review postconditions directly on bounded production calls](direct-bounded-review-postconditions.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-09 Bind incident prevention to exact records and committed evidence](incident-dispositions-bind-exact-records.md) | accepted |  |
| [2026-10-09 Optimize checked MIR for the pinned Kani verifier](optimize-checked-kani-mir.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-09 Preserve source witnesses under checked Kani MIR](preserve-kani-source-covers.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-09 Validate skill catalogs before planning](validated-skill-plan-inputs.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-10 Retain applied profile drift as advisory evidence](advisory-applied-profile-observations.md) | accepted |  |
| [2026-10-10 Isolate code gates and retain native provisioning checks](isolated-code-and-native-host-gates.md) | amended | [2026-10-10](advisory-applied-profile-observations.md) |
| [2026-10-10 Check prose before persistence](persisted-prose-gates.md) | accepted |  |
| [2026-10-11 Read GitHub credentials without mise shims](github-credentials-without-mise-shims.md) | proposed |  |
| [2026-10-11 Preserve tool versions during application](preserve-tool-versions-during-apply.md) | proposed |  |
| [2026-10-11 Bind profile evidence to repositories that own the profile runtime](profile-evidence-scope.md) | proposed |  |
| [2026-10-11 Start mise installations without mise shims](shim-free-mise-installation.md) | proposed |  |
| [2026-10-11 Push the candidate its gates already verified](source-bound-push-validation.md) | amended | [2026-10-12](local-proof-campaigns-on-mac.md), [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-12 Keep fixed processor budgets beside Mac proof campaigns](fixed-cpu-budgets-beside-mac-proof-campaigns.md) | amended | [2026-10-13](local-mutation-and-scoped-ci-proofs.md), [2026-10-19](native-verification-with-lightweight-ci.md) |
| [2026-10-12 Run local proof campaigns on Mac and keep Linux CI](local-proof-campaigns-on-mac.md) | amended | [2026-10-12](fixed-cpu-budgets-beside-mac-proof-campaigns.md), [2026-10-19](native-verification-with-lightweight-ci.md), [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-12 Classify native failures from delivery signals before output replays](native-failure-signals-before-output-replays.md) | proposed |  |
| [2026-10-12 Recover native PATH before constructing candidate environments](native-path-before-candidate-environments.md) | proposed |  |
| [2026-10-12 Persist remote execution ownership before submission](persist-remote-execution-ownership.md) | proposed |  |
| [2026-10-12 Bind acquisition plans to pinned inputs and reusable material identities](pinned-acquisition-plans-and-content-addressed-materials.md) | proposed |  |
| [2026-10-13 Admit native work from a complete resource plan](admit-native-work-from-a-complete-resource-plan.md) | proposed |  |
| [2026-10-13 Assess a pinned mutation engine before adoption](assess-pinned-mutation-engine-before-adoption.md) | proposed |  |
| [2026-10-13 Authenticate issue context at edit and integration boundaries](authenticate-issue-context-at-edit-and-integration-boundaries.md) | amended | [2026-10-20](keep-workflow-evidence-local.md) |
| [2026-10-13 Bind private verifier phase evidence](bind-private-verifier-phase-evidence.md) | proposed |  |
| [2026-10-13 Enable experimental skill maintenance only through explicit local opt-in](experimental-skill-maintenance-by-explicit-opt-in.md) | accepted |  |
| [2026-10-13 Keep local issue authority separate from remote publication](keep-local-issue-authority-separate-from-remote-publication.md) | proposed |  |
| [2026-10-13 Keep mutation testing local while CI verifies its declared proof campaign](local-mutation-and-scoped-ci-proofs.md) | amended | [2026-10-19](native-verification-with-lightweight-ci.md) |
| [2026-10-13 Protect local host candidate execution](protect-local-host-candidate-execution.md) | proposed |  |
| [2026-10-13 Run Renovate through a process from the same source build](renovate-behind-a-source-matched-process.md) | proposed |  |
| [2026-10-13 Require every fixed proof partition in hosted CI](require-every-fixed-proof-partition-in-hosted-ci.md) | superseded | [2026-10-19](native-verification-with-lightweight-ci.md) |
| [2026-10-13 Retain Miri sysroots with fresh interpretation](retain-miri-sysroots-with-fresh-interpretation.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-13 Route CodeRabbit by current PR importance](route-coderabbit-by-current-pr-importance.md) | amended | [2026-10-20](keep-workflow-evidence-local.md) |
| [2026-10-13 Speak fixed development events through an optional desktop queue](speak-fixed-development-events-through-an-optional-desktop-queue.md) | amended | [2026-10-14](report-optional-notification-failures-with-fixed-warnings.md) |
| [2026-10-13 Validate Renovate reports before classifying freshness](validated-renovate-reports.md) | proposed |  |
| [2026-10-14 Partition Cargo artifacts by physical source root](partition-cargo-artifacts-by-physical-source-root.md) | proposed |  |
| [2026-10-14 Report optional notification failures with fixed warnings](report-optional-notification-failures-with-fixed-warnings.md) | proposed |  |
| [2026-10-15 Prove portable component scans with erased witnesses](prove-portable-component-scans-with-erased-witnesses.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-16 Evaluate fixed parser domains before symbolic selection](finite-shell-parser-ctfe.md) | amended | [2026-10-20](proof-verdicts-never-depend-on-time.md) |
| [2026-10-17 Run spoken Git commands in the matching policy companion](isolate-spoken-git-in-the-matching-companion.md) | proposed |  |
| [2026-10-19 Bind inherited Renovate Cargo records to committed workspace metadata](cargo-workspace-renovate-bindings.md) | proposed |  |
| [2026-10-19 Make native verification authoritative and keep hosted CI lightweight](native-verification-with-lightweight-ci.md) | amended | [2026-10-19](optional-native-verification-before-publication.md), [2026-10-20](gate-pull-requests-that-skip-local-hooks.md) |
| [2026-10-19 Make native verification optional before publication](optional-native-verification-before-publication.md) | amended | [2026-10-20](gate-pull-requests-that-skip-local-hooks.md) |
| [2026-10-19 Repository owner assessments for bot pull requests](repository-owner-assessments-for-bot-pull-requests.md) | proposed |  |
| [2026-10-20 Admit Git operations that lose no owner work](admit-tool-owned-and-throwaway-git-operations.md) | accepted |  |
| [2026-10-20 Gate pull requests that skip the local hooks](gate-pull-requests-that-skip-local-hooks.md) | accepted |  |
| [2026-10-20 Keep parallel worktrees independent](keep-parallel-worktrees-independent.md) | amended | [2026-10-20](leave-the-cargo-target-to-each-workspace.md) |
| [2026-10-20 Keep the assure owner map out of projects](keep-the-assure-owner-map-out-of-projects.md) | accepted |  |
| [2026-10-20 Keep workflow evidence on the owner's machines](keep-workflow-evidence-local.md) | accepted |  |
| [2026-10-20 Leave the Cargo target to each workspace](leave-the-cargo-target-to-each-workspace.md) | accepted |  |
| [2026-10-20 Proof verdicts never depend on time](proof-verdicts-never-depend-on-time.md) | accepted |  |
| [2026-10-20 Keep every managed behavior the same on every host](same-behavior-on-every-host.md) | accepted |  |
| [2026-10-21 Apply from the configured source and replace tools atomically](apply-from-the-configured-source-and-replace-tools-atomically.md) | proposed |  |
| [2026-10-21 Keep hosts on the merged main](keep-hosts-on-the-merged-main.md) | accepted |  |
| [2026-10-21 Sign through the password manager agent](sign-through-the-password-manager-agent.md) | accepted |  |
