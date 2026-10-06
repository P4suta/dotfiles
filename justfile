set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# mise's paranoid mode loads this checkout's configuration only after a trust that records its current contents.
[private]
trust:
    mise trust --quiet {{quote(justfile_directory() / "mise.toml")}}

check *args: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check {{args}}

check-staged: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check --staged
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml --bin prose -- --source . repository --scope comments

commit-message file: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml --bin prose -- --source . check --channel commit {{quote(file)}}

profiles: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profiles

proofs *args: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- proofs {{args}}

classify *args: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- classify {{args}}

rehearse *args: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- rehearse {{args}}

skills: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- skills

adapters: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check-adapters

secrets: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check-secrets

gen: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- gen

decide skill: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- decide {{quote(skill)}}

hosts *args: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- hosts {{args}}

next *flags: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- next {{flags}}

diff config destination state: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profile diff --config {{quote(config)}} --destination {{quote(destination)}} --state {{quote(state)}}

apply config destination state backup: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profile apply --config {{quote(config)}} --destination {{quote(destination)}} --state {{quote(state)}} --backup {{quote(backup)}} --live

verify config destination state: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profile verify --config {{quote(config)}} --destination {{quote(destination)}} --state {{quote(state)}}

refresh config: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- refresh --config {{quote(config)}} --live

doctor: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- doctor --strict

prose: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml --bin prose -- --source . repository --scope documents --scope comments

ops-check: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- ops-check

install-review-guard: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-review-guard

install-skill-ops: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-skill-ops

install-prose: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-prose

install-pr-workflow: trust
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-pr-workflow
