---
status: accepted
date: 2026-11-03
---
# Keep the previous build running after a failed reinstall

## Context

The [stale-source decision](0022-refuse-tools-built-from-stale-sources.md) makes every installed tool refuse with `tool.stale` once the files that build it change.
The post-commit and post-merge hooks run the install task of each lagging tool after the checkout has already moved.
When one of those install tasks fails, every tool built from that checkout refuses until a rebuild succeeds or the checkout moves back, as [issue 544](https://github.com/P4suta/dotfiles/issues/544) records.
On 2026-11-03, a refused skill catalog check left `pr-workflow` refusing on the whole machine until `git reset --keep` moved the source back.

## Decision

A failed install task records the failure under `~/.local/state/dotfiles/deferred`.
The record binds the canonical checkout, the content hash that the failed reinstall saw, and the build hash of the installation it left in place.
A stale tool keeps running, with a warning that names the failed install task, while the record matches the checkout it compares with, that checkout's current content, and its own build.
Another checkout, any later source change, or another build refuses with `tool.stale` again, so the next change retries the reinstall.
A successful installation removes the record.

The reinstall attempts every lagging tool even after a failure and then reports each failed install task together.
A stale tool inside its own working copy still builds that checkout first, as before.
The [stale rules](../../crates/rules/src/stale_rules.rs) decide the toleration, and Kani harnesses with rejecting probes check it.

## Alternatives

Building every lagging tool from the incoming tree before the checkout moves would avoid the stale span, but `git pull` offers no hook between fetching and moving the checkout.
Keeping the refusal leaves every tool unusable machine-wide after one unrelated failure, which the incident showed.

## Consequences

For the span between a failed reinstall and the next source change or successful installation, a tool can enforce the rules of its previous build.
That span is the trade-off this decision accepts, and the warning on each run makes it visible.
