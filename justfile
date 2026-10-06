set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# mise's paranoid mode loads this checkout's configuration only after a trust that records its current contents.
[private]
trust:
    mise trust --quiet {{quote(justfile_directory() / "mise.toml")}}

check *args: trust
    mise x -- cargo xtask check {{args}}

check-staged: trust
    mise x -- cargo xtask check --staged
    mise x -- cargo run --locked --package prose --bin prose -- --source . repository --scope comments

commit-message file: trust
    mise x -- cargo run --locked --package prose --bin prose -- --source . check --channel commit {{quote(file)}}

profiles: trust
    mise x -- cargo xtask profiles

proofs *args: trust
    mise x -- cargo xtask proofs {{args}}

classify *args: trust
    mise x -- cargo xtask classify {{args}}

rehearse *args: trust
    mise x -- cargo xtask rehearse {{args}}

skills: trust
    mise x -- cargo xtask skills

adapters: trust
    mise x -- cargo xtask check-adapters

secrets: trust
    mise x -- cargo xtask check-secrets

gen: trust
    mise x -- cargo xtask gen

decide skill: trust
    mise x -- cargo xtask decide {{quote(skill)}}

hosts *args: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles-xtask -- hosts {{args}}

next *flags: trust
    mise x -- cargo xtask next {{flags}}

diff config destination: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles-xtask -- profile diff --config {{quote(config)}} --destination {{quote(destination)}}

apply config destination backup: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles-xtask -- profile apply --config {{quote(config)}} --destination {{quote(destination)}} --backup {{quote(backup)}} --live

verify config destination: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles-xtask -- profile verify --config {{quote(config)}} --destination {{quote(destination)}}

refresh config: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles-xtask -- refresh --config {{quote(config)}} --live

doctor: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles-xtask -- doctor --strict

prose: trust
    mise x -- cargo run --locked --package prose --bin prose -- --source . repository --scope documents --scope comments

ops-check: trust
    mise x -- cargo xtask ops-check

install-review-guard: trust
    mise x -- cargo xtask install-review-guard

install-skill-ops: trust
    mise x -- cargo xtask install-skill-ops

install-prose: trust
    mise x -- cargo xtask install-prose

install-pr-workflow: trust
    mise x -- cargo xtask install-pr-workflow
