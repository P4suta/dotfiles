# Rehearse native application and declare every external tool

Status: Accepted.

## Context

The first native cutover found about fifteen defects that every required gate had passed, and a scoped review found sixteen more in imported helpers.
[ADR 0009](0009-public-native-profiles.md) verified profiles by rendering them and applying files in owned fixtures, without scripts, outside the native home, session, and machine state.
Live `profile apply`, its scripts, native verification, rollback, and the takeover of a machine that earlier repositories provisioned had never run before an owner's machine changed.

The defects share five mechanisms rather than five hundred symptoms.
Native paths were never exercised, so a JSON parse of chezmoi's text listing, an always-failing verification, a helper reinstall conflict, and Windows link removal all reached real machines.
Contracts with external tools were assumed instead of checked, and fixtures repeated the same assumptions, for example a backslash home path that chezmoi never reports and a single forge key that GitHub had rotated.
Environment requirements were implicit: tools on PATH, an interactive desktop, an elevated session, and machine-local state such as a Nix lock.
Bulk migration and configuration scopes ignored references, dropping files that code read and matching files a prose rule then corrupted.
Paths were untyped strings, so Windows verbatim, forward-slash, and backslash spellings reached tools that accept only some of them.

## Decision

Required CI rehearses native application on disposable Linux, macOS, and Windows hosts.
`xtask rehearse` writes an anonymous machine-local configuration, seeds helper binaries owned by another cargo package and existing review history, then applies the native profile with its scripts and verifies it three times.
Before the second application it forgets which scripts ran, so every script runs again on the host it provisioned, with services loaded and running, which is the path that takes over an earlier repository's machine; the third application changes nothing.
It refuses to run unless CI reports a disposable host and `--disposable-host` is given.
Provisioning lists keep one representative entry and every entry that a later step invokes, so each step's real code path runs with small inputs.
A script the host cannot run is omitted only through a `setup.skip` entry that names the script and gives a reason; application prints those entries before any change, and the reasons are part of review.

External programs are variants of `xtask::tool::Tool`, each with a declared provision: pinned in `mise.toml`, provided by the host, installed by a named setup step, or installed by the owner.
`clippy.toml` disallows `Command::new` and path canonicalization outside the registry and `xtask::canonical`, so a new host dependency or path spelling cannot appear without passing through the declared boundary.
Integration tests may spawn the binaries and real tools they verify, and contract tests run the pinned chezmoi and git rather than fakes.

Setup steps declare what they rely on, and the profile gate checks the declarations against what each profile actually renders.
`Step::after` names the steps that must run first, and the gate rejects a rendered script order, before scripts and then after scripts by name, that breaks one.
`Step::requires` names the packaged tools a step invokes and `Tool::package` names the list that installs each one per profile; the gate rejects data that omits a required package, and the rehearsal derives its reduced lists from the same declarations.
Both matches list every step, so adding a step forces a decision about its order and packages.
Application renders its scripts first and refuses, before any change, a step whose requirement the session cannot meet, such as a private source without non-interactive credentials.
After scripts run, files are applied once more without scripts before verification, because templates may probe for programs those scripts installed.

The gate also rejects four structural mistakes that reached a host or a reviewer: a launcher or Git hook that locates its program through the runtime `HOME`, which tools replace when they run Git; a profile leaf that no template includes; an OComment language override that names a directory pattern or a missing file; and a `just` invocation in a hint or document that names no recipe in the justfile.
Repository files that `dotctl` reads are compiled into it.

Defects found this way are fixed in the boundary that owns them: native verification excludes always-run scripts, rollback skips untouched targets and removes Windows directory links correctly, managed listings are read as NUL-separated text, helper installation replaces earlier sources, public owner sources clone over HTTPS despite host rewrites, the forge bundle is accepted only when every primary key is pinned and the current key is present, and a LaunchAgent is bootstrapped only after launchd has removed its running predecessor, which a test checks against the real launchd.

## Alternatives

More fixtures were rejected because a fixture encodes the same assumption as the code it checks; the backslash home path and the single forge key both passed fixtures.
Manual native testing on the owner's machines was rejected as the primary gate because it changes real homes, depends on whoever runs it, and cannot be repeated for every change.
Full-data rehearsal was rejected because installing every package on every pull request would take hours; representative data keeps every orchestration path while the package lists themselves stay native concerns.

## Consequences

Each pull request spends several hosted runner minutes per platform on rehearsal, which the public repository receives without charge.
The rehearsal cannot exercise the owner's credentials, 1Password, desktop sessions, Tailscale, or large existing state; those remain native verification, and steps that need them must say so through `setup.skip` on hosts that lack them.
Disposable hosts run elevated without an interactive desktop, so the rehearsal also covers the remote-session conditions that broke the keyboard and Herdr steps.
The tool registry and lint establish that every spawned program is declared; they do not prove that a declared provision is present on a given host, which the rehearsal and native preflight checks must still show.
Step requirements cover packaged tools and private sources; a requirement that is not declared is still found only by the rehearsal or a native host, and should then be declared rather than handled where it failed.
chezmoi reads ignored build directories in its source, so contract tests read a copy without build outputs, and a concurrent build in a native checkout during application remains an operational hazard.
