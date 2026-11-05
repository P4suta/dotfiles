import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-platform-guidelines, 3 apple-signing, 4 approval-boundaries, 5 checked-arguments, 6 ci-budget, 7 code-signing, 8 coderabbit, 9 coderabbit-review, 10 concise-source, 11 decision-naming, 12 delegation, 13 development-assurance, 14 domyjob, 15 doppler, 16 dotfiles, 17 editorial-assurance, 18 executable-policy, 19 formal-assurance, 20 functional-architecture, 21 gcp, 22 github-repository, 23 ideal-first-development, 24 ideal-specification, 25 japanese-lexicon, 26 just, 27 lefthook, 28 macos-pkg, 29 macos-platform-guidelines, 30 mise, 31 monochrome-design, 32 multi-machine, 33 mutation-testing, 34 nas, 35 ocomment, 36 orbstack, 37 oss-readme, 38 portable-skills, 39 pull-request, 40 registry-publishing, 41 release-evidence, 42 release-please, 43 release-workflow, 44 reliability, 45 renovate, 46 resource-coordination, 47 rust-invariants, 48 rust-release, 49 rust-tooling, 50 screenshots, 51 skill-operations, 52 source-bound-interface-design, 53 sslcom-esigner, 54 storage-scout, 55 synology-dsm, 56 systematic-fixes, 57 tailscale, 58 type-contracts, 59 verification-tools, 60 xtask. -/
def catalog : Deps
  | 19 => [20]
  | 51 => [38]
  | 58 => [18]
  | 59 => [58, 18]
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

theorem plan_implementation_assurance : plan catalog 61 [13, 19, 20, 59] = .ok [13, 18, 20, 19, 58, 59] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 61 [38, 51] = .ok [38, 51] := by decide +kernel

theorem plan_every_skill : plan catalog 61 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 19, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
