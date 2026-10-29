# Agent instruction audit

Every managed global `CLAUDE.md` and `AGENTS.md` renders the partials `home/.chezmoitemplates/partials/agent_policy`, `home/.chezmoitemplates/partials/prose_policy`, and `home/.chezmoitemplates/partials/remote_machines`, and nothing else.
The OpenCode prompts under `home/dot_config/opencode/agents/` and `home/dot_config/opencode/commands/` add instructions when they run.
This document classifies every line of those shared templates and prompts.
A retained text line holds judgment no mechanism can enforce, or it stays until the named follow-up mechanizes it.
Headings, template actions that render nothing, and front matter lines take the classes `Heading:`, `Directive:`, and `Setting:`.
No class admits a line whose template action renders output.

`just check` runs `xtask/src/instruction_audit.rs`, which refuses:

- a partial or prompt line without an entry here
- an entry whose line no longer exists
- an entry whose class doesn't fit its line
- a line whose template action renders output
- a follow-up defined twice, referenced by no retained line, or referenced without a definition
- a removed line without a valid holder
- a repository path named outside a quote that doesn't exist
- a root dispatcher that does more than select the same file in a native profile
- a profile template that renders more than the shared templates
- a file a client loads as instructions anywhere else in the source tree, such as `CLAUDE.md` or `AGENTS.md`

A `Gate` row names an existing gate source file, and a `Skill` row names an existing skill.
A `Merged` row names a code span found in exactly one retained line.
A `Contradicted` row names an existing skill or such a code span.

The check audits Markdown in the Claude Code `rules`, `agents`, and `commands` directories like a partial.
The Codex `prompts` directory and the OpenCode `agent`, `agents`, `command`, and `commands` directories get the same audit.

## Retained lines

### `agent_policy`

> # Global rules

Heading: names the file.

> These personal preferences apply to every project on this machine.

Judgment: no tool observes which project an agent serves.

> A project's own instructions take precedence when they conflict.

Judgment: resolving a conflict between instruction sources requires reading both.

> ## Language

Heading: groups the language lines.

> {{ $language := includeTemplate "partials/reply_language" . -}}

Directive: read the owner's configured reply language.

> {{ if eq $language "Japanese" -}}

Directive: select the Japanese line from the owner's configured reply language.

> - Reply in Japanese.

Judgment: the owner chooses a reply language.
The Stop hook enforces that choice in Claude Code and Codex.

> {{ else -}}

Directive: select the English line for the other supported language.

> - Reply in English.

Judgment: the owner can choose English through the same data value.

> {{ end -}}

Directive: close the reply language selection.

> - Write outward-facing artifacts such as commit messages, PRs, issues, code comments, documentation, and release notes in English.

Follow-up F1: dotguard's `commit-msg` gate refuses a non-English commit message when `guard.lang` has no value or holds `english`.
The `pre-commit` content scan filters only contamination such as Cyrillic, simplified Chinese, and invisible characters, and it accepts Japanese.
Committed comments, documentation, PR documents, and issue documents get no language check.

> ## Environment

Heading: groups the tool lines.

> - Use `mise x -- <command>` when a project pins a specific tool version.

Follow-up F2: the Claude Code shell on the Mac and Linux already runs `mise activate`, and the remaining agent shells don't.

> - Use `gh` for GitHub operations.

Judgment: the checked paths, `pr-workflow` and the CodeRabbit guard, already call `gh`, and the agent picks a client for an ad hoc read.

> - Follow Conventional Commits.

Follow-up F3: `pr-workflow` checks Conventional Commits PR titles, but no hook checks a commit message.

> ## Workflow

Heading: groups the workflow lines.

> - Make the smallest change that accomplishes the task, and keep its diff reviewable.

Judgment: the size a task needs depends on the task.

> - Verify changes with a project's own lint, typecheck, and test commands when they exist.

Judgment: the hook dispatcher runs a repository's own Lefthook gates at commit and push where the repository configures them.
Finding the commands elsewhere requires reading the project.

> - Before GitHub-bound implementation, run `pr-workflow start` for the exact destination.

Judgment: the agent knows before any command runs whether work reaches GitHub.
Once run, `pr-workflow start` refuses a personal non-fork repository without a qualifying issue.

> - Use `pr-workflow` for PR creation, document edits, and ready transitions, and keep the diff within the issue's scope.

Follow-up F4: nothing stops a direct `gh pr create`.
Whether a diff fits an issue's scope stays judgment.

> - In personal repositories, use stable descriptive document paths and name new ADRs after the decision with a front-matter date, as the shared `adr` skill describes.

Follow-up F8: the shared `adr` skill states the convention, and `xtask/src/adr.rs` checks the dotfiles collection.
Other personal repositories have no shared ADR gate.
Choosing a descriptive name still requires reading the decision.

>   Preserve existing filenames and references, and regenerate shared indexes from source records when the repository has a generator.

Follow-up F8: the dotfiles index has a generator, but no shared gate checks index drift and historical reference preservation across personal repositories.

> - Use decision-naming before labeling a plan, design, diagram, branch, API, schema, or file, so names carry the decision or property instead of v1, v2, new, old, final, or next.

Judgment: choosing a descriptive name and naming files and directories have no check.
The prose checker refuses bare version labels in English documents through `Dotfiles.VersionLabel`.
The assurance catalog refuses versioned identifier segments in Rust and TypeScript through `version-label`.
The Stop hook and `dotfiles answer check` refuse a Han character outside the Japanese Industrial Standards X 0208 and X 0213 repertoire in Japanese replies through `dotguard::lang` and the Unihan-derived table in `crates/dotguard/src/jis_kanji_table.rs`.
The word rules in `japanese.toml` wait for a reply checker that reads them.

> - Use monochrome-design for every visual design task, including interfaces, websites, diagrams, and documents.

Judgment: whether an artifact is a visual design task, and whether its hierarchy reads as intended, stays a design review.
The `monochrome-design` skill states the defaults, and a project's browser checks with positive and negative fixtures cover the computed styles.

> - For implementation changes, read the relevant formal-assurance and verification-tools skill instructions directly and meet their verification obligations.

Judgment: selecting relevant skill instructions and checking their verification obligations requires a review of the implementation and its evidence.

> - Keep experimental skill maintenance off by default, and don't run or require its loading, observation, analysis, triage, or checks without this machine's explicit opt-in.

Judgment: the owner controls each machine's opt-in, and verification duties still apply without skill maintenance.

> - Keep the CodeRabbit CLI and PR review pools separate, apply adaptive thresholds where they apply, and hold a push or PR update while review capacity stays unverified.

Follow-up F7: the CodeRabbit guard enforces the floored budget and refuses unverifiable capacity for a CLI review in `crates/coderabbit-guard/src/lib.rs`.
No gate represents the PR review pool or its thresholds, or holds a push on its capacity.

> - Never declare a PR complete while supported review findings, required checks, or a review of its current head remain outstanding.

Judgment: whether a finding has support requires checking the code.

> - Commit, push, reply to or resolve PR threads, request reviews, and merge only when the owner asks.

Judgment: authorization comes from the owner's own messages, and no tool separates them from an agent's.

> - Use japanese-lexicon to verify Japanese readings and romanizations with identified public lexical records or major dictionaries and a named spelling standard.

Judgment: selecting the contextual sense and governing spelling standard needs lexical review.
The answer checker retrieves the declared source and checks supported conversions in `crates/dotfiles/src/answer/lexicon.rs`.

> - Use checked-arguments for substantive reasoning, comparisons, explanations, and recommendations, including the checks for scope, semantic roles, evidence, counterarguments, and suitable formal methods.

Judgment: interpreting natural language and selecting a formal model need argument review.
The answer checker checks declared dependencies, Boolean entailment, comparison axes, and parallel roles in `crates/dotfiles/src/answer.rs`.

> - Before completing a request, check the exact answer draft with `dotfiles answer check` using the request's managed session context.

Judgment: client trust and reload determine whether managed hooks run.
The managed Stop path in `crates/dotfiles/src/answer.rs` refuses an exact draft without completed evidence for its current request.

> - Keep premise truth, natural-language interpretation, formal entailment, and decision relevance separate, and report unsupported or incomplete checks without claiming verification.

Judgment: a formal theorem doesn't establish its empirical premises or the correspondence between its model and prose.
The answer checker distinguishes hypotheses and unverified statements from factual premises and complete proof evidence.

### `prose_policy`

> ## Prose

Heading: groups the prose lines.

> Write prose one sentence per line in English and Japanese: break after `。`, `！`, `？`, `.`, `!`, and `?`.

Follow-up F1: `prose check` and `dotfiles assure stop --reply-language` share the sentence layout decisions in `crates/rules/src/prose_rules.rs`.

> Never wrap a sentence across lines or put two sentences on one line.

Follow-up F1: the same checker covers this form.

> Split long sentences into shorter ones when a channel limits line length.

Follow-up F1: the wrapped sentence refusal names this correction.

> Never merge a paragraph onto one line.

Follow-up F1: the same checker covers this form.

> Never use a hard line break inside a paragraph.

Follow-up F1: the same checker covers this form.

> A blank line separates paragraphs.

Follow-up F1: the same checker covers this form.

> The pre-commit hook applies this to comments and Markdown in committed files.

Follow-up F1: the line names the texts the hook covers and goes once F1 covers the rest.

> The Stop hook applies the same rule to replies with `--reply-language`.

Follow-up F1: `crates/dotfiles/src/assure.rs` checks reply layout before the language decision.

> Apply it yourself to commit messages, issue and PR bodies, release notes, and every agent reply.

Follow-up F1: commit, publication, comment, and document checks share the reply layout rule.

### `remote_machines`

> ## Remote machines

Heading: groups the remote host lines.

> Read the machine-local SSH aliases and topology, and `~/.config/dotfiles/remote-machines.md` when it exists, before operating another host.

Judgment: which host a task needs, and how to reach it, depends on the task.
The notes file stays machine-local, and the agent reads it when needed instead of the template rendering it.

> Keep hostnames, addresses, credentials, and private checkout locations in machine-local configuration.

Judgment: what counts as private depends on the destination.
This repository's secret scan covers only its own publication snapshot.

### `home/dot_config/opencode/agents/reviewer.md`

> ---

Setting: delimits the front matter that OpenCode reads as the agent's configuration.

> description: >-

Setting: opens the description of the agent.

>   Read-only code reviewer for diffs, branches, and pull requests.

Setting: names the agent's job.

>   Use before committing or merging risky changes.

Setting: states when OpenCode delegates to the agent.

> mode: subagent

Setting: makes the agent a subagent.

> permission:

Setting: opens the agent's permission map.

>   edit: deny

Setting: denies the edit tool.

>   bash:

Setting: opens the shell permission map.

>     "*": ask

Setting: asks before any shell command that no entry below permits.

>     git status*: allow

Setting: permits a read-only Git command without asking.

>     git diff*: allow

Setting: permits a read-only Git command without asking.

>     git log*: allow

Setting: permits a read-only Git command without asking.

>     git show*: allow

Setting: permits a read-only Git command without asking.

>     git branch*: allow

Setting: permits a read-only Git command without asking.

>     git merge-base*: allow

Setting: permits a read-only Git command without asking.

> Review changes with rigor and pragmatism.

Judgment: the strictness a finding needs depends on the change.

> Never edit files.

Judgment: `edit: deny` blocks the edit tool, but a shell command the user approves can still write, and no setting separates such a command from a read.

> Check each change for:

Judgment: introduces the review criteria that follow.

> 1. Correctness: logic errors, unhandled edge cases, race conditions

Judgment: finding these defects requires reading the change.

> 2. Security: injection, path traversal, secret leakage, unsafe deserialization

Judgment: finding these defects requires reading the change.

> 3. Performance: obvious regressions

Judgment: finding these defects requires reading the change.

> 4. Maintainability: misleading names, swallowed errors, dead code

Judgment: finding these defects requires reading the change.

> 5. Tests: untested critical paths

Judgment: which paths count as critical depends on the change.

> Rules:

Judgment: introduces the review rules that follow.

> - Skip formatting and style, which formatters and linters cover.

Judgment: whether a comment concerns style or behavior requires reading it.

> - Cite `path:line` for every finding, state its impact in one sentence, and suggest a concrete fix.

Judgment: whether an impact and a fix hold up requires reading the change.

> - Tag severity: [blocker], [warn], or [nit].

Judgment: which severity fits a finding depends on its impact.

> - Never invent issues.

Judgment: whether a finding holds requires reading the change.

> Say so when the change holds up.

Judgment: whether the change holds up requires reading it.

### `home/dot_config/opencode/commands/commit.md`

> ---

Setting: delimits the front matter that OpenCode reads as the command's configuration.

> description: >-

Setting: opens the description of the command.

>   Commit staged changes with a Conventional Commit message

Setting: describes the command in the command list.

> agent: build

Setting: runs the command with the agent that may edit and commit.

> Create one commit from the staged changes.

Judgment: whether the staged changes form one coherent commit requires reading them.

> 1. Inspect them with `git diff --staged --stat`, then `git diff --staged`.

Judgment: the message can describe only what the agent has read.

>    With nothing staged, show `git status` and ask what to stage.

Judgment: what belongs in a commit stays the owner's decision.

>    Never stage anything yourself.

Judgment: what belongs in a commit stays the owner's decision.

> 2. Write an English Conventional Commit message:

Follow-up F3: dotguard `commit-msg` refuses a non-English message, but no hook checks the Conventional Commits header.

> - `type(scope): summary`, with the scope only when it clarifies

Follow-up F3: the header form stays unchecked, and whether a scope clarifies stays judgment.

> - imperative mood, all-lowercase summary, no trailing period, at most 72 characters

Follow-up F3: the case, the trailing period, and the length stay unchecked, and the mood stays judgment.

> - an optional body explaining why, not what

Judgment: what a reader needs to know about the reason depends on the change.

> 3. Commit with `git commit`, passing the message through a heredoc.

Judgment: the heredoc keeps a multi-line message intact through the shell, and no gate checks how a message arrived.

> 4. Show `git log -1 --stat` to confirm.

Judgment: whether the commit holds the intended changes requires reading its contents.

### `home/dot_config/opencode/commands/pr.md`

> ---

Setting: delimits the front matter that OpenCode reads as the command's configuration.

> description: >-

Setting: opens the description of the command.

>   Open a GitHub pull request for the current branch

Setting: describes the command in the command list.

> agent: build

Setting: runs the command with the agent that may run `pr-workflow`.

> Open a PR for the current branch.

Judgment: whether the branch stands ready for review requires reading it.

> 1. Read `git status` and `git log <base>..HEAD`, where base names the repository's default branch.

Judgment: the document can describe only what the agent has read.

> 2. Load the `pull-request` skill and create the PR with `pr-workflow create`.

Follow-up F4: nothing stops a direct `gh pr create` yet.

> 3. Print the URL of the new PR.

Judgment: which result the reader needs depends on the task.

### `home/dot_config/opencode/commands/review.md`

> ---

Setting: delimits the front matter that OpenCode reads as the command's configuration.

> description: >-

Setting: opens the description of the command.

>   Review the current branch's diff from its base branch

Setting: describes the command in the command list.

> agent: reviewer

Setting: runs the command with the read-only reviewer agent.

> Review every change on the current branch.

Judgment: reviewing a change requires reading it.

> 1. Find the base branch with `git symbolic-ref --short refs/remotes/origin/HEAD`, falling back to `main`, then `master`.

Judgment: a branch can target a base other than the default, which the agent notices from the history.

> 2. Diff the whole branch: `git merge-base <base> HEAD`, then `git diff <merge-base>...HEAD`.

Judgment: the review covers only what the agent has read.

> 3. Apply your review rules: severity-tagged findings with `path:line` and concrete fixes, no style noise.

Judgment: points at the reviewer agent's rules, each of which needs judgment.

> 4. Order findings by severity, and end with a one-line verdict.

Judgment: the verdict weighs the findings.

## Removed lines

Each removed line names what holds it now:

- `Gate`: a hook or guard refuses the violation, and the row names its source file.
- `Skill`: the named skill states the rule once, and the client loads it when its description matches the task.
- `Gate and skill`: a gate holds part of the line, and the named skill states the rest.
- `Merged`: the retained line containing the quoted fragment rewords or restates it.
- `Contradicted`: the named retained line or skill states the rule that applies instead.

Formal-assurance and verification-tools obligations still require reading their relevant instructions and checking implementation evidence.
Experimental skill maintenance stays off until the machine's explicit opt-in.

| Former line | Held by | Mechanism |
| --- | --- | --- |
| The Japanese reply line. | Merged | Reworded as `Reply in Japanese`. |
| Follow Conventional Commits and omit AI attribution footers and session URLs. | Gate | Dotguard `commit-msg` strips agent attribution and session URLs in `crates/dotguard/src/attribution.rs` and refuses provenance phrases in `crates/rules/src/provenance_rules.rs`. |
| Keep diffs reviewable by editing the smallest relevant region. | Merged | Merged into `Make the smallest change`. |
| Load `pull-request` before GitHub-bound implementation. | Skill | The `pull-request` skill description states the same trigger. |
| Personal non-fork repositories require an existing issue. | Gate and skill | `pr-workflow start`, `create`, and `edit` refuse a personal non-fork without an open, finished issue in `crates/pr-workflow/src/lib.rs`, and `pull-request` states the rule for external projects and forks. |
| Confirm that the diff stays within the issue's scope. | Merged | Reworded as `for PR creation, document edits, and ready transitions`. |
| Load `formal-assurance` for every implementation change. | Skill | The retained `skill-ops load formal-assurance` line loads `formal-assurance`, which states the obligations. |
| Load `verification-tools` and `type-contracts` for implementation work. | Skill | The retained `skill-ops load formal-assurance` line loads `verification-tools`, and `home/dot_config/skill-ops/policy.json` adds `type-contracts` and `executable-policy`. |
| Load `ci-budget` before an authorized push or PR update. | Gate and skill | The pre-push dispatcher refuses while `~/.config/git/push-paused` exists in `crates/dotfiles/src/hooks.rs`, and `ci-budget` states the local checks. |
| Load `coderabbit-review` for an initial substantive PR. | Gate and skill | The CodeRabbit guard admits a CLI review only within verified allowance in `crates/coderabbit-guard/src/lib.rs`, and `coderabbit-review` states the trigger. |
| Use `floor(capacity * 0.9 * 0.8)` budgets and hold publication on unverified capacity. | Gate | The CodeRabbit guard floors the budget once and refuses unverifiable CLI capacity in `crates/rules/src/review_rules.rs`. |
| Elapsed time alone gives no permission to push. | Merged | Merged into `while review capacity stays unverified`. |
| Honor an owner-imposed CodeRabbit pause. | Gate | The CodeRabbit guard refuses every service call while paused in `crates/rules/src/review_rules.rs`, with a Kani proof. |
| Use `reliability` for intermittent failures. | Skill | `reliability` states the trigger in its description and the rule in its body. |
| Load `dotfiles` for dotfiles changes. | Skill | `dotfiles` states the trigger and the rule in its description. |
| Prefer `skill-ops load NAME` for shared skills. | Gate and skill | The `skill-ops stop` hook refuses completion with pending maintenance in `crates/skill-ops/src/main.rs` and names `skill-operations`. |
| Commit and push only on request. | Merged | Merged into `only when the owner asks`. |
| CodeRabbit automatically reviews ready PRs. | Skill | `coderabbit-review` states it once. |
| Load `coderabbit-review` on an existing PR or an authorized PR update. | Skill | `coderabbit-review` states the trigger in its description and the inspection in its body. |
| Wait for the automatic review of the final pushed head. | Skill | `coderabbit-review` states it once. |
| Explain unsupported findings with concrete evidence. | Skill | `coderabbit-review` states it once. |
| The PR completion line, before the current wording. | Merged | Reworded as `Never declare a PR complete`. |
| A passing CodeRabbit check alone doesn't establish a completed review. | Skill | `coderabbit-review` states it once. |
| Use the existing PR review before requesting a CLI review of the same diff. | Gate and skill | `coderabbit-review` states the order, and the guard enforces the CLI budget in `crates/coderabbit-guard/src/lib.rs`. |
| Commit, push, PR replies, thread resolution, manual review triggers, and merge still require authorization. | Merged | Merged into `only when the owner asks`. |
| The former nine-sentence prose rules and their rationale. | Gate | The global pre-commit hook runs `ocomment fix --tidy --staged`, which reflows a touched paragraph in `home/dot_config/lefthook/global.yml`. |
| Use the `multi-machine` skill for remote execution. | Skill | `multi-machine` and `domyjob` state the trigger in their descriptions. |
| The template actions that rendered the machine-local notes file into every instruction file. | Merged | The retained line names the file to read instead: `when it exists, before operating another host`. |
| In `commit.md`, wrap an optional body at 100 characters. | Contradicted | It contradicted `Never wrap a sentence across lines`, and the command keeps the reason line. |
| In `commit.md`, never append attribution footers. | Gate | Dotguard `commit-msg` strips agent attribution in `crates/dotguard/src/attribution.rs`. |
| In `commit.md`, the Japanese reply line. | Merged | Restated by `Reply in Japanese`. |
| In `pr.md`, open a pull request using `gh`. | Merged | Superseded by `for PR creation, document edits, and ready transitions`. |
| In `pr.md`, the English title of at most 72 characters in imperative mood. | Contradicted | `pull-request` requires an English Conventional Commits title unless the destination has its own rules, and `pr-workflow` checks the syntax in `crates/pr-workflow/src/lib.rs`. |
| In `pr.md`, a body with `## Summary`, `## Changes`, and `## Notes` sections. | Contradicted | `pull-request` prescribes `Why`, `Changes`, and `Validation` when the destination has no template. |
| In `pr.md`, run `gh pr create` with the base, title, and body. | Merged | Replaced by `create the PR with`. |
| In `pr.md`, the Japanese reply line. | Merged | Restated by `Reply in Japanese`. |
| In `reviewer.md`, the Japanese review line. | Merged | Restated by `Reply in Japanese`. |

## Follow-ups

F1 lands with issue #44.
F8 continues the shared ADR work of issue #110.
The others have no scoped issue yet, and each gets its own issue under #42 before implementation.

### F1:

Issue #44 adds one checker.
The `pre-commit` and `commit-msg` gates, `pr-workflow check`, `create`, and `edit`, OComment, and the Claude Code `Stop` hook call it.
Codex and OpenCode hooks call it too.
It refuses outward-facing text in another language or with more than one sentence on a line.

### F2:

The Claude Code shell on Windows and the Codex and OpenCode shells on every host resolve tools through `mise activate` or mise shims, so a pinned version applies without `mise x`.

### F3:

Dotguard `commit-msg` refuses a subject outside the Conventional Commits header form.
It also refuses a description that starts with an uppercase letter or ends with a period, and a header longer than 72 characters.
It names the `type(scope): description` form.
A repository opts out with `git config guard.commits off` when its own instructions state another convention.

### F4:

The managed agent permission profiles deny `gh pr create`, `gh pr edit`, and `gh pr ready`, and the denial names the matching `pr-workflow` command.

### F7:

The pre-push dispatcher asks the CodeRabbit guard for the PR review pool's capacity before pushing a branch with an open PR.
It keeps that capacity apart from the CLI pool, with the applicable adaptive threshold.
It refuses the push when the capacity stays unverified or runs out, and it names the pool and the capacity it found.

### F8:

Reuse the existing ADR checker and index generator through an installed command for personal repositories.
Preserve historical document names and references, and check the exact staged or pushed tree.
Adopt the same checker in repository checks and required CI.
