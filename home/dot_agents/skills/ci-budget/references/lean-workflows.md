# Lean hosted workflows

The project's own check command stays the authority, and hosted CI runs its fast gate plus what only GitHub can observe.
A hosted run that checks nothing never gates a merge, because an automatic merge then trusts nothing.

## Every change

- Run format, lint, type checks, and unit and integration tests on one pinned Linux runner.
- Lint other supported targets by cross-compiling on that runner where the toolchain supports it.
- Expose one required summary job, and keep every other job behind it.
- Trigger on `pull_request` and `merge_group`, and never also on `push` for the same commits.
- Cancel a superseded run of the same ref with `concurrency`.
- Use job timeouts only as safety stops with inconclusive results, and skip jobs a change can't affect with path filters while the summary job still reports.
- Cache dependencies by lockfile digest.
- Upload an artifact only when a person downloads it, and keep it seven days at most.
- Pin actions by commit SHA and let Renovate move them.

## The release gate

Heavy checks gate a release, never a commit, push, pull request, ready transition, or merge.
Run them on the exact candidate commit in the candidate verification stage of `release-workflow`, and publish only after they pass.
Use the project's pinned container or documented hosted runner for each one, and follow `formal-assurance` for exploration bounds and incomplete results.

| Check | Release candidate |
| --- | --- |
| Formal proofs | Full proof inventory, model checks, and Miri |
| Fuzzing | Committed corpus replay and configured exploration, with new crashes kept as seeds |
| Mutation testing | Full configured operator set |
| Stress repeats | Configured fixed-count repeats of concurrency and ordering scenarios |
| Benchmarks | Deterministic operation counts, with timings as observations, never verdicts |
| macOS and Windows | Every supported release target |

A developer may run one of these on demand through the same entrypoint when a change needs its answer, and that run gates nothing.
Keep CI lean by reusing exact-source evidence, never by requiring the owner's hosts.

## Schedules

Add no `schedule` trigger.
Renovate already runs the periodic work through ordinary pull requests, and those pull requests run the fast gate.
A check of external drift that needs a clock runs weekly at most on one Linux runner, opens an issue on failure, and states its reason in the workflow.

## Dependency updates

An automatic merge requires the fast gate on the update's own pull request.
A toolchain update also updates every fixture and catalog that names the version, or the gate refuses it.
