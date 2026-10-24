---
date: 2026-10-25
status: proposed
---

# Verify CI pin readers from the offered source

## Status

Proposed with the Rust reader binding in the local Renovate adapter.

## Context

Renovate reports a GitHub Actions toolchain input that references a step output as an invalid value.
The output can still select an exact Rust declaration that the same report successfully checks through its mise manager.
A matching dependency name alone establishes neither the output's producer nor the version that the script reads.
Unsupported mise tool records also need their successful counterpart bound to the actual committed declaration, including when a lock version exists.

## Decision

Read regular UTF-8 Git blobs from the exact offered revision and decode workflow or composite Action metadata with the pinned YAML parser.
Refuse symbolic links, non-file objects, and invalid UTF-8 rather than interpreting link targets or replacement characters as file contents.
Recognize the existing Bash composite reader and PowerShell controller reader through exact script profiles.
Require a unique producer immediately before each matching consumer, unique step IDs, and immutable references to the expected Actions.
Check every matching consumer across every job, including the independent controller verification job.
Changed scripts, execution modifiers, duplicate mapping keys, aliases, and tags refuse evidence.
More than one document, inputs exceeding 128 KiB, or nesting deeper than 64 collections also refuse evidence.
The limits bound the supported input protocol rather than weakening a claim about arbitrary YAML.
Check every raw character before tokenization so a NUL can't hide trailing data from the parser.
The pure core enforces the raw C0 restrictions common to YAML's `c-printable` and `nb-json` productions.
Tab, line feed, and carriage return stay valid, and other syntax remains the maintained parser's responsibility.
Kani quantifies every Rust Unicode scalar value for that production function, with reachable allowed and refused outcomes and assertion-specific rejecting controls.

The controller profile requires its existing identity guard, matching controller and target checkouts, and the exact clean-tree and non-reparse verification immediately before reading the pin.
The identity guard relates the controller input and reusable workflow to the caller commit.
The native checkout guard establishes the expected revisions and clean trees before the reader executes.
The two existing diagnostic-message variants preserve those checks.

Require one enabled, successful mise Rust counterpart with the expected datasource, package, declaration kind, and identical nonempty declared and checked versions.
Bind that version to the offered root `mise.toml` scalar and the actual line-oriented reader input.
ASCII Rust assignment lines and one unindented Rust assignment keep the supported `awk` and PowerShell interpretations explicit.
Other comments may contain Unicode.
The Bash profile permits its existing inline comments, while the PowerShell profile requires its numeric two-part or three-part version line without a comment.
A different TOML section or multiline string can't supply an extra assignment unnoticed.
Literal updates remain ordinary freshness candidates and keep their existing source attribution.

Composite `action.yml` and `action.yaml` files enter the same manifest gate as workflows.
Locked and unlocked unsupported mise tool records both require the existing scalar source binding.
HTTP tool records also require their existing JSONata counterpart for GitHub's `/releases/latest` URL and `.tag_name` selector.
The supported flat table contains only its version, download URL, checksum, size, OS filter, version-list URL, and JSON selector fields.
Other selectors, expressions, headers, or platform overrides need a separate reviewed binding.
Match the committed table version, repository, official registry, and checked release tag, allowing the producer's exact optional `v` prefix.
Compare the parsed HTTP declaration across the push's two commits so a version-only table change still touches the literal update.
Comments leave the parsed declaration unchanged.

Saved-report inspection accepts `--revision <commit>` to apply the same source binding without another network lookup.
Inspection validates supplied evidence without authenticating the saved report's producer or establishing a push's source attribution.

## Consequences

The supported readers pass without excluding their dependency records.
Other readers and structurally different workflows fail until a reviewed profile establishes their source binding.
The adapter trusts Git object retrieval, the pinned YAML and TOML libraries, Renovate's lookup meanings, and the existing pure freshness decision.
Runtime correspondence assumes GitHub's documented workspace, output, and execution semantics.
Earlier workflow steps must leave the runner, shell, Git executable, and Git configuration trustworthy.
Immutable upstream Actions must also be honest.
The Bash composite also assumes a caller checkout of the offered source.
Runner compromise by a preceding step remains outside the validator's contract.

The scalar freshness proofs continue to cover the unchanged production decision core.
Native real-repository tests, original workflow fixtures, literal-input negative controls, and the registered parser fuzz target cover the external parsing and execution boundary.
Their finite results establish regression evidence rather than a theorem about every shell program or hosted execution.
Reassess the profiles when the producer schema, supported scripts, checkout guards, parser version, or GitHub execution contract changes.

## Alternatives

Accepting a matching report record without reading source admits unrelated pins and changed readers.
Handwritten YAML scanning loses duplicate-key and scalar semantics.
Executing arbitrary workflow scripts during a push adds an unnecessary code-execution boundary.
Proving an isolated Boolean model would not establish the interpretation of Git blobs, YAML, TOML, shell scripts, or the hosted runner.

The field contract comes from Renovate [44.169.0 Rust Action extraction](https://github.com/renovatebot/renovate/blob/44.169.0/lib/modules/manager/github-actions/known-actions/rust-version.ts), its [known Action transformation](https://github.com/renovatebot/renovate/blob/44.169.0/lib/modules/manager/github-actions/known-actions/utils.ts), and its [Rust datasource](https://github.com/renovatebot/renovate/blob/44.169.0/lib/modules/datasource/rust-version/index.ts).
The reader fixtures adapt `P4suta/find-my-files` [commit 05ce0eaf](https://github.com/P4suta/find-my-files/tree/05ce0eaf073ec6b0f024438ec7ea112f5e5d50d9/.github) by dropping its prose comments.
The HTTP version-list inputs follow [mise 2026.10.11](https://github.com/jdx/mise/blob/v2026.10.11/src/backend/http.rs).
The raw character contract follows [YAML 1.2.2, section 5.1](https://yaml.org/spec/1.2.2/#51-character-set).
