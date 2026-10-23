import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-signing, 3 approval-boundaries, 4 checked-arguments, 5 ci-budget, 6 code-signing, 7 coderabbit, 8 coderabbit-review, 9 concise-source, 10 delegation, 11 development-assurance, 12 domyjob, 13 doppler, 14 dotfiles, 15 executable-policy, 16 formal-assurance, 17 functional-architecture, 18 gcp, 19 github-repository, 20 ideal-first-development, 21 ideal-specification, 22 japanese-lexicon, 23 just, 24 lefthook, 25 macos-pkg, 26 mise, 27 multi-machine, 28 mutation-testing, 29 nas, 30 ocomment, 31 orbstack, 32 oss-readme, 33 portable-skills, 34 pull-request, 35 registry-publishing, 36 release-evidence, 37 release-workflow, 38 reliability, 39 renovate, 40 resource-coordination, 41 rust-invariants, 42 rust-release, 43 rust-tooling, 44 skill-operations, 45 sslcom-esigner, 46 storage-scout, 47 systematic-fixes, 48 tailscale, 49 type-contracts, 50 verification-tools, 51 xtask. -/
def catalog : Deps
  | 16 => [17]
  | 44 => [33]
  | 49 => [15]
  | 50 => [49, 15]
  | _ => []

/-- Two skills that rely on each other. -/
def cyclic : Deps
  | 0 => [1]
  | 1 => [0]
  | _ => []

/-- A skill that relies on a skill outside the catalog. -/
def unknown : Deps
  | 0 => [2]
  | _ => []

theorem plan_implementation_assurance : plan catalog 52 [11, 16, 17, 50] = .ok [11, 15, 17, 16, 49, 50] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 52 [33, 44] = .ok [33, 44] := by decide +kernel

theorem plan_every_skill : plan catalog 52 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17, 16, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
