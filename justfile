set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

check *args:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check {{args}}

check-staged:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check --staged
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml --bin prose -- --source . repository --scope comments

commit-message file:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml --bin prose -- --source . check --channel commit {{quote(file)}}

profiles:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profiles

proofs *args:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- proofs {{args}}

skills:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- skills

adapters:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check-adapters

secrets:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check-secrets

gen:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- gen

decide skill:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- decide {{quote(skill)}}

hosts *args:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- hosts {{args}}

next *flags:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- next {{flags}}

diff config destination state:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profile diff --config {{quote(config)}} --destination {{quote(destination)}} --state {{quote(state)}}

apply config destination state backup:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profile apply --config {{quote(config)}} --destination {{quote(destination)}} --state {{quote(state)}} --backup {{quote(backup)}} --live

verify config destination state:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profile verify --config {{quote(config)}} --destination {{quote(destination)}} --state {{quote(state)}}

refresh config:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- refresh --config {{quote(config)}} --live

doctor:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- doctor --strict

prose:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml --bin prose -- --source . repository --scope documents --scope comments

ops-check:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- ops-check

install-review-guard:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-review-guard

install-skill-ops:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-skill-ops

install-prose:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-prose

install-pr-workflow:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- install-pr-workflow
