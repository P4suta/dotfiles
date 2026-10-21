set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# mise's paranoid mode loads this checkout's configuration only after a trust that records its current contents.
[private]
trust:
    mise trust --quiet {{quote(justfile_directory() / "mise.toml")}}

[private]
lint-tools: (mise-install "python" "uv" "yamllint")

mise-install *args: trust
    mise x rust -- cargo run --locked --package dotfiles --bin dotfiles -- mise-install {{args}}

tool-locks: trust
    mise x rust -- cargo xtask tool-locks

check *args: lint-tools
    mise x -- cargo xtask check {{args}}

push-check: lint-tools
    mise x -- cargo xtask push-check

# The hosted gate for a pull request that skipped the local hooks, such as a Renovate update.
dependency-gate: trust
    mise x rust -- cargo check --locked --workspace --all-targets
    mise x rust -- cargo test --locked --package xtask --test pins

check-staged: lint-tools
    mise x -- cargo xtask check --staged

baselines *args: lint-tools
    mise x -- cargo xtask baselines {{args}}

commit-message file: trust
    mise x -- cargo run --locked --package prose --bin prose -- --source . check --channel commit {{quote(file)}}

profiles: trust
    mise x -- cargo xtask profiles

profiles-linux: trust
    mise x -- cargo xtask profiles-linux

proofs *args: trust
    mise x -- cargo xtask proofs {{args}}

proofs-ci *args: trust
    mise x -- cargo xtask proofs-ci {{args}}

verification-prepare *args: trust
    mise x -- cargo xtask verification-env prepare {{args}}

verification-doctor *args: trust
    mise x -- cargo xtask verification-env doctor {{args}}

verification-smoke *args: trust
    mise x -- cargo xtask verification-env smoke {{args}}

njutest-assess *args: trust
    mise x rust -- cargo xtask njutest-assessment {{args}}

graphs: trust
    mise x -- cargo xtask graphs

classify *args: trust
    mise x -- cargo xtask classify {{args}}

issue-context *args: trust
    mise x -- cargo run --locked --package pr-workflow --bin pr-workflow -- change-check {{args}}

rehearse *args: trust
    mise x -- cargo xtask rehearse {{args}}

skills: trust
    mise x -- cargo xtask skills

inventory: trust
    mise x -- cargo xtask inventory

adapters: trust
    mise x -- cargo xtask check-adapters

secrets: trust
    mise x -- cargo xtask check-secrets

gen: trust
    mise x -- cargo xtask gen

decide skill: trust
    mise x -- cargo xtask decide {{quote(skill)}}

hosts *args: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles -- hosts {{args}}

next *flags: trust
    mise x -- cargo xtask next {{flags}}

diff config destination: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles -- profile diff --config {{quote(config)}} --destination {{quote(destination)}}

apply config destination backup: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles -- profile apply --config {{quote(config)}} --destination {{quote(destination)}} --backup {{quote(backup)}} --live

verify config destination: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles -- profile verify --config {{quote(config)}} --destination {{quote(destination)}}

refresh config: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles -- refresh --config {{quote(config)}} --live

doctor: trust
    mise x -- cargo run --locked --package dotfiles --bin dotfiles -- doctor --strict

prose: trust
    mise x -- cargo run --locked --package prose --bin prose -- --source . repository --scope documents --scope comments

ansible-syntax-check: trust
    mise x -- cargo xtask ansible-syntax-check

install-review-guard: trust
    mise x -- cargo xtask install-review-guard

install-skill-ops: trust
    mise x -- cargo xtask install-skill-ops

install-prose: trust
    mise x -- cargo xtask install-prose

install-pr-workflow: trust
    mise x -- cargo xtask install-pr-workflow
