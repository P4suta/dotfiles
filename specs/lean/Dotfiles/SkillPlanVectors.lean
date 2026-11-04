import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-signing, 3 approval-boundaries, 4 checked-arguments, 5 ci-budget, 6 code-signing, 7 coderabbit, 8 coderabbit-review, 9 concise-source, 10 decision-naming, 11 delegation, 12 development-assurance, 13 domyjob, 14 doppler, 15 dotfiles, 16 editorial-assurance, 17 executable-policy, 18 formal-assurance, 19 functional-architecture, 20 gcp, 21 github-repository, 22 ideal-first-development, 23 ideal-specification, 24 japanese-lexicon, 25 just, 26 lefthook, 27 macos-pkg, 28 mise, 29 monochrome-design, 30 multi-machine, 31 mutation-testing, 32 nas, 33 ocomment, 34 orbstack, 35 oss-readme, 36 portable-skills, 37 pull-request, 38 registry-publishing, 39 release-evidence, 40 release-workflow, 41 reliability, 42 renovate, 43 resource-coordination, 44 rust-invariants, 45 rust-release, 46 rust-tooling, 47 screenshots, 48 skill-operations, 49 source-bound-interface-design, 50 sslcom-esigner, 51 storage-scout, 52 synology-dsm, 53 systematic-fixes, 54 tailscale, 55 type-contracts, 56 verification-tools, 57 xtask. -/
def catalog : Deps
  | 18 => [19]
  | 48 => [36]
  | 55 => [17]
  | 56 => [55, 17]
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

theorem plan_implementation_assurance : plan catalog 58 [12, 18, 19, 56] = .ok [12, 17, 19, 18, 55, 56] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 58 [36, 48] = .ok [36, 48] := by decide +kernel

theorem plan_every_skill : plan catalog 58 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 18, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
