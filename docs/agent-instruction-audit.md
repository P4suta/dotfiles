# Agent instruction audit

Every managed global `CLAUDE.md` and `AGENTS.md` renders the partials `.chezmoitemplates/agent_policy`, `.chezmoitemplates/prose_policy`, and `.chezmoitemplates/remote_machines`, and nothing else.
This document classifies every line of those partials.
A retained text line is either judgment that no mechanism can enforce, or a line that stays until the named follow-up mechanizes it.
A heading is classified as `Heading:` and a template directive as `Directive:`, so neither can carry an unaudited instruction.
The machine-local notes that `remote_machines` appends from `~/.config/dotfiles/remote-machines.md` are not tracked and are out of scope.

`just check` runs `xtask/src/instruction_audit.rs`, which refuses:

- a partial line without an entry here, an entry whose line no longer exists, and an entry whose class does not fit its line;
- a follow-up defined more than once, referenced by no retained line, or referenced without a definition;
- a removed line without a valid holder, and a repository path named here that does not exist;
- a root dispatcher (`dot_claude/CLAUDE.md.tmpl`, `dot_codex/AGENTS.md.tmpl`, `dot_config/opencode/AGENTS.md.tmpl`) that does more than select the same file in a native profile;
- a profile template that renders anything besides the partials;
- a `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, or `copilot-instructions.md` anywhere else in the source tree.

## Retained lines

### `agent_policy`

> # Global rules

Heading: names the file.

> These personal preferences apply to every project on this machine.

Judgment: it states the scope a reader applies the rest of the file to; no tool observes which project an agent is serving.

> A project's own instructions take precedence when they conflict.

Judgment: resolving a conflict between instruction sources requires reading both.

> ## Language

Heading: groups the language lines.

> - Reply in the reader's language, which is Japanese for me.

Judgment: chat replies are not persisted, and no hook sees them.

> - Write persisted text, such as commit messages, PRs, issues, code, comments, documentation, and release notes, in English.

Follow-up F1: dotguard's `commit-msg` gate refuses a non-English commit message when `guard.lang` is unset or `english`; the `pre-commit` content scan only filters contamination such as Cyrillic, simplified Chinese, and invisible characters, and accepts Japanese, so committed comments and documentation are not language-checked, and neither are PR and issue documents.

> ## Environment

Heading: groups the tool lines.

> - Use `mise x -- <command>` when a project pins a specific tool version.

Follow-up F2: the Claude Code shell on the Mac and Linux already runs `mise activate`; the remaining agent shells do not.

> - Use `gh` for GitHub operations.

Judgment: the checked paths, `pr-workflow` and the CodeRabbit guard, already call `gh`; choosing a client for an ad hoc read is left to the agent.

> - Follow Conventional Commits.

Follow-up F3: `pr-workflow` checks Conventional Commits PR titles, but no hook checks a commit message.

> ## Workflow

Heading: groups the workflow lines.

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

> - Keep the CodeRabbit CLI and PR review pools separate, apply adaptive thresholds where they apply, and hold a push or PR update when the required review capacity cannot be verified; elapsed time alone does not authorize a push.

Follow-up F8: the CodeRabbit guard enforces the floored budget and refuses unverifiable capacity for a CLI review (`xtask/src/review_guard.rs`); no gate represents the PR review pool or its thresholds, or holds a push on its capacity.

> - Do not declare a PR complete while supported review findings, required checks, or a review of its current head remain outstanding.

Judgment: whether a finding is supported requires assessing it against the code; the state-inspecting next-action command of issue #42 will report the checks and the review of the current head.

> - Commit, push, reply to or resolve PR threads, request reviews, and merge only when I authorize that action.

Judgment: authorization comes from the owner's own messages, which no tool can distinguish from an agent's.

### `prose_policy`

> ## Prose

Heading: groups the prose lines.

> Write prose one sentence per line, in English and in Japanese: break after `。`, `！`, `？`, `.`, `!`, and `?`, and optionally after a clause when that clarifies structure.

Follow-up F7: the global `pre-commit` hook already reflows committed comments and Markdown; commit messages and PR documents remain unchecked.

> Never wrap prose to a fixed column, never merge a paragraph onto one line, and never use a hard line break inside a paragraph; a blank line separates paragraphs.

Follow-up F7: the same gate covers these forms.

> The pre-commit hook applies this to comments and Markdown in committed files; apply it yourself to commit messages, issue and PR bodies, and release notes.

Follow-up F7: the line names the texts the hook cannot see and goes once F7 covers them.

### `remote_machines`

> ## Remote machines

Heading: groups the remote host lines.

> Read the machine-local SSH aliases and topology before operating another host.

Judgment: which host a task needs, and how to reach it, depends on the task.

> Keep hostnames, addresses, credentials, and private checkout locations in machine-local configuration.

Judgment: what is private depends on the destination; this repository's secret scan covers only its own publication snapshot.

> {{- $notes := joinPath .chezmoi.homeDir ".config/dotfiles/remote-machines.md" }}

Directive: names the untracked machine-local notes file and renders nothing.

> {{- if stat $notes }}

Directive: renders the notes only when the file exists.

> {{ include $notes | trim }}

Directive: renders the untracked machine-local notes, which are out of scope.

> {{- end }}

Directive: closes the condition and renders nothing.

## Removed lines

Each removed line names what holds it now:

- `Gate`: a hook or guard refuses the violation; the row names its source file.
- `Skill`: the named skill states the rule once, and the client loads the skill when its description matches the task.
  No gate refuses work done without loading it; F5 adds a completion gate for the implementation assurance bundle only.
- `Gate and skill`: a gate holds part of the line and the named skill states the rest.
- `Merged`: the line was reworded into a retained line.

| Former line | Held by | Mechanism |
| --- | --- | --- |
| Respond to me in Japanese. | Merged | Reworded as the retained reader-language line. |
| Write outward-facing artifacts such as commit messages, PRs, issues, code comments, documentation, and release notes in English. | Merged | Reworded as the retained persisted-text line. |
| Follow Conventional Commits and omit AI co-author footers and session URLs. | Gate | dotguard `commit-msg` strips agent attribution and session URLs (`guard/src/attribution.rs`); the convention is retained as its own line. |
| Keep diffs reviewable by editing the smallest relevant region. | Merged | Merged into the retained smallest-change line. |
| Before GitHub-bound implementation, load `pull-request` and run `pr-workflow start` for the exact destination. | Skill | The `pull-request` skill description states the same trigger; the command is retained. |
| Personal non-fork repositories require an existing issue describing the problem, scope, and acceptance criteria; external repositories and forks follow the destination's contribution rules. | Gate | `pr-workflow start`, `create`, and `edit` refuse a personal non-fork without an open, finished issue (`xtask/src/pr_workflow.rs`), proved in `xtask/src/pr_rules.rs`. |
| Use `pr-workflow` for PR creation, document edits, and ready transitions, and confirm that the diff stays within the issue's scope. | Merged | Reworded as a retained line. |
| Load `formal-assurance` for every implementation change; require source-bound proofs wherever applicable and a checked, justified alternative at genuine semantic or empirical boundaries before declaring it complete. | Skill | The retained bundle line loads `formal-assurance`, which states the obligations. |
| Load `verification-tools` and `type-contracts` for implementation work, and turn enforceable rules into required gates with `executable-policy`. | Skill | The retained bundle line and the requirements in `dot_config/skill-ops/policy.json` load `verification-tools`, `type-contracts`, and `executable-policy`. |
| Load `ci-budget` before an authorized push or PR update; run available CI-equivalent checks locally and honor an active machine-local push hold until explicitly resumed. | Gate and skill | The pre-push dispatcher refuses while `~/.config/git/push-paused` exists (`xtask/src/hooks.rs`); `ci-budget` states the local checks. |
| For an initial substantive PR, load `coderabbit-review` after local checks and use one guarded CLI review when verified allowance permits. | Gate and skill | The CodeRabbit guard admits a CLI review only within verified allowance (`xtask/src/review_guard.rs`); `coderabbit-review` states when one is due. |
| Use `floor(capacity * 0.9 * 0.8)` budgets for verified allowances and applicable adaptive thresholds, keep CLI and PR pools separate, and hold publication when the required capacity cannot be verified; elapsed time alone does not authorize a push. | Gate | The CodeRabbit guard floors the budget once and refuses a CLI review on unverifiable capacity (`xtask/src/review_rules.rs`); the pools, the thresholds, and the publication hold are retained as their own line until F8. |
| Honor an owner-imposed CodeRabbit pause before service calls, quota inquiries, or review requests, and require explicit resumption. | Gate | The CodeRabbit guard refuses every service call while paused (`xtask/src/review_rules.rs`, with a Kani proof). |
| Use `reliability` for time, concurrency, persistence, environment dependencies, and intermittent failures; fix the mechanism rather than relying on repeated green runs. | Skill | `reliability` states the trigger in its description and the rule in its body. |
| For dotfiles changes, load `dotfiles` and apply equivalent behavior through each affected Mac, Linux, and Windows host's own profile. | Skill | `dotfiles` states the trigger and the rule in its description. |
| Prefer `skill-ops load NAME` or a relevant bundle for shared skills; inspect `skill-ops check` before completion and use `skill-operations` for pending maintenance or skill changes. | Gate and skill | The `skill-ops stop` hook refuses completion with pending maintenance (`xtask/src/bin/skill-ops.rs`) and names `skill-operations`; the check is retained for OpenCode. |
| Commit only when I ask, and push only when I tell you to. | Merged | Merged into the retained authorization line. |
| CodeRabbit automatically reviews ready PRs where it is enabled, and supported actionable findings must be addressed as part of the PR workflow. | Skill | Stated once in `coderabbit-review`. |
| When working on an existing PR or an authorized PR update, load `coderabbit-review` and use `gh` to inspect the current head, CI, CodeRabbit review outcome, and unresolved findings. | Skill | `coderabbit-review` states the trigger in its description and the inspection in its body. |
| Wait for the automatic review of the final pushed head and fix supported findings within the authorized task, then rerun the affected checks. | Skill | Stated once in `coderabbit-review`. |
| Explain unsupported findings with concrete evidence instead of applying them blindly or dismissing them without inspection. | Skill | Stated once in `coderabbit-review`. |
| Do not declare the PR complete while supported findings, required checks, or a current-head review remain outstanding. | Merged | Reworded as the retained PR completion line. |
| A passing CodeRabbit check or resolved thread alone does not establish a completed review or verified fix; paused, skipped, stale, failed, and rate-limited reviews remain pending. | Skill | Stated once in `coderabbit-review`. |
| Use the existing PR review before requesting a CLI review of the same diff, and preserve the CLI's local budget. | Gate and skill | `coderabbit-review` states the order, and the guard enforces the CLI budget (`xtask/src/review_guard.rs`). |
| Commit, push, PR replies, thread resolution, manual review triggers, and merge still require the applicable user authorization. | Merged | Merged into the retained authorization line. |
| The former nine-sentence prose rules and their rationale. | Gate | The global pre-commit hook runs `ocomment fix --tidy --staged`, which reflows a touched paragraph of a committed comment or Markdown file (`dot_config/lefthook/global.yml`); the rules are condensed into the retained prose lines. |
| Use the `multi-machine` skill and the current `domyjob` manual for remote execution. | Skill | `multi-machine` and `domyjob` state the trigger in their descriptions. |

## Follow-ups

### F1: Language gate for committed prose and PR and issue documents

dotguard's `pre-commit` gate refuses Japanese in added comment and Markdown prose, and `pr-workflow` validation refuses a title or body that the commit-message scan rejects, both when `guard.lang` is unset or `english` and with the same refusal text as the `commit-msg` gate.

### F2: mise activation in every agent shell

The Claude Code shell on Windows and the Codex and OpenCode shells on every host resolve tools through `mise activate` or mise shims, so a pinned version applies without `mise x`.

### F3: Commit message convention gate

dotguard `commit-msg` refuses a subject that is not a Conventional Commits header and names the `type(scope): description` form, unless the repository sets `git config guard.commits off` because its own instructions state another convention.

### F4: PR authoring through pr-workflow

The managed agent permission profiles deny `gh pr create`, `gh pr edit`, and `gh pr ready`, and the denial names the equivalent `pr-workflow` command.

### F5: Implementation assurance at completion

`skill-ops stop` refuses completion of a session whose observed edits touch implementation files without an observed load of the `implementation-assurance` bundle, and names `skill-ops load --bundle implementation-assurance`.

### F6: OpenCode completion gate

The OpenCode `skill-ops` plugin (`dot_config/opencode/plugins/skill-ops.ts`) runs `skill-ops check` when a session goes idle and reports its refusal to the agent.

### F7: Prose gate for commit messages and PR documents

dotguard `commit-msg` and `pr-workflow` validation run `ocomment check` on the message or body as Markdown and refuse a paragraph that is not one sentence per line.

### F8: CodeRabbit PR review capacity at push

The pre-push dispatcher asks the CodeRabbit guard for the PR review pool's capacity before pushing a branch with an open PR, accounts it separately from the CLI pool with the applicable adaptive threshold, and refuses the push when that capacity cannot be verified or is exhausted, naming the pool and the capacity it found.
