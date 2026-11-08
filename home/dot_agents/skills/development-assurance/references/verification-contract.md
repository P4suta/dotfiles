# Optional execution receipts

Use the runner when a task needs ordered native execution receipts beyond the project's maintained commands.
Agent edit and reply hooks neither require a plan nor inspect the runner's state.
Initialize an explicitly requested run with `dotfiles answer begin`, supplying `session_id`, `cwd`, and `prompt` as JSON on standard input.
For an explicit run, register a JSON plan with `dotfiles verify begin --session SESSION --plan FILE` from the target checkout.
Use the managed session ID from the current request.
The explicit registration captures the checkout's baseline.
Preserve existing obligations and check commands throughout the request.
The plan may add risks, checks, or contracts.

Each contract names a repository-relative file or directory, a change of `behavior`, `refactor`, `configuration`, or `verification`, its `risks`, and its check IDs.
For a refactor, use `dotfiles verify run --session SESSION --check ID --baseline` before production changes and require passing tests afterward.
Verification-only contracts must leave production unchanged.
Configuration contracts admit manifests, locks, and configuration data and require current checks without imposing an artificial mutation of dependency records.
Assign production code to behavior contracts.
Every behavior contract requires behavioral tests and mutation results.
The risks `pure`, `input`, `state`, `effects`, `concurrency`, and `unsafe` add proof, fuzz, model, fault-injection, model with stress, and memory evidence respectively.
Declare every applicable risk.
The gate additionally requires proofs for changed Rust rule modules.
Risk classification and the adequacy of the selected assertions remain review judgments.

Each check names an `id`, `method`, `tool` of `cargo`, `just`, or `dotfiles`, an `arguments` array, a `report`, and a nonempty `expected` inventory.
The `dotfiles` tool accepts `bounded run` with an explicit `--` command separator for the maintained container route.
Its `scope` defaults to `affected`.
Release checks require `full`.
Use named project verification commands.
The runner uses the project's mise context and binds results to the request, source tree, plan, command, and resolved tool.
The runner hashes relevant compiler variables and the extra variable names in a check's `environment` array without retaining their values.
It invalidates a previous result before a new attempt and refuses source drift during execution.
An unfinished contract survives another user message.
A completed change starts a fresh TDD cycle for the next request.

Run `dotfiles verify run --session SESSION --check ID --red` for the failing behavioral test before changing production.
The libtest producer and structured test adapters require actual completed assertion failures.
Raw compiler errors, empty runs, and ignored tests leave red unverified.
An explicit compiler-contract test may establish the failed type contract through a checked structured adapter.
Rust production comparison excludes parsed test and proof items and documentation.
Other languages require tests in recognized test paths or a project-native adapter.
Run the same check without `--red` after implementation.
Run every other applicable check, then `dotfiles verify check --session SESSION`.
Agent Stop and answer gates leave the runner independent.
Unsupported execution evidence leaves the obligation incomplete.

The `libtest` report supports tests, fault injection tests, and stress tests.
It requires every named test and refuses ignored or failing results.
The `kani` report consumes the maintained `proofs` command's completed production and rejecting inventories, with rejecting harness names in `rejecting`.
The `mutants` report requires ignored `report_path` and pinned `verifier_version` fields.
Its `expected` entries name source files or directories.
It checks the native [cargo-mutants inventory and outcomes](https://mutants.rs/mutants-out.html), requiring every selected scenario, a successful baseline, caught mutants for each selected source, and no survivors or timeouts.

Other project-native adapters use `structured` JSON on standard output.
The report contains `version: 1`, `source` from `DOTFILES_VERIFICATION_SOURCE`, `plan` from `DOTFILES_VERIFICATION_PLAN`, the check's `method` and `scope`, nonempty `tool_version`, `target`, and `bounds`, and `results` with `name`, `status`, `executed`, `failed`, `skipped`, and `inconclusive` entries.
Green requires nonempty executed work, `passed`, and zero failed, skipped, or inconclusive work.
Behavioral red requires a failed execution and actual counted assertion failures.
The adapter must execute and validate the actual verifier, its complete applicable inventory, and its native results.
Printing a self-authored success report supplies no evidence that its producer is adequate.
Review producer code with the contract and keep project-native gates authoritative.

When the user requests a full release execution receipt, register all implementation sources and their full-scope producers and run `dotfiles verify check --session SESSION --release` on the candidate checkout.
The check rejects affected-only receipts and missing source coverage.
The full native release campaign still gates publication.
Use `resource-coordination` before an expensive campaign.
A pending resource decision leaves verification incomplete.
