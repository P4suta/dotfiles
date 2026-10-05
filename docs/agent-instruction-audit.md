# Agent instruction audit

Every managed global `CLAUDE.md` and `AGENTS.md` renders the partials `.chezmoitemplates/agent_policy`, `.chezmoitemplates/prose_policy`, and `.chezmoitemplates/remote_machines`, and nothing else.
The OpenCode agent and command prompts under `dot_config/opencode/agents/` and `dot_config/opencode/commands/` add instructions when they run.
This document classifies every line of those partials and prompts.
A retained text line is either judgment that no mechanism can enforce, or a line that stays until the named follow-up mechanizes it.
A heading is classified as `Heading:`, a template action that renders nothing as `Directive:`, and a prompt's front matter line as `Setting:`, so none can carry an unaudited instruction.
No class admits a line whose template action renders output, so a partial cannot include text from elsewhere.

`just check` runs `xtask/src/instruction_audit.rs`, which refuses:

- a partial or prompt line without an entry here, an entry whose line no longer exists, and an entry whose class does not fit its line;
- a line whose template action renders output;
- a follow-up defined more than once, referenced by no retained line, or referenced without a definition;
- a removed line without a valid holder: a `Gate` row must name an existing gate source file, a `Skill` row an existing skill, and a `Merged` row a code span that occurs in exactly one retained line;
- a repository path named outside a quote here that does not exist;
- a root dispatcher (`dot_claude/CLAUDE.md.tmpl`, `dot_codex/AGENTS.md.tmpl`, `dot_config/opencode/AGENTS.md.tmpl`) whose actions do more than select the same file in a native profile;
- a profile template that renders anything besides the partials;
- a file a client loads as instructions anywhere else in the source tree: `CLAUDE.md`, `CLAUDE.local.md`, `AGENTS.md`, `AGENTS.override.md`, `GEMINI.md`, or `copilot-instructions.md`.

Markdown under the Claude Code `rules`, `agents`, and `commands` directories, the Codex `prompts` directory, and the OpenCode `agent`, `agents`, `command`, and `commands` directories is audited line by line like a partial.

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

Follow-up F1: no hook reads a final reply yet.

> - Write persisted text, such as commit messages, PRs, issues, code comments, documentation, and release notes, in English.

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

> - For every implementation change, run `skill-ops load formal-assurance verification-tools` and meet the obligations of the skills it loads before declaring the change complete.

Follow-up F5: `dot_config/skill-ops/policy.json` makes `verification-tools` load `type-contracts` and `executable-policy`, and the observer hook records skill loads, but completion does not yet require them.

> - Before declaring work complete, run `skill-ops check` and resolve what it reports.

Follow-up F6: the `skill-ops stop` hook already blocks completion in Claude Code and Codex; OpenCode has no blocking completion hook.

> - Keep the CodeRabbit CLI and PR review pools separate, apply adaptive thresholds where they apply, and hold a push or PR update when the required review capacity cannot be verified; elapsed time alone does not authorize a push.

Follow-up F7: the CodeRabbit guard enforces the floored budget and refuses unverifiable capacity for a CLI review (`xtask/src/review_guard.rs`); no gate represents the PR review pool or its thresholds, or holds a push on its capacity.

> - Do not declare a PR complete while supported review findings, required checks, or a review of its current head remain outstanding.

Judgment: whether a finding is supported requires assessing it against the code; the state-inspecting next-action command of issue #42 will report the checks and the review of the current head.

> - Commit, push, reply to or resolve PR threads, request reviews, and merge only when I authorize that action.

Judgment: authorization comes from the owner's own messages, which no tool can distinguish from an agent's.

### `prose_policy`

> ## Prose

Heading: groups the prose lines.

> Write prose one sentence per line, in English and in Japanese: break after `。`, `！`, `？`, `.`, `!`, and `?`, and optionally after a clause when that clarifies structure.

Follow-up F1: the global `pre-commit` hook already reflows committed comments and Markdown; commit messages and PR documents remain unchecked.

> Never wrap prose to a fixed column, never merge a paragraph onto one line, and never use a hard line break inside a paragraph; a blank line separates paragraphs.

Follow-up F1: the same checker covers these forms.

> The pre-commit hook applies this to comments and Markdown in committed files; apply it yourself to commit messages, issue and PR bodies, and release notes.

Follow-up F1: the line names the texts the hook cannot see and goes once F1 covers them.

### `remote_machines`

> ## Remote machines

Heading: groups the remote host lines.

> Read the machine-local SSH aliases and topology, and `~/.config/dotfiles/remote-machines.md` when it exists, before operating another host.

Judgment: which host a task needs, and how to reach it, depends on the task; the notes file stays machine-local and is read when needed rather than rendered here.

> Keep hostnames, addresses, credentials, and private checkout locations in machine-local configuration.

Judgment: what is private depends on the destination; this repository's secret scan covers only its own publication snapshot.

### `dot_config/opencode/agents/reviewer.md`

> ---

Setting: delimits the front matter that OpenCode reads as the agent's configuration.

> description: Read-only code reviewer for diffs, branches, and pull requests. Use before committing or merging risky changes.

Setting: tells OpenCode when to delegate to the agent.

> mode: subagent

Setting: makes the agent a subagent.

> permission:

Setting: opens the agent's permission map.

>   edit: deny

Setting: denies the edit tool.

>   bash:

Setting: opens the shell permission map.

>     "*": ask

Setting: asks before any shell command that no entry below allows.

>     git status*: allow

Setting: allows a read-only Git command without asking.

>     git diff*: allow

Setting: allows a read-only Git command without asking.

>     git log*: allow

Setting: allows a read-only Git command without asking.

>     git show*: allow

Setting: allows a read-only Git command without asking.

>     git branch*: allow

Setting: allows a read-only Git command without asking.

>     git merge-base*: allow

Setting: allows a read-only Git command without asking.

> You are a strict but pragmatic code reviewer.

Judgment: how strict a finding should be depends on the change.

> You never modify files.

Judgment: `edit: deny` blocks the edit tool, but a shell command the user approves can still write, and no setting tells such a command from a read.

> Review each change for:

Judgment: introduces the review criteria that follow.

> 1. Correctness — logic errors, unhandled edge cases, race conditions

Judgment: finding these defects requires reading the change.

> 2. Security — injection, path traversal, secret leakage, unsafe deserialization

Judgment: finding these defects requires reading the change.

> 3. Performance — obvious regressions, accidental O(n^2)

Judgment: finding these defects requires reading the change.

> 4. Maintainability — misleading names, swallowed errors, dead code

Judgment: finding these defects requires reading the change.

> 5. Tests — critical paths left untested

Judgment: which paths are critical depends on the change.

> Rules:

Judgment: introduces the review rules that follow.

> - Do not comment on formatting or style; formatters and linters cover it.

Judgment: whether a comment concerns style or behavior requires reading it.

> - For every finding cite `path:line`, explain the impact in one sentence, and suggest a concrete fix.

Judgment: whether an impact and a fix are accurate requires reading the change.

> - Tag severity: [blocker], [warn], or [nit].

Judgment: which severity fits a finding depends on its impact.

> - Never invent issues to seem thorough; if the change is sound, say so.

Judgment: whether a finding is real requires reading the change.

### `dot_config/opencode/commands/commit.md`

> ---

Setting: delimits the front matter that OpenCode reads as the command's configuration.

> description: Commit staged changes with a Conventional Commit message

Setting: describes the command in the command list.

> agent: build

Setting: runs the command with the agent that may edit and commit.

> Create one commit from the currently staged changes.

Judgment: whether the staged changes form one coherent commit requires reading them.

> 1. Inspect the staged changes with `git diff --staged --stat` followed by `git diff --staged`.

Judgment: the message can describe only what the agent has read.

>    If nothing is staged, show `git status` and ask me what to stage — do not stage anything yourself.

Judgment: what belongs in a commit is the owner's decision.

> 2. Write an English Conventional Commit message:

Follow-up F3: dotguard `commit-msg` refuses a non-English message, but no hook checks the Conventional Commits header.

> - `type(scope): summary` — include the scope only when it clarifies

Follow-up F3: the header form is unchecked; whether a scope clarifies stays judgment.

> - imperative mood, all-lowercase summary, no trailing period, max 72 chars

Follow-up F3: the case, the trailing period, and the length are unchecked; the mood stays judgment.

> - add an optional body that explains why, not what

Judgment: what a reader needs to know about the reason depends on the change.

> 3. Commit with `git commit` using a heredoc for the message.

Judgment: the heredoc keeps a multi-line message intact through the shell; no gate sees how a message was passed.

> 4. Show `git log -1 --stat` to confirm.

Judgment: whether the commit holds the intended changes requires reading its contents.

### `dot_config/opencode/commands/pr.md`

> ---

Setting: delimits the front matter that OpenCode reads as the command's configuration.

> description: Open a GitHub pull request for the current branch

Setting: describes the command in the command list.

> agent: build

Setting: runs the command with the agent that may run `pr-workflow`.

> Open a pull request for the current branch.

Judgment: whether the branch is ready for review requires reading it.

> 1. Check `git status` and `git log <base>..HEAD` to understand the branch, where base is the repository's default branch.

Judgment: the document can describe only what the agent has read.

> 2. Load the `pull-request` skill and create the pull request with `pr-workflow create`.

Follow-up F4: nothing stops a direct `gh pr create` yet.

> 3. Print the resulting PR URL.

Judgment: which result the reader needs is judgment; F1 checks only the language and style of the reply.

### `dot_config/opencode/commands/review.md`

> ---

Setting: delimits the front matter that OpenCode reads as the command's configuration.

> description: Review the current branch against its base branch

Setting: describes the command in the command list.

> agent: reviewer

Setting: runs the command with the read-only reviewer agent.

> Review every change on the current branch.

Judgment: reviewing a change requires reading it.

> 1. Determine the base branch: `git symbolic-ref --short refs/remotes/origin/HEAD`, falling back to `main`, then `master`.

Judgment: a branch can target a base other than the default, which the agent notices from the history.

> 2. Diff the whole branch: `git merge-base <base> HEAD`, then `git diff <merge-base>...HEAD`.

Judgment: the review can cover only what the agent has read.

> 3. Apply your review rules: severity-tagged findings with `path:line` and concrete fixes, no style noise.

Judgment: points at the reviewer agent's rules, each of which is judgment.

> 4. Order findings by severity and end with a one-line overall verdict.

Judgment: the verdict weighs the findings.

## Removed lines

Each removed line names what holds it now:

- `Gate`: a hook or guard refuses the violation; the row names its source file.
- `Skill`: the named skill states the rule once, and the client loads the skill when its description matches the task.
  No gate refuses work done without loading it; F5 adds a completion gate for `formal-assurance` and `verification-tools` only.
- `Gate and skill`: a gate holds part of the line and the named skill states the rest.
- `Merged`: the retained line that contains the quoted fragment rewords or restates it.

| Former line | Held by | Mechanism |
| --- | --- | --- |
| Respond to me in Japanese. | Merged | Reworded as `Reply in the reader's language`. |
| Write outward-facing artifacts such as commit messages, PRs, issues, code comments, documentation, and release notes in English. | Merged | Reworded as `Write persisted text`. |
| Follow Conventional Commits and omit AI co-author footers and session URLs. | Gate | dotguard `commit-msg` strips agent attribution and session URLs (`guard/src/attribution.rs`); the convention is retained as its own line. |
| Keep diffs reviewable by editing the smallest relevant region. | Merged | Merged into `Make the smallest change`. |
| Before GitHub-bound implementation, load `pull-request` and run `pr-workflow start` for the exact destination. | Skill | The `pull-request` skill description states the same trigger; the command is retained. |
| Personal non-fork repositories require an existing issue describing the problem, scope, and acceptance criteria; external repositories and forks follow the destination's contribution rules. | Gate and skill | `pr-workflow start`, `create`, and `edit` refuse a personal non-fork without an open, finished issue (`xtask/src/pr_workflow.rs`), proved in `xtask/src/pr_rules.rs`; `pull-request` states that external projects and forks follow their own contribution rules. |
| Use `pr-workflow` for PR creation, document edits, and ready transitions, and confirm that the diff stays within the issue's scope. | Merged | Reworded as `for PR creation, document edits, and ready transitions`. |
| Load `formal-assurance` for every implementation change; require source-bound proofs wherever applicable and a checked, justified alternative at genuine semantic or empirical boundaries before declaring it complete. | Skill | The retained `skill-ops load formal-assurance` line loads `formal-assurance`, which states the obligations. |
| Load `verification-tools` and `type-contracts` for implementation work, and turn enforceable rules into required gates with `executable-policy`. | Skill | The retained `skill-ops load formal-assurance` line loads `verification-tools`, and the requirements in `dot_config/skill-ops/policy.json` load `type-contracts` and `executable-policy` with it. |
| Load `ci-budget` before an authorized push or PR update; run available CI-equivalent checks locally and honor an active machine-local push hold until explicitly resumed. | Gate and skill | The pre-push dispatcher refuses while `~/.config/git/push-paused` exists (`xtask/src/hooks.rs`); `ci-budget` states the local checks. |
| For an initial substantive PR, load `coderabbit-review` after local checks and use one guarded CLI review when verified allowance permits. | Gate and skill | The CodeRabbit guard admits a CLI review only within verified allowance (`xtask/src/review_guard.rs`); `coderabbit-review` states when one is due. |
| Use `floor(capacity * 0.9 * 0.8)` budgets for verified allowances and applicable adaptive thresholds, keep CLI and PR pools separate, and hold publication when the required capacity cannot be verified; elapsed time alone does not authorize a push. | Gate | The CodeRabbit guard floors the budget once and refuses a CLI review on unverifiable capacity (`xtask/src/review_rules.rs`); the pools, the thresholds, and the publication hold are retained as their own line until F7. |
| Honor an owner-imposed CodeRabbit pause before service calls, quota inquiries, or review requests, and require explicit resumption. | Gate | The CodeRabbit guard refuses every service call while paused (`xtask/src/review_rules.rs`, with a Kani proof). |
| Use `reliability` for time, concurrency, persistence, environment dependencies, and intermittent failures; fix the mechanism rather than relying on repeated green runs. | Skill | `reliability` states the trigger in its description and the rule in its body. |
| For dotfiles changes, load `dotfiles` and apply equivalent behavior through each affected Mac, Linux, and Windows host's own profile. | Skill | `dotfiles` states the trigger and the rule in its description. |
| Prefer `skill-ops load NAME` or a relevant bundle for shared skills; inspect `skill-ops check` before completion and use `skill-operations` for pending maintenance or skill changes. | Gate and skill | The `skill-ops stop` hook refuses completion with pending maintenance (`xtask/src/bin/skill-ops.rs`) and names `skill-operations`; the check is retained for OpenCode. |
| Commit only when I ask, and push only when I tell you to. | Merged | Merged into `only when I authorize that action`. |
| CodeRabbit automatically reviews ready PRs where it is enabled, and supported actionable findings must be addressed as part of the PR workflow. | Skill | Stated once in `coderabbit-review`. |
| When working on an existing PR or an authorized PR update, load `coderabbit-review` and use `gh` to inspect the current head, CI, CodeRabbit review outcome, and unresolved findings. | Skill | `coderabbit-review` states the trigger in its description and the inspection in its body. |
| Wait for the automatic review of the final pushed head and fix supported findings within the authorized task, then rerun the affected checks. | Skill | Stated once in `coderabbit-review`. |
| Explain unsupported findings with concrete evidence instead of applying them blindly or dismissing them without inspection. | Skill | Stated once in `coderabbit-review`. |
| Do not declare the PR complete while supported findings, required checks, or a current-head review remain outstanding. | Merged | Reworded as `Do not declare a PR complete`. |
| A passing CodeRabbit check or resolved thread alone does not establish a completed review or verified fix; paused, skipped, stale, failed, and rate-limited reviews remain pending. | Skill | Stated once in `coderabbit-review`. |
| Use the existing PR review before requesting a CLI review of the same diff, and preserve the CLI's local budget. | Gate and skill | `coderabbit-review` states the order, and the guard enforces the CLI budget (`xtask/src/review_guard.rs`). |
| Commit, push, PR replies, thread resolution, manual review triggers, and merge still require the applicable user authorization. | Merged | Merged into `only when I authorize that action`. |
| The former nine-sentence prose rules and their rationale. | Gate | The global pre-commit hook runs `ocomment fix --tidy --staged`, which reflows a touched paragraph of a committed comment or Markdown file (`dot_config/lefthook/global.yml`); the rules are condensed into the retained prose lines. |
| Use the `multi-machine` skill and the current `domyjob` manual for remote execution. | Skill | `multi-machine` and `domyjob` state the trigger in their descriptions. |
| The template actions in `remote_machines` that rendered the untracked machine-local notes file into every instruction file. | Merged | The retained line names the file to read instead: `when it exists, before operating another host`. |
| `commit.md`: wrap an optional body at 100 chars explaining why, not what | Merged | It contradicted `Never wrap prose to a fixed column`; the reason requirement stays in the command. |
| `commit.md`: Never append attribution footers. | Gate | dotguard `commit-msg` strips agent attribution (`guard/src/attribution.rs`). |
| `commit.md`: Reply to me in Japanese. | Merged | Restated by `Reply in the reader's language`. |
| `pr.md`: Open a pull request for the current branch using `gh`. | Merged | Superseded by `for PR creation, document edits, and ready transitions`. |
| `pr.md`: Title: English, max 72 chars, imperative mood; keep the repo's existing title style if one is apparent. | Gate and skill | `pr-workflow` checks the title syntax (`xtask/src/pr_workflow.rs`); `pull-request` states the title rules and the precedence of the destination's own style. |
| `pr.md`: Body in English markdown with `## Summary`, `## Changes`, and `## Notes` sections for testing, caveats, and follow-ups. | Skill | `pull-request` states the body sections and the precedence of the destination's template. |
| `pr.md`: Run `gh pr create` with the base, title, and body, using a heredoc for the body. | Merged | Replaced by `create the pull request with`. |
| `pr.md`: Reply to me in Japanese; the PR title and body stay English. | Merged | Restated by `Reply in the reader's language`; the English requirement is the retained persisted-text line. |
| `reviewer.md`: Write your review in Japanese. | Merged | Restated by `Reply in the reader's language`. |

## Follow-ups

F1 lands with issue #44.
The others have no scoped issue yet; each gets its own issue under #42 before it is implemented.

### F1: Writing standard checker

Issue #44 adds one checker and calls it from the `pre-commit` and `commit-msg` gates, `pr-workflow check`, `create`, and `edit`, OComment, and the Claude Code `Stop` hook with the equivalent Codex and OpenCode hooks.
It refuses persisted text that is not English or not one sentence per line, and it checks a final reply against the reader-language rule.

### F2: mise activation in every agent shell

The Claude Code shell on Windows and the Codex and OpenCode shells on every host resolve tools through `mise activate` or mise shims, so a pinned version applies without `mise x`.

### F3: Commit message convention gate

dotguard `commit-msg` refuses a subject that is not a Conventional Commits header, a description that starts with an uppercase letter or ends with a period, and a header longer than 72 characters, and names the `type(scope): description` form, unless the repository sets `git config guard.commits off` because its own instructions state another convention.

### F4: PR authoring through pr-workflow

The managed agent permission profiles deny `gh pr create`, `gh pr edit`, and `gh pr ready`, and the denial names the equivalent `pr-workflow` command.

### F5: Implementation assurance at completion

`skill-ops stop` refuses completion of a session whose observed edits touch implementation files without observed loads of `formal-assurance` and `verification-tools`, and names `skill-ops load formal-assurance verification-tools`.

### F6: OpenCode completion gate

The OpenCode `skill-ops` plugin (`dot_config/opencode/plugins/skill-ops.ts`) runs `skill-ops check` when a session goes idle and reports its refusal to the agent.

### F7: CodeRabbit PR review capacity at push

The pre-push dispatcher asks the CodeRabbit guard for the PR review pool's capacity before pushing a branch with an open PR, accounts it separately from the CLI pool with the applicable adaptive threshold, and refuses the push when that capacity cannot be verified or is exhausted, naming the pool and the capacity it found.
