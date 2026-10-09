import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-signing, 3 approval-boundaries, 4 ci-budget, 5 code-signing, 6 coderabbit, 7 coderabbit-review, 8 concise-source, 9 delegation, 10 development-assurance, 11 domyjob, 12 doppler, 13 dotfiles, 14 executable-policy, 15 formal-assurance, 16 functional-architecture, 17 gcp, 18 github-repository, 19 ideal-first-development, 20 ideal-specification, 21 just, 22 lefthook, 23 macos-pkg, 24 mise, 25 multi-machine, 26 mutation-testing, 27 nas, 28 ocomment, 29 orbstack, 30 oss-readme, 31 portable-skills, 32 pull-request, 33 registry-publishing, 34 release-evidence, 35 release-workflow, 36 reliability, 37 renovate, 38 resource-coordination, 39 rust-invariants, 40 rust-release, 41 rust-tooling, 42 skill-operations, 43 sslcom-esigner, 44 storage-scout, 45 systematic-fixes, 46 tailscale, 47 type-contracts, 48 verification-tools, 49 xtask. -/
def catalog : Deps
  | 15 => [16]
  | 42 => [31]
  | 47 => [14]
  | 48 => [47, 14]
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

theorem plan_implementation_assurance : plan catalog 50 [10, 15, 16, 48] = .ok [10, 14, 16, 15, 47, 48] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 50 [31, 42] = .ok [31, 42] := by decide +kernel

theorem plan_every_skill : plan catalog 50 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
