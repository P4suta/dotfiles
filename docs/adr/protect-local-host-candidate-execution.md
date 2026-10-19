---
date: 2026-10-13
status: proposed
---
# Protect local host candidate execution

## Context

The host controller captures a committed bundle for remote workers but previously ran its own installed executable against the live local checkout.
A local edit could therefore change intermediate Source reads without changing the candidate named by the resulting receipt.
Checking the live tree before and after execution cannot establish which files a compiler or verifier consumed between those checks.

## Decision

The [local capture adapter](../../crates/dotfiles/src/hosts/local.rs) clones the accepted bundle into a separate owned scope and validates the commit, tree, gate, and both stage input identities.
It isolates Git configuration during capture and refuses links or special filesystem nodes.
Tracked Source files and their ancestors lose their write permissions on Unix, and Windows applies native owner SID deny permissions to data writes, metadata writes, deletion, and child replacement.
Before protection, the adapter creates the known `target` and fuzz artifact output directories only after Git confirms that their prefixes contain no tracked material.
These owned ignored outputs remain writable while protected ancestors prevent replacing the tracked Source namespace.
Actual file-open and rename controls must reject writes before the captured Source can be used.
A privileged execution that can still write the protected scope is refused.

The adapter builds the worker from that protected Source with build outputs and tool state outside the capture scope.
It copies the executable into the owned scope and protects that scope before starting Code or Real.
The worker validates its actual executable path and digest, embedded build root, revision and content hash, captured Git identities, stage inputs, and active write controls.
The controller accepts exactly the expected execution identity in the stage packet.
Real continues to receive the original native home.
The scope and protection guards remain owned until stage recording completes.

The [pending lease](../../crates/dotfiles/src/hosts/pending.rs) persists an unverified ordering barrier before local capture begins.
Interruption, capture failure, build failure, or missing provenance cannot revive earlier credit for a stage that this attempt must measure.
Previously verified stages selected for reuse retain their original generations.
Only verified recording followed by owned cleanup retires the pending execution.

The [stage input schema](../../crates/dotfiles/src/hosts.rs) changes so receipts from the earlier live-checkout producer become misses.
Local stage notes retain path-free commit, tree, build, and executable identities.
Existing append ordering, generation checks, origin validation, compare-and-swap recording, and remote bundle execution remain authoritative.
Remote workers also protect their captured Source throughout measured stages, retaining the same verified writable output namespaces.

## Alternatives

Live-root hash comparisons permit intermediate reads from changed files.
A captured writable directory permits ordinary tools to alter their inputs or replace their worker while the check runs.
An installed worker can originate from another Source even when its working directory holds the captured candidate.
A shared Cargo output path can change independently, so its executable is copied into the protected owned scope before execution.

## Consequences

Local execution adds a candidate build and an owned filesystem scope.
The existing host cache retains build outputs, while captured Source and private worker storage remain temporary.
Permission or cleanup failures are explicit errors with the owned scope identified in local diagnostics.
The [focused controls](../../crates/dotfiles/src/hosts/local.rs) cover live-edit isolation, actual filesystem refusals, provenance mismatch, capture mismatch, and interruption ordering.

The supported ownership contract excludes deliberate permission overrides or replacement of the entire owned namespace by its privileged owner.
It also assumes the native filesystem enforces its permission and rename semantics and the original native home remains under its normal owner.
No passing packet supplies credit for an unmeasured Code or Real stage.
