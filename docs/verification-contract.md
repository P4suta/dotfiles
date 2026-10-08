# Verification contract reconciliation

This contract maps the acceptance criteria of [dotfiles-mac issue 36](https://github.com/P4suta/dotfiles-mac/issues/36), [dotfiles-private issue 59](https://github.com/P4suta/dotfiles-private/issues/59), and [dotfiles-win issue 36](https://github.com/P4suta/dotfiles-win/issues/36).
Each row names the enforcement boundary and regression evidence.
`Enforced` identifies checks in the implementation.
Passing verification receipts must identify the exact revision or installed profile.
Local commands and required CI use the same repository gates.

| Row | Criterion | Status | Enforcement and evidence |
| --- | --- | --- | --- |
| A1 | Required validation and child failure propagation | Enforced | `dotfiles::hooks::gate` failure propagation and `push.evidence` for missing freshness inputs or invalid reports |
| A2 | Common local and hosted gates with pinned tools | Enforced | `xtask::change` gate selection, mandatory launcher pins, and `workflow_findings` coverage checks |
| A3 | Actual remote default branch for new branches | Enforced | `default_branch_tip` through `ls-remote --symref` and real Git tests for changed or unavailable remote `HEAD` |
| A4 | Added lines bound to their own manifest | Enforced | Per-manifest change lines and `a_dependency_never_borrows_change_lines_from_another_manifest` |
| A5 | Breaking Cargo 0.x updates distinct from compatible updates | Enforced | Required Renovate `isBreaking` data and pure `freshness_rules` decisions |
| A6 | Freshness evidence for each offered commit | Enforced | Offered commit hashes, isolated indexes, and native tests for staged, working, and untracked content |
| A7 | Source proofs and native boundary regressions | Enforced | `policy/obligations.toml`, `just proofs`, and native Git, hook, collector, and profile tests |
| A8 | Scoped installation with owner state preservation | Enforced | No forced overwrite, merged host keys, and mandatory catalog integrity checks |
| A9 | Passing checks and complete review evidence for the final head | Enforced | `pr-workflow` refusal of failed checks and owed, stale, unavailable, or incomplete review evidence |
| A10 | Verification separate from release and signing operations | Enforced | Checks and rehearsals in CI with separate release boundaries |
| B1 | Global hooks without per-repository setup | Enforced | Managed `core.hooksPath`, the shared dispatcher, and dotguard's refusal of hook overrides |
| B2 | Missing, suppressed, or unbound CI results | Enforced | Only `success` in the required aggregator and refusal of `continue-on-error` |
| B3 | Missing tools, invalid contracts, stale evidence, and open maintenance findings | Enforced | Closed freshness errors, `freshness::require_current`, obligation closure, and `skill-ops stop` |
| B4 | Shared catalog and discovery on every supported native family | Enforced | Required matrix checks for skills, links, resources, decisions, placement, and adapters |
| B5 | Core proofs and process and file regression tests | Enforced | The A7 gates and native tests |
| B6 | History, credentials, approvals, and unrelated settings during profile updates | Enforced | Seeded preservation fixtures across repeated real chezmoi runs and native rehearsals |
| B7 | Combined verification and publication | Superseded | Checked PR workflow with publication under current owner authorization |
| B8 | Verification separate from tags, publication, releases, signing, and approvals | Enforced | Check commands without publication and dotguard protection for tags and history |
| C1 | Native Windows paths and reviewed policy | Enforced | Windows render, chezmoi apply, syntax checks, and both manifest families |
| C2 | Effective managed hooks and installation checks | Enforced | `just verify` managed file comparison and `doctor` checks of effective Git configuration on every host |
| C3 | Staged commit content and offered push revisions | Enforced | Exported index snapshots, staged ShellCheck regressions, and freshness over the offered tree |
| C4 | Actual successful required CI results | Enforced | Aggregator tests that reject failed, cancelled, skipped, or missing required results |
| C5 | Windows source checks, strict lint, formatting, and tests | Enforced | Selected `just check` gates in the required Windows context |
| C6 | Scoped native rollout with unrelated state preservation | Enforced | The A8 and B6 gates and transactions that refuse conflicting managed host edits |

The subject and rejecting probe counts increase with added coverage.
Proof, Miri, mutation, lint, and exception budgets keep their existing limits.
Text rule tests run natively.
Miri and Kani still verify the bounded decision specifications.
