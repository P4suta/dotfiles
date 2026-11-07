import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-platform-guidelines, 3 apple-signing, 4 approval-boundaries, 5 checked-arguments, 6 ci-budget, 7 code-signing, 8 coderabbit, 9 coderabbit-review, 10 concise-source, 11 decision-naming, 12 delegation, 13 development-assurance, 14 diagram-explanations, 15 domyjob, 16 doppler, 17 dotfiles, 18 editorial-assurance, 19 executable-policy, 20 formal-assurance, 21 functional-architecture, 22 gcp, 23 github-repository, 24 ideal-first-development, 25 ideal-specification, 26 japanese-lexicon, 27 just, 28 knowledge-to-skills, 29 lefthook, 30 macos-pkg, 31 macos-platform-guidelines, 32 mise, 33 monochrome-design, 34 multi-machine, 35 mutation-testing, 36 nas, 37 ocomment, 38 orbstack, 39 oss-readme, 40 portable-skills, 41 pull-request, 42 registry-publishing, 43 release-evidence, 44 release-please, 45 release-workflow, 46 reliability, 47 renovate, 48 resource-coordination, 49 rust-invariants, 50 rust-release, 51 rust-tooling, 52 screenshots, 53 skill-operations, 54 source-bound-interface-design, 55 sslcom-esigner, 56 storage-scout, 57 synology-dsm, 58 systematic-fixes, 59 tailscale, 60 type-contracts, 61 verification-tools, 62 xtask. -/
def catalog : Deps
  | 20 => [21]
  | 53 => [40]
  | 60 => [19]
  | 61 => [60, 19]
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

theorem plan_implementation_assurance : plan catalog 63 [13, 20, 21, 61] = .ok [13, 19, 21, 20, 60, 61] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 63 [40, 53] = .ok [40, 53] := by decide +kernel

theorem plan_every_skill : plan catalog 63 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 20, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
