# Dotfiles

One chezmoi source for Mac, Linux, Windows, and WSL, and the Rust tools that apply and check it.
[The architecture](docs/architecture.md) covers the render flow, the hook chain, and the gate map.

## Layout

| Path | Holds |
| --- | --- |
| `home/` | The chezmoi source state that `.chezmoiroot` names. |
| `home/.chezmoiscripts/` | The setup launchers, one directory per OS family. |
| `home/.chezmoitemplates/` | Shared partials under `partials/` and per-profile bodies under `native/`. |
| `xtask/` | The task runner and the installed runtime behind every hook and launcher. |
| `crates/dotguard/` | `dotguard`, the Git wrapper, and the reaper and herdr-agent binaries. |
| `crates/dotctl/` | Windows machine setup. |
| `Cargo.toml` | The workspace of `xtask/` and `crates/`, with its one lint policy, lockfile, and `clippy.toml`. |
| `policy/` | Data the gates read. |
| `docs/` | Architecture, [decision records](docs/adr/README.md), and skill decisions. |
| `ops/` | Ansible inventory and runbooks. |

## Commands

- `just check` runs the gates a change selects.
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
| A setup step | `home/.chezmoiscripts/` and `xtask/src/setup.rs` |
| Shared data and tool versions | `home/.chezmoidata/`, typed in `crates/model/src/data.rs`, `mise.toml`, and `mise.lock` |
| A gate | `xtask/src/` with its tests in `xtask/tests/` |
| A rule a gate decides | `xtask/src/*_rules.rs` and its entry in `xtask/src/skill_proofs.rs` |
| A shared skill | `home/dot_agents/skills/<skill>/` and `docs/skills/decisions/<skill>.json` |
| A durable design decision | a new record in `docs/adr/`, then `just gen` |

## Rules

- Every rule module has unit tests, Kani harnesses, and a rejecting counterexample in the proof inventory.
- A change that alters what a host receives updates both manifest sets in `xtask/tests/snapshots/` and explains the difference.
- Lint exceptions use `#[expect]` with a reason.
