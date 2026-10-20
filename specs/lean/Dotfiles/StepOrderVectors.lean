import Dotfiles.StepOrder

/-!
The run order of each family that `platform::step::sequence` derives, which `just gen` writes and a Rust test holds to the Rust results.
Lean checks by evaluation that each order respects every direct prerequisite, so `direct_edges_suffice` gives every transitive one.
-/

namespace Dotfiles.StepOrder.Vectors

open Dotfiles.StepOrder

/-! The steps in declaration order: 0 mise, 1 runtime, 2 github, 3 scoop, 4 winget, 5 nix, 6 desktop, 7 tools, 8 wezterm, 9 herdr, 10 integrations, 11 python-stub, 12 keyboard, 13 sshd, 14 storage-scout, 15 fcitx5, 16 macos-defaults, 17 forge-keys, 18 claude, 19 ocaml, 20 haskell, 21 racket, 22 guard, 23 ocomment, 24 domyjob, 25 fleet, 26 retire-tmux, 27 upgrade, 28 chat, 29 adapters. -/

/-- Each unix step's prerequisites among the unix steps. -/
def unixAfter : Deps
  | 1 => [0]
  | 2 => [0, 1]
  | 5 => [1]
  | 6 => [5, 1]
  | 7 => [0, 5, 6, 1]
  | 10 => [7, 1]
  | 15 => [1]
  | 16 => [1]
  | 17 => [1]
  | 18 => [1]
  | 19 => [7, 1]
  | 20 => [7, 1]
  | 21 => [6, 1]
  | 22 => [1]
  | 23 => [7, 1]
  | 24 => [7, 1]
  | 25 => [7, 1]
  | 26 => [1]
  | 28 => [24, 10, 18, 1]
  | 29 => [0, 1, 2, 5, 6, 7, 10, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 28]
  | _ => []

/-- The unix launchers in run order. -/
def unixOrder : List Nat := [0, 1, 2, 5, 6, 7, 10, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 28, 29]

theorem unix_respected : DirectlyRespected unixAfter unixOrder := by decide +kernel

theorem unix_transitive : ∀ x ∈ unixOrder, ∀ y, Reaches unixAfter x y →
    y ∈ unixOrder ∧ unixOrder.idxOf y < unixOrder.idxOf x :=
  direct_edges_suffice _ _ unix_respected

/-- Each windows step's prerequisites among the windows steps. -/
def windowsAfter : Deps
  | 1 => [0]
  | 2 => [0, 1]
  | 3 => [1]
  | 4 => [1]
  | 7 => [0, 3, 1]
  | 8 => [1]
  | 9 => [1]
  | 10 => [7, 9, 1]
  | 11 => [1]
  | 12 => [1]
  | 13 => [1]
  | 14 => [1]
  | 18 => [1]
  | 23 => [7, 1]
  | 24 => [7, 1]
  | 28 => [24, 10, 18, 1]
  | 29 => [0, 1, 2, 3, 4, 7, 8, 9, 10, 11, 12, 13, 14, 18, 23, 24, 28]
  | _ => []

/-- The windows launchers in run order. -/
def windowsOrder : List Nat := [0, 1, 2, 3, 4, 7, 8, 9, 10, 11, 12, 13, 14, 18, 23, 24, 28, 29]

theorem windows_respected : DirectlyRespected windowsAfter windowsOrder := by decide +kernel

theorem windows_transitive : ∀ x ∈ windowsOrder, ∀ y, Reaches windowsAfter x y →
    y ∈ windowsOrder ∧ windowsOrder.idxOf y < windowsOrder.idxOf x :=
  direct_edges_suffice _ _ windows_respected

end Dotfiles.StepOrder.Vectors
