# Commands and Evidence

`skill-ops` is a native Rust executable installed through the dotfiles `install:skill-ops` task.
It reads the managed catalog under `~/.agents/skills` and policy under `~/.config/skill-ops`.
The reviewed installation manifest declares the managed files; unrelated legacy helpers and caches remain local and outside the measured catalog.
Installation applies policy, validates and installs the native runtime, and only then activates the OpenCode adapter.
The adapter resolves `~/.local/bin/skill-ops` or `skill-ops.exe` on Windows without relying on GUI PATH inheritance; a custom installation may supply the plugin's absolute `executable` option.
Observations, improvement notes, reports, and runtime dispositions stay under `~/.local/state/skill-ops` on each machine.
Reports and dispositions also have immutable content-addressed history; changing a referenced evidence file invalidates its runtime disposition.
Do not synchronize or delete that state to clear a pending gate.

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

Use a fresh report path and native Windows paths when applicable.
`analyze` publishes an immutable report, updates the local draft view, and lists the findings that still need triage; it does not apply recommendations.
Completion follows the immutable analysis named by the accepted disposition, so another session's draft cannot replace its snapshot.
New notes, changed inputs or evidence, the observation threshold, and disappearing records still invalidate completion.
The note kinds are `missing-skill`, `mechanize`, `failure`, `conflict`, and `evaluation`.
Keep a note's summary free of private payloads and give it concrete local evidence.
The report identifies its catalog, policy, observed events, counts, collection signals, and findings.

A disposition file is a JSON object with `report_hash` and `decisions` for the listed findings.
Each decision has the exact finding `id`, an `outcome` of `keep`, `implemented`, `rejected`, or `deferred`, a concrete `reason`, existing relative local `evidence` paths, and `revisit` as a condition or `null`.
For `implemented`, point to the actual changed contract and its verification evidence.
For `deferred`, provide a real reconsideration condition and retain the work as pending rather than claiming implementation.
`triage` rejects stale analyses, missing or duplicate findings, incomplete decisions, missing evidence, and evidence escaping its root.
Collect new observations and reanalyze before triage if the underlying evidence changed.

The dotfiles repository tracks one decision per skill in `docs/skills/decisions/<skill>.json`, bound to that skill's content revision; engine changes need none.
The required check refuses a skill whose decision is missing, incomplete, or bound to an older revision, and names the command that drafts it.
While a skill's entrypoint exceeds the policy's `max_entry_words`, its decision also needs a `size` disposition with its own `outcome`, `reason`, and `revisit`; the check refuses a missing one, and one that the current limit no longer requires.
`just decide NAME` keeps the previous outcome, evidence, and revisit condition, prints the revision's findings and the content diff since the recorded decision, and leaves the reason empty.
For a decision already bound to the current revision, it only adds or removes the size disposition that the limit requires.
Write the reason for the new revision, and change the other fields when the judgment changed.

Installation records the reviewed revisions in the manifest it writes with the engine.
The local gate adopts their catalog findings, and their size findings unless the size disposition is deferred, while the installed skill still matches them; a deferred decision is not adopted.
The engine refuses a manifest without reviewed revisions; run `mise run install:skill-ops` to rewrite it.
The runtime maintenance gate becomes pending when content changes, a demonstrated improvement note is added, or the configured number of new observations is reached.
A pending gate analyzes the current evidence and completes when every finding is adopted, without `analyze` or a disposition file.
Codex and Claude Stop adapters request maintenance before completion and bound automatic continuation rather than looping indefinitely.
The OpenCode observer supplies the same local data; the authoritative catalog gate still applies through the project checks.
OpenCode does not have an equivalent blocking Stop hook in this integration; honor `skill-ops check` from the shared agent policy before completion.

Verify registration, native client trust, actual activation, and an observed real read separately.
New hook definitions may require the client's normal trust review; registration alone does not prove activation.
Keep existing hooks, settings, credentials, and approvals intact.
Use current [Codex hooks](https://learn.chatgpt.com/docs/hooks), [Claude hooks](https://code.claude.com/docs/en/hooks), and [OpenCode plugins](https://opencode.ai/docs/plugins/) for adapter contracts.
