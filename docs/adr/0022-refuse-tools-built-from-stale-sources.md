# Refuse installed tools built from stale sources

The owner accepted this decision.

## Context

An installed `pr-workflow` built before the PR-scope pause merged skipped the review exclusion and started CodeRabbit reviews.
No check revealed that the installation lagged the rules it should enforce.
Installers that ran chezmoi without `--source` also read the default source, a bare repository whose head stays at an old commit, while `just apply` used the invoking checkout.

## Decision

`xtask/build.rs` embeds the revision, the checkout, and a SHA-256 over the files that build the tools.
Those files include everything under `xtask/src`, the manifests and lockfiles, and each file the library compiles in with `include_str!` or `#[path]`.
Both the build script and the library compile `xtask/src/build_inputs.rs`, so the embedded hash and a checkout hash always agree about which files count.

`pr-workflow`, `coderabbit`, and `skill-ops`, including its stop hook, hash the same files in the checkout they can find before doing anything else.
They look at `DOTFILES_SOURCE`, then the dotfiles checkout containing the working directory, then the checkout that built the tool.
A difference refuses with `tool.stale` and the next action `mise run install:<tool>`.
No checkout means the tool refuses nothing.

Each install task records the hash it installed from under `~/.local/state/dotfiles/installed`.
`just apply` and the global post-merge hook run the install task for each recorded installation whose sources changed, and never install a tool the owner skipped.

Every chezmoi command an installer runs comes from `Tool::chezmoi_in(checkout)`, and a test refuses any other construction, so nothing reads the default source.

A pure core in `xtask/src/stale_rules.rs` holds the decisions, and Kani harnesses with rejecting counterexamples check them in `just proofs`.

## Alternatives

The owner rejected comparing revisions instead of content because a merge that touches no build input would then force a rebuild, and a dirty checkout would pass.
The owner rejected hashing only the binary source file because the library compiles every binary source into each tool.

## Consequences

Editing any build input makes the installed tools refuse until the install task runs in that checkout.
Adding an `include_str!` of a new file requires listing it in `build_inputs::SHARED`, which a test enforces.
