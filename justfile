set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

check:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check

profiles:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profiles

proofs:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- proofs

decide skill:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- decide {{quote(skill)}}

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
