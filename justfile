default:
    @just --list

check:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- check

profiles:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- profiles

proofs:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- proofs

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

ops-check:
    mise x -- cargo run --locked --manifest-path xtask/Cargo.toml -- ops-check
