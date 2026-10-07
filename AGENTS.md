# Dotfiles

One chezmoi source for Mac, Linux, Windows, and WSL, and the Rust tools that apply and check it.
[The architecture](docs/architecture.md) covers the render flow, the hook chain, and the gate map.

## Layout

| Path | Holds |
| --- | --- |
| `home/` | The chezmoi source state that `.chezmoiroot` names. |
| `home/.chezmoiscripts/` | The setup launchers that `just gen` writes, one directory per OS family. |
| `home/.chezmoitemplates/` | Shared partials under `partials/` and per-profile bodies under `native/`. |
| `xtask/` | The development tasks behind `cargo xtask` and the just recipes, which no hook or launcher runs. |
| `crates/platform/` | The home, the managed `PATH`, the one process constructor, the tool registry, the setup steps, and the build stamp. |
| `crates/dotfiles/` | The installed runtime `dotfiles` behind every hook and launcher, with the Windows setup behind `dotfiles windows`. |
| `crates/skill-ops/`, `crates/pr-workflow/`, `crates/prose/`, `crates/coderabbit-guard/` | Each installed tool with its rules. |
| `crates/dotguard/` | `dotguard`, the Git wrapper, and the reaper and herdr-agent binaries. |
| `crates/model/` | The typed chezmoi data. |
| `crates/rules/` | The pure decisions of every tool, which reach no dependency and no standard library. |
| `Cargo.toml` | The workspace of `xtask/` and `crates/`, with its one lint policy, lockfile, and `clippy.toml`. |
| `policy/` | Data the gates read. |
| `docs/` | Architecture, [decision records](docs/adr/README.md), and skill decisions. |
| `ops/` | Ansible inventory and runbooks. |

## Commands

- `just check` runs the gates a change selects, and `just check-staged` runs the lint stages over the staged files before each commit.
- `just proofs` runs the Kani proofs, in a Linux container on Windows.
- `just profiles` renders every profile and compares it with the committed manifests.
- `just diff`, `just apply`, and `just verify` preview, apply, and verify this host's profile.
- `just gen` writes the generated files, such as the decision record index.
- `just decide` drafts the decision record of a changed skill.

## Where to change what

| Intent | File |
| --- | --- |
| A file every host receives | `home/<source path>` |
| A file that differs per profile | an inline condition at its path, or `home/.chezmoitemplates/native/<topic>/<profile>` selected from there |
| Which profiles receive a target | one line in `home/.chezmoidata/targets.toml`, naming a set of `profiles.toml`, then `just gen` |
| A target to retire from hosts | one line in `home/.chezmoidata/retired.toml`, then `just gen` |
| A setup step | its declaration in `crates/platform/src/step.rs` and its work in `crates/dotfiles/src/setup.rs`, then `just gen` |
| Shared data and tool versions | `home/.chezmoidata/`, typed in `crates/model/src/data.rs`, `mise.toml`, and `mise.lock` |
| A gate | `xtask/src/` with its tests in `xtask/tests/`, or the crate of the tool that runs it |
| A rule a gate decides | a `*_rules.rs` module in `crates/rules/src/`, re-exported by its caller's crate, and its Kani obligation with its probes in `policy/obligations.toml` |
| A structural rule for every repository | `home/dot_config/assure/rules/` with its fixtures in `rule-tests/` and its entry in `catalog.toml` |
| A shared skill | `home/dot_agents/skills/<skill>/` and `docs/skills/decisions/<skill>.json` |
| A durable design decision | a new record `docs/adr/<decision>.md` with `date:` in its front matter and no number, then `just gen` |

## Rules

- Every rule module has unit tests, Kani harnesses, and a rejecting counterexample in the proof inventory.
- A rule module receives every fact as a value and returns its decision as a value, and the compiler and the structural gate refuse anything else.
- A change that alters what a host receives updates both manifest sets in `xtask/tests/snapshots/` and explains the difference.
- Lint exceptions use `#[expect]` with a reason.
- A number, URL, or system path with a meaning gets a named constant, and a cleanup failure goes through `platform::cleanup`, as the catalog in `home/dot_config/assure/` requires.
- The baselines in `policy/lint-baselines/` only shrink: fix a new finding instead of admitting it.
