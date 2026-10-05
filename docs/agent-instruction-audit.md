# Agent instruction audit

Every managed global `CLAUDE.md` and `AGENTS.md` renders the partials `.chezmoitemplates/agent_policy`, `.chezmoitemplates/prose_policy`, and `.chezmoitemplates/remote_machines`, and nothing else.
This document classifies every instruction line of those partials.
A retained line is either judgment that no mechanism can enforce, or a line that stays until the named follow-up mechanizes it.
`just check` refuses a partial line without an entry here, an entry whose line no longer exists, and a profile template that renders text outside the partials.
Headings and template directives carry no instruction and are not classified.
The machine-local notes that `remote_machines` appends from `~/.config/dotfiles/remote-machines.md` are not tracked and are out of scope.

## Retained lines

### `agent_policy`

> These personal preferences apply to every project on this machine.

Judgment: it states the scope a reader applies the rest of the file to; no tool observes which project an agent is serving.

> A project's own instructions take precedence when they conflict.

Judgment: resolving a conflict between instruction sources requires reading both.

> - Reply in the reader's language, which is Japanese for me.

Judgment: chat replies are not persisted, and no hook sees them.

> - Write persisted text, such as commit messages, PRs, issues, code, comments, documentation, and release notes, in English.

Follow-up F1: dotguard's `commit-msg` and `pre-commit` gates already refuse foreign scripts in commit messages and committed files; PR and issue documents remain unchecked.

> - Use `mise x -- <command>` when a project pins a specific tool version.

Follow-up F2: the Claude Code shell on the Mac and Linux already runs `mise activate`; the remaining agent shells do not.

> - Use `gh` for GitHub operations.

Judgment: the checked paths, `pr-workflow` and the CodeRabbit guard, already call `gh`; choosing a client for an ad hoc read is left to the agent.

> - Follow the destination's commit message convention, and Conventional Commits when it states none.

Follow-up F3: `pr-workflow` checks Conventional Commits PR titles, but no hook checks a commit message.

> - Make the smallest change that accomplishes the task, and keep its diff reviewable.

Judgment: the size a task needs depends on the task.

> - Verify changes with a project's own lint, typecheck, and test commands when they exist.

Judgment: the hook dispatcher runs a repository's own Lefthook gates at commit and push where the repository configures them; finding the commands elsewhere requires reading the project.

> - Before GitHub-bound implementation, run `pr-workflow start` for the exact destination.

Judgment: whether work will become GitHub-bound is known before any command runs; once run, `pr-workflow start` refuses a personal non-fork repository without a qualifying issue.

> - Use `pr-workflow` for PR creation, document edits, and ready transitions, and keep the diff within the issue's scope.

Follow-up F4: nothing stops a direct `gh pr create`; whether a diff fits an issue's scope stays judgment.

> - For every implementation change, run `skill-ops load --bundle implementation-assurance` and meet the obligations it loads before declaring the change complete.

Follow-up F5: the observer hook records skill loads, but completion does not yet require this bundle.

> - Before declaring work complete, run `skill-ops check` and resolve what it reports.

Follow-up F6: the `skill-ops stop` hook already blocks completion in Claude Code and Codex; OpenCode has no blocking completion hook.

> - Do not declare a PR complete while supported review findings, required checks, or a review of its current head remain outstanding.

Judgment: whether a finding is supported requires assessing it against the code; the state-inspecting next-action command of issue #42 will report the checks and the review of the current head.

> - Commit, push, reply to or resolve PR threads, request reviews, and merge only when I authorize that action.

Judgment: authorization comes from the owner's own messages, which no tool can distinguish from an agent's.

### `prose_policy`

> Write prose one sentence per line, in English and in Japanese: break after `。`, `！`, `？`, `.`, `!`, and `?`, and optionally after a clause when that clarifies structure.

Follow-up F7: the global `pre-commit` hook already reflows committed comments and Markdown; commit messages and PR documents remain unchecked.

> Never wrap prose to a fixed column, never merge a paragraph onto one line, and never use a hard line break inside a paragraph; a blank line separates paragraphs.

Follow-up F7: the same gate covers these forms.

> The pre-commit hook applies this to comments and Markdown in committed files; apply it yourself to commit messages, issue and PR bodies, and release notes.

Follow-up F7: the line names the texts the hook cannot see and goes once F7 covers them.

### `remote_machines`

> Read the machine-local SSH aliases and topology before operating another host.

Judgment: which host a task needs, and how to reach it, depends on the task.

> Keep hostnames, addresses, credentials, and private checkout locations in machine-local configuration.

Judgment: what is private depends on the destination; this repository's secret scan covers only its own publication snapshot.

## Removed lines

| Former line | Mechanism that holds it now |
| --- | --- |
| Respond to me in Japanese. | Reworded as the retained reader-language line. |
| Write outward-facing artifacts such as commit messages, PRs, issues, code comments, documentation, and release notes in English. | Reworded as the retained persisted-text line. |
| Follow Conventional Commits and omit AI co-author footers and session URLs. | dotguard `commit-msg` strips agent attribution and session URLs (`guard/src/attribution.rs`); the convention is retained. |
| Keep diffs reviewable by editing the smallest relevant region. | Merged into the retained smallest-change line. |
| Before GitHub-bound implementation, load `pull-request` and run `pr-workflow start` for the exact destination. | The `pull-request` skill description states the same trigger; the command is retained. |
| Personal non-fork repositories require an existing issue describing the problem, scope, and acceptance criteria; external repositories and forks follow the destination's contribution rules. | `pr-workflow start`, `create`, and `edit` refuse a personal non-fork without an open, finished issue (`xtask/src/pr_workflow.rs`), proved in `xtask/src/pr_rules.rs`. |
| Use `pr-workflow` for PR creation, document edits, and ready transitions, and confirm that the diff stays within the issue's scope. | Reworded as a retained line. |
| Load `formal-assurance` for every implementation change; require source-bound proofs wherever applicable and a checked, justified alternative at genuine semantic or empirical boundaries before declaring it complete. | The `implementation-assurance` bundle loads it; the obligations are stated once in the skill. |
| Load `verification-tools` and `type-contracts` for implementation work, and turn enforceable rules into required gates with `executable-policy`. | The bundle and the requirements in `dot_config/skill-ops/policy.json` load all three. |
| Load `ci-budget` before an authorized push or PR update; run available CI-equivalent checks locally and honor an active machine-local push hold until explicitly resumed. | The pre-push dispatcher refuses while `~/.config/git/push-paused` exists (`xtask/src/hooks.rs`); the trigger is the `ci-budget` skill description. |
| For an initial substantive PR, load `coderabbit-review` after local checks and use one guarded CLI review when verified allowance permits. | The `coderabbit-review` skill states when a CLI review is due, and the CodeRabbit guard admits a review only within verified allowance (`xtask/src/review_guard.rs`). |
| Use `floor(capacity * 0.9 * 0.8)` budgets for verified allowances and applicable adaptive thresholds, keep CLI and PR pools separate, and hold publication when the required capacity cannot be verified; elapsed time alone does not authorize a push. | The CodeRabbit guard enforces the budget and refuses unverifiable capacity (`xtask/src/review_guard.rs`). |
| Honor an owner-imposed CodeRabbit pause before service calls, quota inquiries, or review requests, and require explicit resumption. | The CodeRabbit guard refuses every service call while paused (`xtask/src/review_rules.rs`, with a Kani proof). |
| Use `reliability` for time, concurrency, persistence, environment dependencies, and intermittent failures; fix the mechanism rather than relying on repeated green runs. | The `reliability` skill description states the trigger, and the skill states the rule. |
| For dotfiles changes, load `dotfiles` and apply equivalent behavior through each affected Mac, Linux, and Windows host's own profile. | The `dotfiles` skill description states the trigger and the rule. |
| Prefer `skill-ops load NAME` or a relevant bundle for shared skills; inspect `skill-ops check` before completion and use `skill-operations` for pending maintenance or skill changes. | The observer hook records skill reads however they are loaded, and the `skill-ops stop` refusal names the maintenance steps; the check is retained for OpenCode. |
| Commit only when I ask, and push only when I tell you to. | Merged into the retained authorization line. |
| CodeRabbit automatically reviews ready PRs where it is enabled, and supported actionable findings must be addressed as part of the PR workflow. | Stated once in the `coderabbit-review` skill. |
| When working on an existing PR or an authorized PR update, load `coderabbit-review` and use `gh` to inspect the current head, CI, CodeRabbit review outcome, and unresolved findings. | The `coderabbit-review` skill description states the trigger, and the skill states the inspection. |
| Wait for the automatic review of the final pushed head and fix supported findings within the authorized task, then rerun the affected checks. | Stated once in the `coderabbit-review` skill. |
| Explain unsupported findings with concrete evidence instead of applying them blindly or dismissing them without inspection. | Stated once in the `coderabbit-review` skill. |
| Do not declare the PR complete while supported findings, required checks, or a current-head review remain outstanding. | Reworded as the retained PR completion line. |
| A passing CodeRabbit check or resolved thread alone does not establish a completed review or verified fix; paused, skipped, stale, failed, and rate-limited reviews remain pending. | Stated once in the `coderabbit-review` skill. |
| Use the existing PR review before requesting a CLI review of the same diff, and preserve the CLI's local budget. | Stated once in the `coderabbit-review` skill; the guard enforces the budget. |
| Commit, push, PR replies, thread resolution, manual review triggers, and merge still require the applicable user authorization. | Merged into the retained authorization line. |
| The former nine-sentence prose rules and their rationale. | Condensed into the retained prose lines; the global pre-commit hook runs `ocomment fix --tidy --staged`, which reflows a touched paragraph of a committed comment or Markdown file (`dot_config/lefthook/global.yml`). |
| Use the `multi-machine` skill and the current `domyjob` manual for remote execution. | The `multi-machine` and `domyjob` skill descriptions state the trigger. |

## Follow-ups

### F1: Language gate for PR and issue documents

`pr-workflow` validation refuses a title or body that dotguard's language scan rejects, with the same `guard.lang` setting and refusal text as the commit gates.

### F2: mise activation in every agent shell

The Claude Code shell on Windows and the Codex and OpenCode shells on every host resolve tools through `mise activate` or mise shims, so a pinned version applies without `mise x`.

### F3: Commit message convention gate

dotguard `commit-msg` refuses a subject that is not Conventional Commits when the repository sets `git config guard.commits conventional`, and `pr-workflow start` sets it for a destination that states no other convention.

### F4: PR authoring through pr-workflow

The managed agent permission profiles deny `gh pr create`, `gh pr edit`, and `gh pr ready`, and the denial names the equivalent `pr-workflow` command.

### F5: Implementation assurance at completion

`skill-ops stop` refuses completion of a session whose observed edits touch implementation files without an observed load of the `implementation-assurance` bundle, and names `skill-ops load --bundle implementation-assurance`.

### F6: OpenCode completion gate

The OpenCode `skill-ops` plugin runs `skill-ops check` when a session goes idle and reports its refusal to the agent.

### F7: Prose gate for commit messages and PR documents

dotguard `commit-msg` and `pr-workflow` validation run `ocomment check` on the message or body as Markdown and refuse a paragraph that is not one sentence per line.
