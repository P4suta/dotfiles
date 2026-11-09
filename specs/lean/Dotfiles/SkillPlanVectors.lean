import Dotfiles.SkillPlan

/-!
The skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.
Each theorem checks by evaluation that the Lean definition gives the same plan.
-/

namespace Dotfiles.SkillPlan.Vectors

open Dotfiles.SkillPlan

/-- The shared skills in sorted order: 0 adr, 1 android-platform-guidelines, 2 apple-notarization, 3 apple-platform-guidelines, 4 apple-signing, 5 approval-boundaries, 6 checked-arguments, 7 ci-budget, 8 code-signing, 9 coderabbit, 10 coderabbit-review, 11 concise-source, 12 decision-naming, 13 delegation, 14 development-assurance, 15 diagram-explanations, 16 domyjob, 17 doppler, 18 dotfiles, 19 editorial-assurance, 20 executable-policy, 21 formal-assurance, 22 functional-architecture, 23 gcp, 24 github-repository, 25 github-style-slides, 26 ideal-first-development, 27 ideal-specification, 28 japanese-lexicon, 29 just, 30 knowledge-to-skills, 31 lefthook, 32 liquid-glass-design, 33 macos-pkg, 34 macos-platform-guidelines, 35 mise, 36 monochrome-design, 37 multi-machine, 38 mutation-testing, 39 nas, 40 ocomment, 41 orbstack, 42 oss-readme, 43 portable-skills, 44 presentation-decks, 45 pull-request, 46 registry-publishing, 47 release-evidence, 48 release-please, 49 release-workflow, 50 reliability, 51 renovate, 52 resource-coordination, 53 rust-invariants, 54 rust-release, 55 rust-tooling, 56 screenshots, 57 skill-operations, 58 source-bound-interface-design, 59 sslcom-esigner, 60 storage-scout, 61 synology-dsm, 62 systematic-fixes, 63 tailscale, 64 type-contracts, 65 verification-tools, 66 xtask. -/
def catalog : Deps
  | 21 => [22]
  | 57 => [43]
  | 64 => [20]
  | 65 => [64, 20]
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

theorem plan_implementation_assurance : plan catalog 67 [14, 21, 22, 65] = .ok [14, 20, 22, 21, 64, 65] := by decide +kernel

theorem plan_skill_maintenance : plan catalog 67 [43, 57] = .ok [43, 57] := by decide +kernel

theorem plan_every_skill : plan catalog 67 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66] = .ok [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 21, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66] := by decide +kernel

theorem plan_cycle : plan cyclic 2 [0] = .error .cycle := by decide +kernel

theorem plan_unknown : plan unknown 1 [0] = .error .unknown := by decide +kernel

end Dotfiles.SkillPlan.Vectors
