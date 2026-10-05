# Rehearse the native apply and declare every external tool

Status: accepted.

## Context

The first native cutover found about fifteen defects that every required gate had passed, and a scoped review found sixteen more in imported helpers.
[Public native profiles](0009-public-native-profiles.md) checked profiles by rendering them and applying files in owned fixtures, without scripts, outside the native home, session, and machine state.
No live `profile apply`, script, native verification, rollback, or takeover of a machine that an earlier repository provisioned had run before the owner's machines changed.

The defects share five mechanisms:

- Untested native paths let four defects reach real machines: a JSON parse of a text listing, an always-failing verification, a helper reinstall conflict, and Windows link removal.
- Fixtures repeated the code's assumptions about external tools, such as a backslash home path that chezmoi never reports and a single forge key that GitHub had rotated.
- Environment needs stayed implicit: tools on `PATH`, an interactive desktop, an elevated session, and machine-local state such as a Nix lock.
- Bulk migration and configuration scopes ignored references, dropping files that code read and matching files that a prose rule then corrupted.
- Untyped path strings let Windows verbatim, forward-slash, and backslash spellings reach tools that accept only some of them.

## Decision

Required CI rehearses the native apply on disposable Linux, macOS, and Windows hosts.
`xtask rehearse` writes an anonymous machine-local configuration and seeds existing review history and helper binaries from another cargo package.
It then applies the native profile with its scripts and verifies it, three times.
Before the second apply, it forgets which scripts ran, so every script runs again on the host it provisioned with services loaded and running.
That path takes over an earlier repository's machine.
The third apply changes nothing.
It refuses to run unless CI reports a disposable host and the caller passes `--disposable-host`.
Provisioning lists keep one representative entry and every entry that a later step invokes, so each step's real code path runs with small inputs.
The apply omits a script that the host has no way to run only through a `setup.skip` entry with the script name and a reason.
It prints those entries before any change.

Each external program has a variant of `xtask::tool::Tool` with a declared provision: pinned in `mise.toml`, provided by the host, installed by a named setup step, or installed by the owner.
`clippy.toml` disallows `Command::new` and path canonicalization outside the registry and `xtask::canonical`, so every new host dependency or path spelling passes through the declared boundary.
Integration tests may spawn the binaries and real tools they verify, and contract tests run the pinned chezmoi and Git instead of fakes.

Setup steps declare their dependencies, and the profile gate checks the declarations with what each profile renders.
`Step::after` names the steps that must run first, and the gate refuses a rendered script order, before scripts and then after scripts by name, that breaks one.
`Step::requires` names the packaged tools a step invokes, and `Tool::package` names the list that installs each one per profile.
The gate refuses data that omits a required package, and the rehearsal derives its reduced lists from the same declarations.
Installed entry points carry the same obligation, so the gate reads every rendered file.
A rendered launcher that reads the secrets of the machine-local configuration requires Doppler.
Both matches list every step, so a new step forces a decision about its order and packages.
The apply renders its scripts first and refuses any step that the session has no way to serve, such as a private source without non-interactive credentials.
It also refuses while a managed file differs from the last apply outside chezmoi, and names every such file.
A fix made on a host belongs in the source, and nobody can answer an overwrite prompt mid-apply.
Both refusals happen before any change.
After scripts run, the apply writes files once more without scripts before verification, because templates may probe for programs those scripts installed.

The gate also refuses four structural mistakes:

- A launcher or Git hook that locates its program through the runtime `HOME`, which tools replace when they run Git.
- A profile leaf that no template includes.
- An OComment language override that names a directory pattern or a missing file.
- A `just` invocation in a hint or document that names no recipe in the justfile.

`dotctl` compiles in the repository files it reads.

Each defect gets its fix in the boundary that owns it:

- Native verification excludes always-run scripts.
- Rollback skips untouched targets and removes Windows directory links.
- Managed listings parse as null-separated text.
- Helper installation replaces earlier sources.
- Public owner sources clone over HTTPS despite host rewrites.
- The forge bundle passes only when every primary key has a pin and the current key appears.
- A LaunchAgent bootstraps only after launchd has removed its running predecessor, which a test checks with the real launchd.

## Alternatives

More fixtures lost because a fixture encodes the same assumption as the code it checks, and the backslash home path and the single forge key both passed fixtures.
Manual native testing on the owner's machines lost as the primary gate: it changes real homes, depends on the operator, and costs too much to repeat per change.
Full-data rehearsal lost because installing every package on every pull request would take hours, and representative data keeps every orchestration path while the package lists stay native concerns.

## Consequences

Each pull request spends minutes of hosted runner time per platform on rehearsal, free for the public repository.
The rehearsal never touches the owner's credentials, 1Password, desktop sessions, Tailscale, or large existing state.
Those stay native verification, and a step that needs them says so through `setup.skip` on hosts that lack them.
Disposable hosts run elevated without an interactive desktop, so the rehearsal also covers the remote-session conditions that broke the keyboard and Herdr steps.
The tool registry and lint show that every spawned program has a declaration, and the rehearsal and native preflight checks show that a declared provision exists on a host.
Step declarations cover packaged tools and private sources.
An undeclared need surfaces only in the rehearsal or on a native host, and then gets a declaration instead of a local workaround.
Chezmoi reads ignored build directories in its source, so contract tests read a copy without build outputs.
A concurrent build in a native checkout during an apply stays an operational hazard.
