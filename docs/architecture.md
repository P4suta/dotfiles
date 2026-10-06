# Architecture

This repository renders one chezmoi source state for four host profiles: `mac`, `linux`, `windows`, and `wsl`.
Rust tools under `crates/` apply it and run every Git hook, and the development tasks in `xtask/` check it.
The installed runtime `dotfiles` builds from `crates/dotfiles/`, which also holds the Windows setup behind `dotfiles windows`.
`skill-ops`, `pr-workflow`, `prose`, and the `coderabbit` guard each build from their own crate, and `dotguard` from `crates/dotguard/`.
Every crate starts a process through `crates/platform/`, which also holds the home, the managed `PATH`, the tool registry, and the setup steps.
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
`crates/dotfiles/src/hooks.rs` then runs, in order:

1. For `pre-commit`, `commit-msg`, and `pre-push`, the process start health probe.
2. For `pre-push`, the push pause marker.
3. For `pre-commit`, `commit-msg`, `pre-push`, and `post-commit`, the matching `dotguard` gate.
4. For `pre-push`, the host family evidence gate, then the ready pull request gate.
5. For `commit-msg`, the prose checker on the message.
6. For `pre-commit`, the global Lefthook configuration, then for every hook but `post-commit` and `reference-transaction`, the repository's own Lefthook configuration.
   A repository with its own Lefthook configuration first has its mise configuration trusted, because mise runs in paranoid mode and the hook runs that repository's commands anyway.
7. For `pre-commit`, a refusal when a hook changed the staged tree.
8. For `pre-push`, the Renovate freshness gate.
9. For `post-merge`, the reinstallation of tools built from changed sources.
10. For `post-checkout`, `post-merge`, and a committed `reference-transaction`, the storage maintenance notification.

## Gate map

| Command | Runs |
| --- | --- |
| `just check` | The gates a change selects: the lint stages, prose, links, and generated files, the shared skills, the Rust packages, and the profiles. |
| `just check-staged` | The pre-commit check: the lint stages over the staged files, then the prose of documents and of the comments `OComment` reads. |
| `just baselines` | The refusal of a lint baseline under `policy/lint-baselines/` that grew since a base, which CI runs from the merge base. |
| `just profiles` | Every profile's rendering, an isolated apply and verification, the committed manifests, the ignore and remove audit, and the script input audit. |
| `just proofs` | The Kani harnesses and rejecting counterexamples of every rule module, each within its time budget. |
| `just prose` | The writing standard over documents and comments. |

`xtask/src/change_rules.rs` maps each changed path to the gates it selects, and a path no rule names selects every gate.
Only a rule module, the proof inventory, or the tooling selects the proofs, because each harness compiles one rule module alone.

The lint stages run each pinned linter: rustfmt, typos, taplo, yamllint, shellcheck, and `PSScriptAnalyzer`.
An encoding stage refuses a byte order mark, malformed UTF-8, and a missing final newline, which the profile gate also refuses in every rendered file.
Each stage holds its findings to a baseline in `policy/lint-baselines/`, which counts them per path and rule.
The gate refuses a finding beyond the baseline and an entry that admits more than its path holds, so a fixed finding leaves the baseline for good.
