---
status: proposed
date: 2026-10-21
---
# Attest initial local artifacts before forward issue work

## Context

An existing directory can contain useful source before its first Git commit or local issue.
Normal local selection must reject unregistered edits, so it cannot assign earlier issue chronology to those artifacts.
The [local provider](keep-local-issue-authority-separate-from-remote-publication.md) needs a separate transition that records the actual baseline and permits future work.
Remote adoption requires an existing source commit and a live destination, which an unborn local project lacks.

## Decision

Provide an explicit preview and import transition for an unused local project with an unborn branch and no refs or remotes.
Require a nonempty staged baseline, no unstaged or nonignored untracked artifacts, and explicit scopes covering every imported path.
Reject alternate indexes and index flags that hide working changes.
Bind the preview digest to the actual checkout, common Git directory, local state revision, branch, issue, scopes, paths, and index tree.
An import must match that digest before and after signing.

Use the existing signing identity to attest the baseline as a detached signed Git evidence object without parents or a ref update.
Keep the baseline and raw signed evidence in owner-local issue history outside the project.
Verify its payload and tree against trusted owner signers outside the project whenever the active record is read.
Resolve those signers from the owner's global Git configuration under the bound home and ignore project-local allowlist overrides.
This evidence describes existing artifacts and grants future scoped issue authority.
It supplies no claim that the issue preceded the artifacts and no authority to publish.

Permit scoped repairs after import and before the first real source commit.
Preserve the original signed baseline independently of active scope replacement while the normal local provider observes that commit and enforces its ancestry afterward.
Keep ordinary late selection and remote adoption unchanged.
The executable boundary fixtures and pure admission rule are tracked in [issue 278](https://github.com/P4suta/dotfiles/issues/278).

## Alternatives

Weakening ordinary selection would let late issue creation erase the distinction between earlier artifacts and authorized edits.
Creating an artificial source commit would alter project history before its intended first commit.
Reusing remote adoption would require invented destination or source identities.
Freezing the imported tree until the first commit would prevent the repairs that the project's required commit checks demand.

## Consequences

Existing local projects can enter the supported issue workflow while preserving their actual chronology.
Import requires the existing signing identity and trusted owner signers, even though normal local preparation stays offline and unsigned.
Git, its index observations, the filesystem, and owner-managed trust configuration remain external boundaries covered by integration tests.
Required project verification and later remote promotion retain their separate gates.
