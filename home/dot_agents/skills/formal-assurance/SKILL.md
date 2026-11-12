---
name: formal-assurance
description: >-
  Use source-bound formal methods when project requirements or the actual correctness question need them.
  Use for required proofs, model checking, verifier limits, and formal correctness claims.
metadata:
  tools: "tools.toml"
---

# Formal assurance

Run the project's maintained proof checks when the changed behavior or release stage requires them.
Choose formal methods when they answer a concrete correctness question.
Don't impose per-task proof inventories, evidence JSON, signed declarations, or separate completion dossiers.
Ordinary factual prose needs source checks, not an artificial theorem.

Identify the property, actual production entry point, input domain, and assumptions before selecting a verifier.
Prefer a verifier for the implementation language.
Kani checks executable Rust under its supported semantics and harness assumptions.
Lean 4 checks a formal model, so justify any claimed correspondence with production code.
Read [verifier-specific checks](references/verifiers.md) for the applicable verifier.
Use [scoping and alternatives](references/scoping.md) when a property crosses verifier semantics.

A proof establishes its stated theorem under its assumptions.
It doesn't establish empirical premises, natural-language interpretation, or decision relevance.
State bounded coverage accurately and keep semantic and integration checks for external boundaries.
Never replace a required proof with a detached model, a passing compile, or a coverage percentage.
Repair counterexamples and failed checks instead of weakening the property to get a pass.

Use pinned tools and project-owned commands so contributors can reproduce the verdict.
Bound verification by input sizes, unwinding, run counts, and seeds.
A timeout, unsupported target, skipped harness, or solver failure is inconclusive.
Resolve a required inconclusive check or ask the user for the concrete decision needed before claiming completion.
Confirm successful verification with the actual command and result.

On owner machines, tools listed in `~/.config/dotfiles/bounded-tools.toml` run only in containers with the configured resource limits.
Use the project's bounded entrypoint or `dotfiles bounded run --image <name@sha256:digest> --project . -- <project command>`.
GitHub Actions keeps its documented native execution.
Use `resource-coordination` before an expensive campaign.
Verification requirements grant no unbounded resource use.
Keep heavy campaigns at the stage defined by the project and `ci-budget`.

`assure`, `pr-workflow`, `domyjob`, `dotfiles hosts check`, and `storage-scout` remain optional owner-side conveniences.
They never replace a project's checks or require private configuration in a project.
