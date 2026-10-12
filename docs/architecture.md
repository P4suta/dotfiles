# Architecture

This repository renders one chezmoi source state for four host profiles: `mac`, `linux`, `windows`, and `wsl`.
Rust tools under `crates/` apply it and run every Git hook, and the development tasks in `xtask/` check it.
The installed runtime `dotfiles` builds from `crates/dotfiles/`, which also holds the Windows setup behind `dotfiles windows`.
`skill-ops`, `pr-workflow`, `prose`, and the `coderabbit` guard each build from their own crate, and `dotguard` from `crates/dotguard/`.
Every crate starts a process through `crates/platform/`, which also holds the home, the managed `PATH`, the tool registry, and the setup steps.
Every pure decision lives in `crates/rules/`, which has no dependency and no standard library, and each tool re-exports the rule modules it calls, as [its decision record](adr/pure-decision-core-behind-adapters.md) states.
One Cargo workspace at the root builds them with one lockfile, one toolchain from `mise.toml`, and one lint policy.
[ADR 0023](adr/0023-repository-layout-and-target-architecture.md) records this layout and the steps that remain.

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

`home/dot_config/assure/shapes.toml` names the code shapes every repository on these machines must verify: pure decisions, protocols, and parsers, each with the signals that find it and the owner it needs.
The same gate applies the anti-pattern catalog beside it, and counts each finding as a gap.
A repository names its owners and budgets in `assure.toml` at its root, and an exemption sits in the comment block that precedes the site it exempts, as [its decision record](adr/verification-proportional-to-code-shape.md) describes.
`dotfiles assure start` reports the gaps when an agent session starts, and the stop hook refuses to end a turn that leaves a gap in a file it changed.
The `pre-commit` and `pre-push` hooks refuse a change that adds a gap.
`pr-workflow ready` also refuses a PR that weakens a budget, and in a repository with gaps one that closes none.

## Gate map

| Command | Runs |
| --- | --- |
| `just check` | The gates a change selects: the lint stages, prose, links, and generated files, the shared skills, the core boundary and the structural catalog, the Rust packages, and the profiles. |
| `just push-check` | Every check on the native host, plus the complete proof suite on Mac. The hosts gate records these results, and pre-push admits only matching candidate evidence. |
| `just check-staged` | The pre-commit check: the lint stages over the staged files, then the prose of documents and of the comments `OComment` reads. |
| `just baselines` | The refusal of a lint baseline under `policy/lint-baselines/` that grew since a base, which CI runs from the merge base. |
| `just profiles` | Every profile's rendering, an isolated apply and verification, the committed manifests, the ignore and remove audit, and the script input audit. |
| `just proofs` | The Kani obligations of `policy/obligations.toml`, Miri over the pure core's tests for the host and Windows, with Linux also interpreted on Mac, and a mutation campaign over the core. It also replays the corpus of every fuzz target under `fuzz/`, checks the Quint and Alloy models under `specs/`, and builds and checks the Lean package under `specs/lean/`. |
| `just prose` | The writing standard over documents and comments. |

Prose gates check persisted files, comments, Markdown, commit messages, issues, pull requests, and release notes.
The adapters setup step and `just install-prose` remove retired Claude Code and Codex reply hooks and the OpenCode reply plugin.

The Rust gate of `just check` also runs the obligation closure, which holds `policy/obligations.toml` to the source.
The workflow gate refuses a pre-push configuration that adds a different validation environment or loses the ref-update input.
The code stage hashes every checked file, and the real stage also hashes the runtime sources its installation builds.
An unchanged stage reuses passing evidence, while changed inputs require another run.
The [local proof campaign](adr/local-proof-campaigns-on-mac.md) runs on Mac, while required CI proofs keep Linux target semantics.
Linux and Mac code stages finish before native provisioning begins.
It refuses a rule module without a Kani obligation, a harness outside the listed modules, and a public rule function that no harness mentions and no exemption beside it covers.
It also refuses a proved function that no production code calls, a pilot whose issue closed without a decision record, and a metric of `policy/baselines.toml` that moved.

Each rule module keeps its properties in `spec_*` functions of its `proofs` module.
Its Kani harnesses prove them for every input, and its tests run them over every input they can list.
The mutation campaign of `just proofs` runs those tests, since cargo-mutants can't run Kani, and refuses a surviving mutant or a timeout that `policy/obligations.toml` doesn't name as non-terminating.

A Quint model under `specs/` describes a workflow that races other actors, such as `pr-workflow ready` while other hosts push.
`just proofs` checks it in a pinned image without a network.
`quint verify --backend tlc` visits every reachable state with one worker, and the obligation records the exact count of distinct states.
A probe module models the code before its fix, and the model checker must find its violation.
The rule module with the same step function replays the committed traces and compares the whole state after every step.
The gate writes those traces again at the fixed seed to show they still match.

Each Alloy assertion has an instance and a mutated fact that yields a counterexample at the same recorded scope.
`just gen` exports the actual profile manifests as an Alloy instance, so the model checks the deployed target, retirement, directory, and hook relations.
Lean proves the general skill plan and setup order theorems.
Generated vectors compare its definitions with the Rust functions, and the gate checks each module in a fresh kernel and refuses any axiom outside the three standard axioms.

`crates/rules/src/change_rules.rs` maps each changed path to the gates it selects, and a path no rule names selects every gate.
Only a rule module, the proof inventory, or the tooling selects the proofs, because each harness compiles one rule module alone.

The lint stages run each pinned linter: rustfmt, typos, taplo, yamllint, shellcheck, and `PSScriptAnalyzer`.
An encoding stage refuses a byte order mark, malformed UTF-8, and a missing final newline, which the profile gate also refuses in every rendered file.
Each stage holds its findings to a baseline in `policy/lint-baselines/`, which counts them per path and rule.
The gate refuses a finding beyond the baseline and an entry that admits more than its path holds, so a fixed finding leaves the baseline for good.
