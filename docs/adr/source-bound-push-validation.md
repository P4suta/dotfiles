---
date: 2026-10-11
status: amended
changed_by: [local-proof-campaigns-on-mac.md]
---
# Push the candidate its gates already verified

## Context

The hosts gate recorded `just check`, while pre-push reran it under the Git hook environment and ran the proof suite separately.
A successful hosts result therefore omitted checks that could first fail during push.
Only registered tools removed Git's repository variables, so directly constructed child processes could use a hook's index or object store.
The installed workflow tools refreshed after merges but became stale after commits that changed their sources.
Reporting pending incident maintenance as a new tool failure also enlarged the work required to finish it.

## Decision

The hosts gate runs `just push-check` through its isolated code environment.
That command runs every existing check on each required native family and collects the complete proof suite on Linux, the target of the CI proof job and the former Windows pre-push proof container.
Both stages must complete, even when one fails.
Independent proof batches, modules, and verifier stages collect their failures before refusing the candidate.
Invalid shared prerequisites refuse their dependent work, while unrelated verifiers still run.
Only a complete successful inventory contributes verified counts.
The prose gate checks both document and source comment style, matching the staged check and the standalone prose command.
Pre-push validates those recorded results instead of rerunning a different check or proof command.
The global signature, history, health, owner pause, assurance, and PR state gates still inspect the current push.

Admission binds each stage record to the candidate commit, its tree, its stage input hash, and the canonical code command.
Old `just check` results, custom code commands, and incomplete records cannot authorize a push.
The code stage hashes every checked file, including profiles.
Its scratch home borrows only the selected Kani release bundle, preserving fixed verifier storage without inheriting host configuration or repeating setup downloads.
The real stage hashes profile files, runtime sources, manifests, and shared pins.
An unchanged stage retains the existing passing-result reuse, while changed inputs require another run.
Each completed job records its execution order under a short lock shared by the repository's worktrees.
The latest matching execution determines whether a stage can be reused, and a failure wins an equal-order conflict.
A reused receipt retains the original execution's order and provenance, so copying an old success cannot cover a later failure.
Recovering a partially recorded job retains its first execution order and never allocates a newer one for a replay.
Writing reused evidence rechecks that execution's freshness, and admission checks it across all commits.
Legacy receipts without an execution order cannot authorize reuse or admission.
After a new execution passes, subsequent matching candidates can reuse that verified result again.
The workflow matrix must retain Linux so the proof suite participates in admission.
The ordinary Rust checks discover every independent Cargo workspace from tracked manifests and validate every lockfile with Cargo before the expensive proof and benchmark stages.
Missing and stale lockfiles fail together, including those of auxiliary tools and proof fixtures.

The [process constructor](../../crates/platform/src/process.rs) removes Git's repository-local variables for every ordinary child.
Clippy refuses another constructor, including qualified calls and import aliases.
Repository operations explicitly select the inherited context through `repository_command`, preserving alternate indexes for hooks and the Git wrapper without removing and rebuilding its environment.
Their unrelated children use the ordinary constructor and receive no implicit repository context.

Post-commit refreshes recorded workflow tool installations after the signing gate succeeds, using the existing content hashes and installation tasks.
The stamp covers transitive local dependencies, and a Cargo metadata check refuses an omitted dependency source.
Unchanged inputs require no rebuild.
Failed installation leaves the freshness refusal effective and reports the required repair.
Pending maintenance is a distinct checked status, so neither the command nor the native failure collectors can record it as another failure.
The command still refuses completion until the original obligations are discharged.
Discharge reruns every cited regression test and lets Cargo reuse this checkout's compiled artifacts without reusing a test outcome.
Kani compiles bounded batches of each production module or probe file's declared harnesses instead of recompiling for each harness.
Each report retains the existing JSON size limit, and validation rejects duplicate harnesses across batch boundaries.
The proof gate runs the real Kani driver regression explicitly and refuses a missing or unsuccessful test.
Bounded classifier inputs select any member of the same word set directly, and each prefix materializes the selected case before checking the production classifier against its specification.
This preserves every three-word case while removing symbolic string-table aliasing and remainder arithmetic.
The refinement of the skill plan uses the same independent Lean algorithm with temporary arrays sized for the existing proof domain of five skills.
Its temporary skill indices use checked one-byte values for the same zero-through-seven domain, and the result restores every slot to the original representation.
Its native reference retains the original eight-slot capacity, and the public implementation and every proof input, assertion, cover witness, and unwind bound remain unchanged.
The result validator still checks every harness, property, cover witness, counterexample, and individual time budget.
Linux's timed code stage finishes before real provisioning begins, so setup compilation cannot consume its proof budget.
Other families retain concurrent stages only with the existing core and storage capacity.
Credential fixtures construct their tool lookup path instead of copying the caller's unrelated tools.
A regression verifies the fixture under a parent PATH beyond the [Windows command interpreter's limit](https://learn.microsoft.com/en-us/troubleshoot/windows-client/shell-experience/command-line-string-limitation).
The shared mise installation constructor removes duplicate PATH entries while preserving their first occurrence and lookup order.
On Windows it counts the resulting UTF-16 code units and refuses a PATH that cmd would silently discard, before starting the installer or taking its lock.
The pure length decision has an exhaustive Kani harness and rejecting boundary probes, and native tests check supplementary characters and duplicate reduction.
OpenCode's desired configuration declares domyjob's MCP server from the installed build pin and its regular executable.
The renderer rejects malformed or incomplete installations and writes the same compact server value as domyjob's normal setup, so both converge on the declared configuration.
Without an installed pin, every profile retains its previous rendering.
Configuration regeneration records the actual working tree as `sourceDir`, preserving the rooted source for subsequent commands.
Candidate commands redirect Go's writable paths into their owned environment and discard inherited Go configuration that could redirect those writes.
Candidate cleanup retains a directory descriptor from that environment's creation and prepares its directories only after its child processes finish.
The operating system's directory descriptors and no-follow opens keep this traversal inside the owned tree without following links.
The proved permission rule adds only the owner's write and search permissions, preserving every other bit.
Regular files and link targets retain their permissions throughout cleanup, including files linked from outside the environment.
The ordinary symlink-safe directory removal performs deletion, and preparation or removal failures report the affected path without replacing the candidate's original result.
Native regressions check actual read-only directories, partial installations, outside targets, descriptor-bound replacements, and inherited Go settings.

## Alternatives

Running every pre-push check again inside the hosts gate would retain duplicate verification and a dependency on installed home tools.
Recording one canonical gate result preserves the isolated checks and makes push admission a bounded read of candidate evidence.
Removing a failing check or accepting an old result would omit evidence and remains prohibited.
Repeating the Linux proof suite on each family would repeat the former pre-push proof target without adding a native-platform check.
Native compilation, tests, profile rendering, and runtime checks remain required on each family.

## Consequences

The first candidate needs new complete records on every required family.
Subsequent pushes consume those records without repeating their expensive work.
Changes to a profile also invalidate code checks because those checks inspect profiles, which narrows the earlier reuse boundary.
The [host tests](../../crates/dotfiles/tests/hosts.rs), [process tests](../../crates/platform/tests/tool_environment.rs), [hook rules](../../crates/rules/src/hook_rules.rs), and [maintenance test](../../crates/skill-ops/tests/skill_runtime.rs) cover the affected boundaries.
