import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-signing, 3 approval-boundaries, 4 checked-arguments, 5 ci-budget, 6 code-signing, 7 coderabbit, 8 coderabbit-review, 9 concise-source, 10 decision-naming, 11 delegation, 12 development-assurance, 13 domyjob, 14 doppler, 15 dotfiles, 16 executable-policy, 17 formal-assurance, 18 functional-architecture, 19 gcp, 20 github-repository, 21 ideal-first-development, 22 ideal-specification, 23 japanese-lexicon, 24 just, 25 lefthook, 26 macos-pkg, 27 mise, 28 monochrome-design, 29 multi-machine, 30 mutation-testing, 31 nas, 32 ocomment, 33 orbstack, 34 oss-readme, 35 portable-skills, 36 pull-request, 37 registry-publishing, 38 release-evidence, 39 release-workflow, 40 reliability, 41 renovate, 42 resource-coordination, 43 rust-invariants, 44 rust-release, 45 rust-tooling, 46 skill-operations, 47 source-bound-interface-design, 48 sslcom-esigner, 49 storage-scout, 50 synology-dsm, 51 systematic-fixes, 52 tailscale, 53 type-contracts, 54 verification-tools, 55 xtask. -/
def catalog : Deps
  | 17 => [18]
  | 46 => [35]
  | 53 => [16]
  | 54 => [53, 16]
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

theorem plan_implementation_assurance : plan catalog 56 [12, 17, 18, 54] = .ok [12, 16, 18, 17, 53, 54] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 56 [35, 46] = .ok [35, 46] := by decide +kernel

theorem plan_every_skill : plan catalog 56 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 17, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
