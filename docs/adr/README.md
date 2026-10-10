# Architecture decisions

`just gen` writes this index from the front matter of each record.

| Record | Status | Changed by |
| --- | --- | --- |
| [0001 Shared skills and typed repository governance](0001-shared-skills-and-governance.md) | accepted |  |
| [0002 Enforce a local CodeRabbit review budget](0002-coderabbit-local-review-budget.md) | amended | [0007](0007-reserved-review-capacity.md) |
| [0003 Local shared-skill operations](0003-local-skill-operations.md) | amended | [0012](0012-windows-proofs-in-a-linux-container.md), [0014](0014-per-skill-decision-records.md), [2026-10-07](checked-obligation-manifest.md) |
| [0004 Checked pull request authoring](0004-checked-pull-request-authoring.md) | accepted |  |
| [0005 Native desktop integration assurance](0005-native-desktop-integration-assurance.md) | accepted |  |
| [0006 Global personal issue prerequisite](0006-global-personal-issue-prerequisite.md) | accepted |  |
| [0007 Preserve included review capacity](0007-reserved-review-capacity.md) | amended | [0008](0008-layered-review-capacity.md) |
| [0008 Apply layered review-capacity budgets](0008-layered-review-capacity.md) | accepted |  |
| [0009 Public source with native profiles and Rust orchestration](0009-public-native-profiles.md) | accepted |  |
| [0010 Rehearse the native apply and declare every external tool](0010-rehearse-native-application.md) | accepted |  |
| [0011 Run one Git policy on every host](0011-one-git-policy-on-every-host.md) | accepted |  |
| [0012 Run the Kani proofs on Windows in a pinned Linux container](0012-windows-proofs-in-a-linux-container.md) | accepted |  |
| [0013 Mark a PR ready only after every check passes](0013-ready-only-after-passing-checks.md) | accepted |  |
| [0014 Bind tracked skill decisions to each skill revision](0014-per-skill-decision-records.md) | accepted |  |
| [0015 Scope the CodeRabbit owner pause](0015-scoped-coderabbit-pause.md) | accepted |  |
| [0016 Enforce one writing standard on persisted prose and replies](0016-one-writing-standard.md) | amended | [2026-10-10](persisted-prose-gates.md) |
| [0017 Normalize line endings to LF on every host and agent write](0017-lf-line-endings-everywhere.md) | accepted |  |
| [0018 Turn off persistent agent memory in the native client profiles](0018-agent-memory-off.md) | accepted |  |
| [0019 Recommend the next workflow step from inspected state](0019-state-inspecting-next-action.md) | accepted |  |
| [0020 Require a passing gate on every operating system family before push](0020-os-family-push-evidence.md) | accepted |  |
| [0021 Check that every agent instruction line has a class](0021-checked-agent-instruction-audit.md) | accepted |  |
| [0022 Refuse installed tools built from stale sources](0022-refuse-tools-built-from-stale-sources.md) | accepted |  |
| [0023 Give the repository a layout that shows what it manages](0023-repository-layout-and-target-architecture.md) | accepted |  |
| [2026-10-05 Reuse the `gh` login for the remote GitHub tool server](github-mcp-uses-the-github-cli-login.md) | accepted |  |
| [2026-10-06 Report every gate refusal as one structured record](structured-gate-refusals.md) | accepted |  |
| [2026-10-07 Enforce formal obligations and deterministic metrics through one repository manifest](checked-obligation-manifest.md) | accepted |  |
| [2026-10-07 Name new decision records after their decision and place them by date](dated-decision-record-names.md) | accepted |  |
| [2026-10-07 Refuse generic anti-patterns through one catalog of checks](generic-anti-pattern-catalog.md) | accepted |  |
| [2026-10-07 Keep every decision in one pure core crate behind its adapters](pure-decision-core-behind-adapters.md) | accepted |  |
| [2026-10-07 Replay the whole reachable state graph of each finite Quint model](state-graph-replay-of-finite-quint-models.md) | accepted |  |
| [2026-10-07 Require verification in proportion to code shape on every repository](verification-proportional-to-code-shape.md) | accepted |  |
| [2026-10-08 Drop Aeneas extraction for the plan and ordering cores and try Verus next](aeneas-extraction-pilot.md) | accepted |  |
| [2026-10-08 Record rule failures at their execution boundaries](automatic-rule-failure-observations.md) | proposed |  |
| [2026-10-08 Keep bounded harnesses instead of Kani loop contracts for the Git argument scan](bounded-harness-over-kani-loop-contracts.md) | accepted |  |
| [2026-10-08 Bind installed tools to reviewed skills](installed-tools-and-reviewed-skills.md) | proposed |  |
| [2026-10-08 Keep property fuzzing in the fuzz targets instead of bolero](no-bolero-beside-kani.md) | accepted |  |
| [2026-10-08 Generate repository artifacts from checked metadata](repository-metadata-and-generated-artifacts.md) | proposed |  |
| [2026-10-09 Compose the bounded Git argument proofs through a scalar cursor](compose-bounded-git-argument-proofs.md) | accepted |  |
| [2026-10-09 Check review postconditions directly on bounded production calls](direct-bounded-review-postconditions.md) | accepted |  |
| [2026-10-09 Bind incident prevention to exact records and committed evidence](incident-dispositions-bind-exact-records.md) | accepted |  |
| [2026-10-09 Optimize checked MIR for the pinned Kani verifier](optimize-checked-kani-mir.md) | accepted |  |
| [2026-10-09 Preserve source witnesses under checked Kani MIR](preserve-kani-source-covers.md) | accepted |  |
| [2026-10-09 Validate skill catalogs before planning](validated-skill-plan-inputs.md) | accepted |  |
| [2026-10-10 Check prose before persistence](persisted-prose-gates.md) | accepted |  |
