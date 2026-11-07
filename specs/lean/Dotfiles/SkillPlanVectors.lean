import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 apple-notarization, 2 apple-platform-guidelines, 3 apple-signing, 4 approval-boundaries, 5 checked-arguments, 6 ci-budget, 7 code-signing, 8 coderabbit, 9 coderabbit-review, 10 concise-source, 11 decision-naming, 12 delegation, 13 development-assurance, 14 diagram-explanations, 15 domyjob, 16 doppler, 17 dotfiles, 18 editorial-assurance, 19 executable-policy, 20 formal-assurance, 21 functional-architecture, 22 gcp, 23 github-repository, 24 github-style-slides, 25 ideal-first-development, 26 ideal-specification, 27 japanese-lexicon, 28 just, 29 knowledge-to-skills, 30 lefthook, 31 macos-pkg, 32 macos-platform-guidelines, 33 mise, 34 monochrome-design, 35 multi-machine, 36 mutation-testing, 37 nas, 38 ocomment, 39 orbstack, 40 oss-readme, 41 portable-skills, 42 presentation-decks, 43 pull-request, 44 registry-publishing, 45 release-evidence, 46 release-please, 47 release-workflow, 48 reliability, 49 renovate, 50 resource-coordination, 51 rust-invariants, 52 rust-release, 53 rust-tooling, 54 screenshots, 55 skill-operations, 56 source-bound-interface-design, 57 sslcom-esigner, 58 storage-scout, 59 synology-dsm, 60 systematic-fixes, 61 tailscale, 62 type-contracts, 63 verification-tools, 64 xtask. -/
def catalog : Deps
  | 20 => [21]
  | 55 => [41]
  | 62 => [19]
  | 63 => [62, 19]
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

theorem plan_implementation_assurance : plan catalog 65 [13, 20, 21, 63] = .ok [13, 19, 21, 20, 62, 63] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 65 [41, 55] = .ok [41, 55] := by decide +kernel

theorem plan_every_skill : plan catalog 65 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 20, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
