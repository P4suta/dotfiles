import Dotfiles.StepOrder

/-!
The run order of each family that `platform::step::sequence` derives, which `just gen` writes and a Rust test holds to the Rust results.
Lean checks by evaluation that each order respects every direct prerequisite, so `direct_edges_suffice` gives every transitive one.
-/

namespace Dotfiles.StepOrder.Vectors

open Dotfiles.StepOrder

/-! The steps in declaration order: 0 mise, 1 runtime, 2 github, 3 scoop, 4 winget, 5 nix, 6 desktop, 7 tools, 8 herdr, 9 integrations, 10 python-stub, 11 keyboard, 12 sshd, 13 storage-scout, 14 fcitx5, 15 macos-defaults, 16 forge-keys, 17 claude, 18 ocaml, 19 haskell, 20 racket, 21 guard, 22 ocomment, 23 domyjob, 24 fleet, 25 retire-tmux, 26 upgrade, 27 chat, 28 adapters. -/

/-- Each unix step's prerequisites among the unix steps. -/
def unixAfter : Deps
  | 1 => [0]
  | 2 => [0, 1]
  | 5 => [1]
  | 6 => [5, 1]
  | 7 => [0, 5, 6, 1]
  | 9 => [7, 1]
  | 14 => [1]
  | 15 => [1]
  | 16 => [1]
  | 17 => [1]
  | 18 => [7, 1]
  | 19 => [7, 1]
  | 20 => [6, 1]
  | 21 => [1]
  | 22 => [7, 1]
  | 23 => [7, 1]
  | 24 => [7, 1]
  | 25 => [1]
  | 27 => [23, 9, 17, 1]
  | 28 => [0, 1, 2, 5, 6, 7, 9, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 27]
  | _ => []

/-- The unix launchers in run order. -/
def unixOrder : List Nat := [0, 1, 2, 5, 6, 7, 9, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 27, 28]

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
  | 9 => [7, 8, 1]
  | 10 => [1]
  | 11 => [1]
  | 12 => [1]
  | 13 => [1]
  | 17 => [1]
  | 22 => [7, 1]
  | 23 => [7, 1]
  | 27 => [23, 9, 17, 1]
  | 28 => [0, 1, 2, 3, 4, 7, 8, 9, 10, 11, 12, 13, 17, 22, 23, 27]
  | _ => []

/-- The windows launchers in run order. -/
def windowsOrder : List Nat := [0, 1, 2, 3, 4, 7, 8, 9, 10, 11, 12, 13, 17, 22, 23, 27, 28]

theorem windows_respected : DirectlyRespected windowsAfter windowsOrder := by decide +kernel

theorem windows_transitive : ∀ x ∈ windowsOrder, ∀ y, Reaches windowsAfter x y →
    y ∈ windowsOrder ∧ windowsOrder.idxOf y < windowsOrder.idxOf x :=
  direct_edges_suffice _ _ windows_respected

end Dotfiles.StepOrder.Vectors
