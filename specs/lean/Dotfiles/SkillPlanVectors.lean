import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 android-platform-guidelines, 2 app-store-connect, 3 apple-notarization, 4 apple-platform-guidelines, 5 apple-signing, 6 approval-boundaries, 7 checked-arguments, 8 ci-budget, 9 code-signing, 10 coderabbit, 11 coderabbit-review, 12 concise-source, 13 decision-naming, 14 delegation, 15 development-assurance, 16 diagram-explanations, 17 domyjob, 18 doppler, 19 dotfiles, 20 editorial-assurance, 21 executable-policy, 22 formal-assurance, 23 functional-architecture, 24 gcp, 25 github-repository, 26 github-style-slides, 27 ideal-first-development, 28 ideal-specification, 29 japanese-lexicon, 30 just, 31 knowledge-to-skills, 32 lefthook, 33 liquid-glass-design, 34 macos-pkg, 35 macos-platform-guidelines, 36 mise, 37 monochrome-design, 38 multi-machine, 39 mutation-testing, 40 nas, 41 ocomment, 42 orbstack, 43 oss-readme, 44 portable-skills, 45 presentation-decks, 46 pull-request, 47 registry-publishing, 48 release-evidence, 49 release-please, 50 release-workflow, 51 reliability, 52 renovate, 53 resource-coordination, 54 rust-invariants, 55 rust-release, 56 rust-tooling, 57 screenshots, 58 skill-operations, 59 source-bound-interface-design, 60 sslcom-esigner, 61 storage-scout, 62 synology-dsm, 63 systematic-fixes, 64 tailscale, 65 type-contracts, 66 verification-tools, 67 xtask. -/
def catalog : Deps
  | 22 => [23]
  | 58 => [44]
  | 65 => [21]
  | 66 => [65, 21]
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

theorem plan_implementation_assurance : plan catalog 68 [15, 22, 23, 66] = .ok [15, 21, 23, 22, 65, 66] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 68 [44, 58] = .ok [44, 58] := by decide +kernel

theorem plan_every_skill : plan catalog 68 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 23, 22, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
