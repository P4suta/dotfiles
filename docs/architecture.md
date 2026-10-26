# Architecture

This repository renders one chezmoi source state for four host profiles: `mac`, `linux`, `windows`, and `wsl`.
Rust tools under `crates/` apply it and run every Git hook, and the development tasks in `xtask/` check it.
The installed runtime `dotfiles` builds from `crates/dotfiles/`, which also holds the Windows setup behind `dotfiles windows`.
`skill-ops`, `pr-workflow`, `prose`, and the `coderabbit` guard each build from their own crate, and `dotguard` from `crates/dotguard/`.
Every crate starts a process through `crates/platform/`, which also holds the home, the managed `PATH`, the tool registry, and the setup steps.
Every pure decision lives in `crates/rules/`, which has no dependency and no standard library, and each tool re-exports the rule modules it calls, as [its decision record](adr/pure-decision-core-behind-adapters.md) states.
One Cargo workspace at the root builds them with one lockfile, one toolchain from `mise.toml`, and one lint policy.
[ADR 0023](adr/0023-repository-layout-and-target-architecture.md) records this layout and the steps that remain.

`dotfiles repository check --remote origin` resolves the named Git remote's URL to its GitHub owner/name before checking the description.
The `--remote` option also accepts an explicit GitHub owner/name.
`dotfiles repository generate --remote origin` uses the same resolution to update the description.

## Render flow

1. `.chezmoiroot` names `home/`, so chezmoi reads the source state there while every command passes the checkout root as `--source`.
2. A machine-local configuration outside the checkout selects the profile and holds the Git identity, the signing key, and the project root.
   `home/.chezmoi.toml.tmpl` defines it once: `chezmoi init` writes it with the profile detected from the host, and the profile gates render their fixtures from it.
   `dotfiles profile doctor` refuses a configured profile that differs from the detected one.
3. The TOML files in `home/.chezmoidata/` hold the data, split by concern: shared values at the top level and per-profile values under `platforms.<profile>`.
   Templates read each value at its explicit path, and `crates/model` refuses an unknown key with its file.
4. Every target renders from one template at its own path.
   A target that a single profile receives holds its body there, and a small difference between profiles stays an inline condition.
   A target whose bodies differ more selects `home/.chezmoitemplates/native/<topic>/<profile>`, which can serve further profiles with the same body.
   `home/.chezmoitemplates/partials/` holds the shared partials, and the profile gate refuses any other file there, a duplicate native body, and an unused template.
5. `targets.toml` and `retired.toml` in `home/.chezmoidata/` name, through the sets of `profiles.toml`, the profiles that receive each restricted target and that remove each retired one.
   `home/.chezmoiignore` and `home/.chezmoiremove` render from them, and `just gen` writes [the target reference](reference/targets.md).
6. `home/.chezmoiscripts/unix/` and `home/.chezmoiscripts/windows/` hold the setup launchers, which `just gen` writes from the steps in `crates/platform/src/step.rs`.
   Each step declares its profiles, the steps it relies on, when it reruns, and its inputs, and each launcher's name carries its position in the run order.
   The first launcher installs the pinned mise after it checks the release's SHA-256, and the second builds the runtime from the checkout with the toolchain mise pins.
   Every other launcher passes `CHEZMOI_WORKING_TREE` and `CHEZMOI_CONFIG_FILE` to `dotfiles setup STEP`, and each `run_onchange_` launcher hashes only its own inputs through the `inputs` partial.
   `home/.chezmoidata/scripts.toml` restricts each launcher to its profiles.
7. `just apply` runs as a transaction that restores every target when the apply or its verification fails.

## Hook chain

`~/.config/git/config` sets `core.hooksPath` to `~/.config/git/hooks` on every host, and each hook there executes `dotfiles hook <name>`.
`crates/dotfiles/src/hooks.rs` then runs the gates that `plan` in `crates/rules/src/hook_rules.rs` lists, stops at the first refusal, and only warns when a notification fails.
`crates/dotfiles/tests/hook_exec_count.rs` counts the processes each hook spawns and compares the count with the `exec-count` metrics of `policy/baselines.toml`.
The plan runs, in order:

1. For `pre-push`, the push pause marker.
2. For `pre-commit`, `commit-msg`, and `pre-push`, the process start health probe.
3. For `pre-commit`, `commit-msg`, `pre-push`, and `post-commit`, the matching `dotguard` gate.
4. For `pre-push`, the host family evidence gate, then the ready pull request gate.
5. For `pre-commit` and `pre-push`, the verification gate, which refuses a change that adds code of a shape nothing in the repository verifies.
6. For `commit-msg`, the prose checker on the message.
7. For `pre-commit`, the global Lefthook configuration, then for every hook but `post-commit` and `reference-transaction`, the repository's own Lefthook configuration.
   A repository with its own Lefthook configuration first has its mise configuration trusted, because mise runs in paranoid mode and the hook runs that repository's commands anyway.
8. For `pre-commit`, a refusal when a hook changed the staged tree.
9. For `pre-push`, the Renovate freshness gate.
10. For `post-commit` and `post-merge`, the reinstallation of tools built from changed sources.
11. For `post-checkout`, `post-merge`, and a committed `reference-transaction`, the storage maintenance notification.

## Verification on every repository

Agent mutation hooks run `dotfiles lint-hook` over only the written paths.
The same bounded linter adapter checks scripts before the Bash guard admits execution.
The pure `lint_hook_rules` selects file types, linters, configuration sources, and admission, while repository configuration keeps its supported exclusions.
Missing linters, timeouts, and findings block with reproducing commands.

The Bash guard collects GitHub publication bodies during the same bounded shell traversal that inspects legacy commands.
The pinned syntax parser supplies source spans, and a static-word decoder preserves quoting and refuses unresolved expansions.
File inputs need one direct invocation, no redirects, bounded regular UTF-8 files, and complete JSON with unique keys.
Relative files also need a known execution directory.
One literal standard-input heredoc supplies exact bytes.
Pipelines and expandable heredocs leave inspection incomplete.
Close and reopen comments, merge bodies, and nested API body fields share the issue or PR prose channel and the attribution checks.
Legacy exceptions never exempt publication prose.
Structured coauthor records keep names as metadata.
The checker still inspects ordinary colon prose and malformed records.
These checks assume that the executable is the supported GitHub CLI and that files and context stay fixed between inspection and execution.
They establish neither a coauthor's identity nor atomic enforcement at the network boundary.

Codex runs command hooks through the configured session shell.
The Windows profile uses PowerShell invocation syntax and `dotfiles codex-hooks run` to preserve each hook's input, output, and exit status.
The wrapper records its start, child completion, and failure diagnostics under `~/.local/state/dotfiles/codex-hooks.jsonl`.
`dotfiles codex-hooks logs --limit 20` prints recent execution evidence, and missing or unfinished runs supply no success evidence.
The log keeps execution metadata and standard error, while the hook input and standard output pass through to the child and Codex.
An exclusive file lock protects concurrent appends and log compaction.
Native process, stream, file-lock, and PowerShell behavior form external boundaries covered by integration checks.
Codex serializes shell calls as `Bash` and omits their dialect, so the Windows wrapper supplies the native PowerShell context to the shared guard while keeping explicit input metadata.
When a session starts outside a checkout, the edit guard discovers the target checkout and checks its signed issue authority.
A session inside a checkout keeps that checkout's path boundary.

Skill observations and maintenance holds default to off.
An [explicit machine-local experimental opt-in](reference/experimental-skill-maintenance.md) keeps the engine available.
The opt-in leaves required repository verification and historical maintenance records unchanged.
Ordinary agents read the applicable skill instructions directly.

`home/dot_config/assure/shapes.toml` names the code shapes every repository on these machines must verify: pure decisions, protocols, and parsers, each with the signals that find it and the owner it needs.
The same gate applies the anti-pattern catalog beside it, and counts each finding as a gap.
The TypeScript number rule leaves declaration files, tests, stubs, and fixtures to their generators and test checks.
For generated TypeScript, the survey resolves repository `.gitattributes` from the same working tree, index, or commit as the source.
A changed or removed attribute file rechecks the source it can classify.
An isolated Git repository applies the [attribute precedence](https://git-scm.com/docs/gitattributes) and [NUL-delimited attribute protocol](https://git-scm.com/docs/git-check-attr).
The isolated context ignores owner and system attribute overrides.
Only `linguist-generated` set or `true` omits TypeScript from this catalog, and other values keep the ordinary checks.
Git's attribute semantics remain an external assumption checked by native regression fixtures, while the pure marker decision has a source-bound proof obligation.
The attribute fuzz corpus uses line-separated fields that the target converts to NUL separators and checks through decoding and canonical marker encoding.
The owner's configuration names each repository's owners and budgets under `~/.config/assure/repos/`, as [the owner-map decision](adr/keep-the-assure-owner-map-out-of-projects.md) describes.
An exemption sits in the comment block that precedes the site it exempts.
`dotfiles assure start` reports the gaps when an agent session starts, and the stop hook refuses to end a turn that leaves a gap in a file it changed.
Claude Code and Codex also pass the owner's `reply_language` to this hook to check final reply prose, with at most one continuation, as [the language decision](adr/check-owner-reply-language-at-stop.md) describes.
The hook and every prose channel share the sentence layout rules in `crates/rules/src/prose_rules.rs`.
They refuse two sentences on one line and a sentence that wraps across adjacent prose lines.
They report the first line and tell writers to split long sentences when a channel limits line length.
Headings, labels ending in a colon, lists, tables, code, quotes, and front matter keep their structural line rules.
The `pre-commit` and `pre-push` hooks refuse a change that adds a gap.
`pr-workflow ready` also refuses a PR that weakens a budget.
Existing gaps remain in the session report without requiring an unrelated repair in every PR.
Merge checks account for inherited code, as [the change-provenance decision](adr/assurance-follows-change-provenance.md) describes.

## Gate map

| Command | Runs |
| --- | --- |
| `just check` | The gates a change selects: the lint stages, prose, links, and generated files, the shared skills, the core boundary and the structural catalog, the Rust packages, and the profiles. |
| `just push-check` | Optional native implementation checks, plus the proof campaign on Mac and fixed CPU measurement budgets on Linux. The hosts gate records actual results for explicit native verification. |
| `just check-staged` | The pre-commit check: the lint stages over the staged files, then the prose of documents and of the comments `OComment` reads. |
| `just baselines` | The refusal of a lint baseline under `policy/lint-baselines/` that grew since a base, which the native check runs from its selected base. |
| `just profiles` | Every profile's rendering, an isolated apply and verification, the committed manifests, the ignore and remove audit, and the script input audit. |
| `just proofs` | The Kani obligations of `policy/obligations.toml`, Miri over the pure core's tests for the host and Windows, with Linux also interpreted on Mac, and a mutation campaign over the core. It also replays every fuzz corpus and verifies the Quint, Alloy, and Lean models. Linux also runs the fixed CPU budgets, while Mac reports their separate mandatory Linux Code gate. |
| `just proofs-ci` | The explicit manual campaign runs Kani, Miri, fuzz corpus replay, fixed CPU budgets, Quint, Alloy, and Lean. It excludes mutation testing and grants no native publication credit. Ordinary hosted CI doesn't run it. |
| `just prose` | The writing standard over documents and comments. |

The credential-free Code stage checks a clean candidate bundle and supplies implementation evidence.
It doesn't authenticate the committed change's issue authority.
The edit guard and staged pre-commit check enforce that authority before publication.
The [lightweight hosted workflow](adr/native-verification-with-lightweight-ci.md) reports its checked-out commit and tree on all three OS families and preserves required status propagation.
It repeats no native tests, proofs, or provisioning rehearsals.
For a pull request from someone other than the owner, such as a Renovate update, its [dependency gate](adr/gate-pull-requests-that-skip-local-hooks.md) installs Rust and compiles the workspace and checks the toolchain pins.
Its successful status doesn't independently establish native verification or issue authority.
The [candidate publication policy](adr/optional-native-verification-before-publication.md) makes native verification optional for dotfiles.
Pre-push and PR readiness read this policy from the exact candidate commit, and missing policy still requires native evidence.
Local staged checks, required hosted checks, signing, issue authority, and the assessed review route remain active.

Prose gates check persisted files, comments, Markdown, commit messages, issues, pull requests, and release notes.
The adapters setup step and `just install-prose` remove retired Claude Code and Codex reply hooks and the OpenCode reply plugin.

The Rust gate of `just check` also runs the obligation closure, which holds `policy/obligations.toml` to the source.
The workflow gate refuses a pre-push configuration that adds a different validation environment or loses the ref-update input.
The code stage hashes every checked file, and the real stage also hashes the runtime sources its installation builds.
An unchanged stage reuses passing evidence, while changed inputs require another run.
The [local proof campaign](adr/local-proof-campaigns-on-mac.md) runs on Mac with its declared native target semantics and Linux Miri interpretation.
The [fixed CPU budgets](adr/fixed-cpu-budgets-beside-mac-proof-campaigns.md) run in the mandatory Linux code stage, so a Mac campaign result alone grants no push evidence.
Linux and Mac code stages finish before native provisioning begins.
It refuses a rule module without a Kani obligation, a harness outside the listed modules, and a public rule function that no harness mentions and no exemption beside it covers.
It also refuses a proved function that no production code calls, a pilot whose issue closed without a decision record, and a metric of `policy/baselines.toml` that moved.

Each rule module keeps its properties in `spec_*` functions of its `proofs` module.
Its Kani harnesses prove them for every input, and its tests run them over every input they can list.
The mutation campaign of `just proofs` runs those tests, since cargo-mutants can't run Kani, and refuses a surviving mutant or a timeout that `policy/obligations.toml` doesn't name as non-terminating.
Mutation workers and their shared Cargo jobserver use at most eight slots, with one test thread per worker.
Completion requires a successful baseline, every selected mutant's terminal result, and matching counters and exit status.
Failed campaigns keep logs and mutation reports outside the scratch scope.

A Quint model under `specs/` describes a workflow that races other actors, such as `pr-workflow ready` while other hosts push.
`just proofs` checks it in a pinned image without a network.
`quint verify --backend tlc` visits every reachable state with one worker, and the obligation records the exact count of distinct states.
A probe module models the code before its fix, and the model checker must find its violation.
The rule module with the same step function replays the committed traces and compares the whole state after every step.
The gate writes those traces again at the fixed seed to show they still match.

Each Alloy assertion has an instance and a mutated fact that yields a counterexample at the same recorded scope.
Every model image verifies the pinned SHA-256 of its downloaded verifier bytes before using them.
The model gate also requires checksum refusals for a corrupted copy of each verifier asset.
`just gen` exports the actual profile manifests as an Alloy instance, so the model checks the deployed target, retirement, directory, and hook relations.
Lean proves the general skill plan and setup order theorems.
Generated vectors compare its definitions with the Rust functions, and the gate checks each module in a fresh kernel and refuses any axiom outside the three standard axioms.

`crates/rules/src/change_rules.rs` maps each changed path to the gates it selects, and a path no rule names selects every gate.
Only a rule module, the proof inventory, or the tooling selects the proofs, because each harness compiles one rule module alone.

The lint stages run each pinned linter: rustfmt, typos, taplo, yamllint, shellcheck, and `PSScriptAnalyzer`.
An encoding stage refuses a byte order mark, malformed UTF-8, and a missing final newline, which the profile gate also refuses in every rendered file.
Each stage holds its findings to a baseline in `policy/lint-baselines/`, which counts them per path and rule.
The gate refuses a finding beyond the baseline and an entry that admits more than its path holds, so a fixed finding leaves the baseline for good.
