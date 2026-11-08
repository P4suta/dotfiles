import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 android-platform-guidelines, 2 apple-notarization, 3 apple-platform-guidelines, 4 apple-signing, 5 approval-boundaries, 6 checked-arguments, 7 ci-budget, 8 code-signing, 9 coderabbit, 10 coderabbit-review, 11 concise-source, 12 decision-naming, 13 delegation, 14 development-assurance, 15 diagram-explanations, 16 domyjob, 17 doppler, 18 dotfiles, 19 editorial-assurance, 20 executable-policy, 21 formal-assurance, 22 functional-architecture, 23 gcp, 24 github-repository, 25 github-style-slides, 26 ideal-first-development, 27 ideal-specification, 28 japanese-lexicon, 29 just, 30 knowledge-to-skills, 31 lefthook, 32 macos-pkg, 33 macos-platform-guidelines, 34 mise, 35 monochrome-design, 36 multi-machine, 37 mutation-testing, 38 nas, 39 ocomment, 40 orbstack, 41 oss-readme, 42 portable-skills, 43 presentation-decks, 44 pull-request, 45 registry-publishing, 46 release-evidence, 47 release-please, 48 release-workflow, 49 reliability, 50 renovate, 51 resource-coordination, 52 rust-invariants, 53 rust-release, 54 rust-tooling, 55 screenshots, 56 skill-operations, 57 source-bound-interface-design, 58 sslcom-esigner, 59 storage-scout, 60 synology-dsm, 61 systematic-fixes, 62 tailscale, 63 type-contracts, 64 verification-tools, 65 xtask. -/
def catalog : Deps
  | 21 => [22]
  | 56 => [42]
  | 63 => [20]
  | 64 => [63, 20]
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

theorem plan_implementation_assurance : plan catalog 66 [14, 21, 22, 64] = .ok [14, 20, 22, 21, 63, 64] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 66 [42, 56] = .ok [42, 56] := by decide +kernel

theorem plan_every_skill : plan catalog 66 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 21, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
