# Commands and evidence

The dotfiles `install:skill-ops` task installs `skill-ops`, a native Rust executable.
It reads the managed catalog under `~/.agents/skills` and policy under `~/.config/skill-ops`.
The reviewed installation manifest declares the managed files, and unrelated legacy helpers and caches stay local and outside the measured catalog.
Installation applies policy, checks and installs the native runtime, and only then activates the OpenCode adapter.
The adapter resolves `~/.local/bin/skill-ops`, or `skill-ops.exe` on Windows, without relying on GUI `PATH` inheritance.
A custom installation may supply the plugin's absolute `executable` option.
Observations, improvement notes, reports, and runtime dispositions stay under `~/.local/state/skill-ops` on each machine.
Reports and dispositions also keep immutable content-addressed history, and changing a referenced evidence file invalidates its runtime disposition.
Never synchronize or delete that state to clear a pending gate.

```console
skill-ops load verification-tools
skill-ops load --bundle skill-maintenance
skill-ops doctor
skill-ops analyze --out /tmp/skill-analysis.json
skill-ops note --kind mechanize --skill example --summary "The demonstrated invariant needs an executable gate." --evidence path/to/regression
skill-ops triage --decisions /tmp/skill-decisions.json --evidence-root /path/to/project
skill-ops check
just decide skill-operations
mise run check:skills
mise run check:rust
mise run check:adapters
mise run check:proofs
```

Use a fresh report path, and native Windows paths where they apply.
`analyze` publishes an immutable report, updates the local draft view, and lists the findings that still need triage, without applying recommendations.
Completion follows the immutable analysis that the accepted disposition names, so another session's draft never replaces its snapshot.
New notes, changed inputs or evidence, the observation threshold, and disappearing records still invalidate completion.
Each note takes one of the kinds `missing-skill`, `mechanize`, `failure`, `conflict`, and `evaluation`.
Keep a note's summary free of private payloads, and give it concrete local evidence.
The report identifies its catalog, policy, observed events, counts, collection signals, and findings.

A disposition file holds a JSON object with `report_hash` and `decisions` for the listed findings.
Each decision has the exact finding `id`, an `outcome` of `keep`, `implemented`, `rejected`, or `deferred`, a concrete `reason`, existing relative local `evidence` paths, and `revisit` as a condition or `null`.
An `implemented` decision points to the changed contract and its verification evidence.
A `deferred` decision gives a real reconsideration condition and keeps the work pending.
`triage` rejects stale analyses, missing or duplicate findings, incomplete decisions, missing evidence, and evidence escaping its root.
If the underlying evidence changed, collect new observations and reanalyze before triage.

The dotfiles repository tracks one decision per skill in `docs/skills/decisions/<skill>.json`, bound to that skill's content revision.
Engine changes need no decision.
The required check refuses a skill whose decision stays missing, incomplete, or bound to an older revision, and names the command that drafts it.
While a skill's entry point exceeds the policy's `max_entry_words`, its decision also needs a `size` disposition with its own `outcome`, `reason`, and `revisit`.
The check refuses a missing size disposition and one the current limit no longer requires.
`just decide NAME` keeps the previous outcome, evidence, and revisit condition, prints the revision's findings and the content diff since the recorded decision, and leaves the reason empty.
For a decision already bound to the current revision, it only adds or removes the size disposition the limit requires.
Write the reason for the new revision, and change the other fields when the judgment changed.

Installation records the reviewed revisions in the manifest it writes with the engine.
While the installed skill still matches them, the local gate adopts their catalog findings and their size findings, except a deferred size disposition.
The gate never adopts a deferred decision.
The engine refuses a manifest without reviewed revisions, and `mise run install:skill-ops` rewrites it.
The runtime maintenance gate turns pending when content changes, a new demonstrated improvement note arrives, or new observations reach the configured count.
A pending gate analyzes the current evidence and completes when the gate adopts every finding, without `analyze` or a disposition file.
Codex and Claude Stop adapters request maintenance before completion and bound automatic continuation instead of looping forever.
The OpenCode observer supplies the same local data, and the authoritative catalog gate still applies through the project checks.
OpenCode has no blocking Stop hook in this integration, so honor `skill-ops check` from the shared policy for agents before completion.

Verify registration, native client trust, actual activation, and an observed real read as distinct steps.
New hook definitions may need the client's normal trust review, and registration alone proves no activation.
Keep existing hooks, settings, credentials, and approvals intact.
Use current [Codex hooks](https://learn.chatgpt.com/docs/hooks), [Claude hooks](https://code.claude.com/docs/en/hooks), and [OpenCode plugins](https://opencode.ai/docs/plugins/) for adapter contracts.
