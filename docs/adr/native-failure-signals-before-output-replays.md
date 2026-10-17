---
date: 2026-10-12
status: proposed
---
# Classify native failures from delivery signals before output replays

## Context

A tool can successfully read a log containing an earlier gate refusal.
Recording that text as a new failure creates another incident when inspecting the original incident.
The native collector must distinguish delivery facts from delivered output.
The installed Codex 0.162.1 implementation sends raw output as the `Bash` post-tool response and omits its exit status from that response.
Its [hook response implementation](https://github.com/openai/codex/blob/7a078f6bb4ce0dc2a81408b334ce52979d92c603/codex-rs/core/src/tools/context.rs#L438-L447) and [hook tests](https://github.com/openai/codex/blob/7a078f6bb4ce0dc2a81408b334ce52979d92c603/codex-rs/core/tests/suite/hooks.rs#L5453-L5460) define that observed boundary.

## Decision

The [pure incident rule](../../crates/rules/src/incident_rules.rs) receives explicit failure, declared source, structured refusal, and delivery facts as values.
A declared source takes precedence for every source variant.
Otherwise an explicit failure records its gate refusal or failed delivery even when another field reports a zero exit.
A numeric zero exit without an explicit failure records no incident from output text.
The collector classifies the known Codex `PostToolUse` event with tool name `Bash` and a selected string `tool_response`, without a preceding root error, as output-only delivery.
Output-only delivery makes no assertion about command success and records no new incident solely from that text.
Unknown shapes keep structured-refusal detection.
Unknown declared sources remain errors.
The existing maintenance and expected-failure decisions remain in force.

Managed gates record their actual failure at the [structured refusal emitter](../../crates/platform/src/refusal.rs) before native observers receive their output.
The [real hook regression](../../crates/dotfiles/tests/hook_refusals.rs) requires that primary incident to survive both the original output delivery and a successful Git read of its saved log without new incidents.
The [adapter tests](../../crates/skill-ops/tests/skill_ops.rs) cover zero exits, conflicting failure signals, source declarations, and unrelated client shapes.
Kani checks all 72 states of the pure boundary and rejects false claims through the [proof inventory](../../policy/obligations.toml).

## Alternatives

Interpreting an exit-code header in output can't distinguish a current result from a saved log containing the same header.
Treating output-only delivery as successful execution would claim a fact absent from the client contract.
Collecting raw native inputs or adding transcript joins would add sensitive persisted data and a new correlation boundary.

## Consequences

Inspection of old output can't create another incident through the known delivery shapes.
Managed gate failures keep their primary record, and explicit native failures keep their source.
For commands outside managed gates, the collector needs an explicit failure signal through an output-only client response.
Other clients and future unrecognized shapes keep the existing conservative decision.
This refines native collection in [automatic rule observations](automatic-rule-failure-observations.md).
