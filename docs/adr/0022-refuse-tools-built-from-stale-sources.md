# Refuse installed tools built from stale sources

Status: Accepted.

## Context

An installed `pr-workflow` built before the PR-scope pause merged skipped the review exclusion and started CodeRabbit reviews.
Nothing noticed that the installation lagged the rules it was meant to enforce.
Installers that ran chezmoi without `--source` also read the default source, a bare repository whose head stays at an old commit, while `just apply` used the invoking checkout.

## Decision

`xtask/build.rs` embeds the revision, the checkout, and a SHA-256 over the files that build the tools: everything under `xtask/src`, the manifests and lockfiles, and each file the library compiles in with `include_str!` or `#[path]`.
`xtask/src/build_inputs.rs` is compiled by both the build script and the library, so the embedded hash and a checkout's hash cannot disagree about which files count.

`pr-workflow`, `coderabbit`, and `skill-ops`, including its stop hook, hash the same files in the checkout they can find before doing anything else.
They look at `DOTFILES_SOURCE`, then the dotfiles checkout containing the working directory, then the checkout the tool was built in.
A difference refuses with `tool.stale` and the next action `mise run install:<tool>`; no checkout means nothing is refused.

Each install task records the hash it installed from under `~/.local/state/dotfiles/installed`.
`just apply` and the global post-merge hook run the install task for each recorded installation whose sources changed, and never install a tool the owner did not install.

Every chezmoi command an installer runs comes from `Tool::chezmoi_in(checkout)`, and a test refuses any other construction, so nothing reads the default source.

The decisions are a pure core in `xtask/src/stale_rules.rs`, verified by Kani harnesses with rejecting counterexamples in `just proofs`.

## Alternatives

Comparing revisions instead of content was rejected because a merge that touches no build input would then force a rebuild, and a dirty checkout would pass.
Hashing only the binary's own source file was rejected because the library compiles every binary's source into each tool.

## Consequences

Editing any build input makes the installed tools refuse until the install task runs in that checkout.
Adding an `include_str!` of a new file requires listing it in `build_inputs::SHARED`, which a test enforces.
