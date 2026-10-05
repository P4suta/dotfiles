# Local shared-skill operations

Status: accepted, with the tracked catalog review superseded by [per-skill decision records](0014-per-skill-decision-records.md).

## Context

Shared instructions need maintained discovery, composition, evaluation, enforcement, and native distribution.
Usage frequency alone shows neither compliance, eligibility, nor whether two skills should merge.
An analysis without a required disposition turns into a permanent list of open recommendations.
The standard must outlive any one coding client or review service.

## Decision

Add a native `skill-ops` command to the Rust xtask, with a managed capability-based skill policy.
Keep canonical instructions and conditional resources in one tree.
Check the aliases, and keep the prerequisite graph explicit and acyclic.
Offer named bundles for common tasks, and keep each skill useful alone.
Use native client adapters to tell explicit loader signals, direct file reads, and weaker shell references apart.
Store hashed operation identities and catalog revisions, and never store prompts, command bodies, raw session identities, or transcripts.

Publish observations as immutable content-addressed records with synchronized files, atomic publication, and Unix directory synchronization.
Keep reports and dispositions in immutable local history, with separate current pointers.
Refuse corrupt state and conflicting replay instead of resetting history.
Bind each runtime disposition to its exact report and the current hashes of its evidence files.
Read the accepted report by its verified immutable identity, not by pairing draft and decision pointers that concurrent sessions update apart.
Capture report identities before scanning local records, so a newer analysis never outruns its input snapshot.
Check newer draft references for missing records, and keep the accepted snapshot subject to current inputs, notes, evidence, and observation thresholds.
Windows tests exercise file synchronization and atomic publication, and power-loss persistence rests on the operating system and filesystem.

Require an evidence-backed disposition for every current catalog and runtime finding.
A disposition implements a change, keeps or rejects a finding with a reason, or defers it with a reconsideration condition.
A record proves structural completeness and freshness, and the deciding session stays responsible for the quality and authorized implementation of its decision.
Offer no blanket approval command, and never merge skills from co-use counts.
Drive maintenance from content changes, improvement notes, and new observation counts, not elapsed time.
Permit one automatic maintenance request per Stop hook, and keep an incomplete outcome when the next check still fails.
Keep the OpenCode observer and the shared completion instruction explicit, because OpenCode has no blocking Stop hook.

Run the catalog review, strict adapter typecheck, native regressions, and production proof gate in the project checks and required CI.
Bind the maintenance engine to its pinned tool and gate configurations.
Keep build and dependency outputs outside chezmoi source state, so native checks can run concurrently.
Six Kani harnesses import the production completion, disposition, composition, maintenance, immutable-identity, and installation-stage decision functions.
They quantify their Boolean and native integer inputs, require reachable outcomes, and check overflow without conclusion-shaped assumptions.
Check the exact harness inventory and its passing properties, and require a false control claim to produce the expected counterexample.
Native integration tests cover SDK payloads with an empty `PATH`, concurrent publication, and parser failures.
They also cover evidence freshness, ordered installation, preservation, and filesystem boundaries.
Run local Kani hooks on Mac and Linux hosts, and keep the required Linux CI proof gate on every pull request.
Check Windows native contracts without reporting the missing verifier as a passing proof.
These proofs leave the Rust standard library, serialization packages, filesystem, client implementation, and human judgment unverified.

Apply files and native executables through each machine's own profile, and never copy credentials, client trust decisions, or usage history.
Merge collector definitions into existing client settings, and require normal native trust and activation checks.
Apply policy before installing the native executable, and activate the OpenCode adapter only after installation succeeds.
Resolve the adapter executable from the native user directory or an explicit absolute path.
Keep third-party and system skills outside the owned catalog, and never attribute their use to owned entries with similar names.
Record the reviewed canonical file inventory in the installed manifest, and check every managed file without deleting unrelated native helpers or caches.

## Consequences

A changed catalog, policy, or maintenance engine invalidates the current catalog review.
A pending runtime improvement fails the completion check until it has a complete current disposition.
Reinstallation keeps observations, notes, reports, and old decisions.
Observation counts miss coverage gaps and implicit client skill injection.
The owner keeps one standard across clients and code-review providers.
New verification capabilities answer real failure classes and reach compatible projects through reviewable changes.
More client integrations and proof obligations extend the canonical skill tree and its evidence without replacing them.
