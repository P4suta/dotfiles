---
date: 2026-10-25
status: proposed
---
# Hold mutation campaign temporary ownership

## Status

Proposed.

## Context

The mutation runner stages writable source before invoking cargo-mutants.
[Cargo-mutants 27.1 creates worker copies in the child process's temporary directory](https://github.com/sourcefrog/cargo-mutants/blob/v27.1.0/src/copy_tree.rs).
Protecting only the source directory leaves workers outside its ownership boundary.
Cargo's build lock doesn't establish ownership for the whole campaign.

## Decision

Declare one scratch owner for the parent of separate source and child temporary directories.
Hold its exclusive `owner.lock` before copying source and until every synchronous mutation command finishes.
Publish the existing owner marker format for cleanup consumers.
Set child temporary-directory variables on each command without changing the parent's environment.
Keep the child temporary root outside the copied source to prevent recursive copying.

[Rust's file lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.lock) supplies the operating-system boundary.
The owned file handle keeps the lease across build and test.
Drop the handle before removing the campaign tree so Windows can remove the lock file.
Keep mutation workers, the shared jobserver, test threads, timeouts, assertions, and retry behavior unchanged.

## Consequences

A cleanup consumer that honors ancestor ownership can reject removal throughout the campaign.
The contract depends on the operating system's file-lock semantics and the mutation runner waiting for its workers.
Kernel lock tests and a bounded real-worker fixture check those external boundaries.
Ownership evidence doesn't identify an earlier competing lock holder or establish the cause of a historical timeout.

Keeping only the source lease would leave sibling workers unprotected.
Pausing cleanup would leave coordination dependent on service state.
Increasing timeouts or reducing workers would change verification behavior without correcting ownership.
