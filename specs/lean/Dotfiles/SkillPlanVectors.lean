import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-signing, 3 approval-boundaries, 4 checked-arguments, 5 ci-budget, 6 code-signing, 7 coderabbit, 8 coderabbit-review, 9 concise-source, 10 decision-naming, 11 delegation, 12 development-assurance, 13 domyjob, 14 doppler, 15 dotfiles, 16 executable-policy, 17 formal-assurance, 18 functional-architecture, 19 gcp, 20 github-repository, 21 ideal-first-development, 22 ideal-specification, 23 japanese-lexicon, 24 just, 25 lefthook, 26 macos-pkg, 27 mise, 28 monochrome-design, 29 multi-machine, 30 mutation-testing, 31 nas, 32 ocomment, 33 orbstack, 34 oss-readme, 35 portable-skills, 36 pull-request, 37 registry-publishing, 38 release-evidence, 39 release-workflow, 40 reliability, 41 renovate, 42 resource-coordination, 43 rust-invariants, 44 rust-release, 45 rust-tooling, 46 screenshots, 47 skill-operations, 48 source-bound-interface-design, 49 sslcom-esigner, 50 storage-scout, 51 synology-dsm, 52 systematic-fixes, 53 tailscale, 54 type-contracts, 55 verification-tools, 56 xtask. -/
def catalog : Deps
  | 17 => [18]
  | 47 => [35]
  | 54 => [16]
  | 55 => [54, 16]
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

theorem plan_implementation_assurance : plan catalog 57 [12, 17, 18, 55] = .ok [12, 16, 18, 17, 54, 55] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 57 [35, 47] = .ok [35, 47] := by decide +kernel

theorem plan_every_skill : plan catalog 57 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 17, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
