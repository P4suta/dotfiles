# Lean hosted workflows

The project's one local command stays the authority, and hosted CI repeats its fast deterministic part plus what only GitHub can observe.
A hosted run that checks nothing never gates a merge, because an automatic merge then trusts nothing.

## Every change

- Run format, lint, type checks, and unit and integration tests on one Linux runner within ten minutes.
- Expose one required summary job, and keep every other job behind it.
- Trigger on `pull_request` and `merge_group`, and never also on `push` for the same commits.
- Cancel a superseded run of the same ref with `concurrency`.
- Give every job `timeout-minutes`, and skip jobs a change can't affect with path filters while the summary job still reports.
- Cache dependencies by lockfile digest.
- Upload an artifact only when a person downloads it, and keep it seven days at most.
- Pin actions by commit SHA and let Renovate move them.

## Where heavy checks run

| Check | Hosted pull request | Local or native host | On demand or release |
| --- | --- | --- | --- |
| Fuzzing | Replay the committed corpus as ordinary tests | Time-boxed campaigns that commit new crashes as seeds | `workflow_dispatch` |
| Mutation testing | Never | Diff-scoped runs before review | Full runs before a release |
| Formal proofs | Changed modules, when they finish in minutes | The project gate | The full campaign before a release |
| Benchmarks | Never as a gate | Fixed-budget local runs | On demand |
| macOS and Windows | Code with platform-specific paths only | The owner's machines through `multi-machine` | Release builds |

A macOS minute costs about ten Linux minutes and a Windows minute about two, so a matrix entry needs a platform-specific path to earn its place.

## Schedules

Add no `schedule` trigger.
Renovate already runs the periodic work through ordinary pull requests, and those pull requests run the fast gate.
A check of external drift that needs a clock runs weekly at most on one Linux runner, opens an issue on failure, and states its reason in the workflow.

## Dependency updates

An automatic merge requires the fast gate on the update's own pull request.
A toolchain update also updates every fixture and catalog that names the version, or the gate refuses it.
