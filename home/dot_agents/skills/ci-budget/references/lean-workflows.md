# Lean hosted workflows

The project's own check command stays the authority, and hosted CI runs the checks selected by affected contracts plus what only GitHub can observe.
A hosted run that checks nothing never gates a merge, because an automatic merge then trusts nothing.

## Every change

- Run format, lint, type checks, and unit and integration tests on one pinned Linux runner.
- Expose one required summary job, and keep every other job behind it.
- Trigger on `pull_request` and `merge_group`, and never also on `push` for the same commits.
- Cancel a superseded run of the same ref with `concurrency`.
- Use job timeouts only as safety stops with inconclusive results, and skip jobs a change can't affect with path filters while the summary job still reports.
- Cache dependencies by lockfile digest.
- Upload an artifact only when a person downloads it, and keep it seven days at most.
- Pin actions by commit SHA and let Renovate move them.

## Where heavy checks run

Use the project's pinned container or documented hosted runner for every selected check.
Follow `formal-assurance` for exploration bounds and incomplete results.

| Check | Selected change | On demand or release |
| --- | --- | --- |
| Fuzzing | Replay the committed corpus and explore affected input boundaries | Broader configured exploration with new crashes kept as seeds |
| Mutation testing | Changed contracts and their dependencies | Full configured operator set |
| Formal proofs | Changed contracts and their dependencies | Full proof inventory |
| Benchmarks | Deterministic operation counts for affected algorithms | Timings as observations, never verdicts |
| macOS and Windows | Affected platform behavior | Supported release targets |

Keep CI lean by selecting relevant checks and reusing exact-source evidence, never by requiring the owner's hosts.

## Schedules

Add no `schedule` trigger.
Renovate already runs the periodic work through ordinary pull requests, and those pull requests run the fast gate.
A check of external drift that needs a clock runs weekly at most on one Linux runner, opens an issue on failure, and states its reason in the workflow.

## Dependency updates

An automatic merge requires the fast gate on the update's own pull request.
A toolchain update also updates every fixture and catalog that names the version, or the gate refuses it.
