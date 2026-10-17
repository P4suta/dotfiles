/-!
# Direct edges suffice for the setup step order

`crates/dotfiles/src/setup.rs` checks a run order against the direct prerequisites of each step alone.
This file proves that a sequence holding every direct prerequisite of its steps, each before its step, also holds every transitive prerequisite before its step.
-/

namespace Dotfiles.StepOrder

/-- The steps each step relies on directly, each step named by its index in the declaration. -/
abbrev Deps := Nat → List Nat

/-- `y` is a prerequisite of `x`, directly or through other prerequisites. -/
inductive Reaches (after : Deps) : Nat → Nat → Prop
  | direct {x y : Nat} : y ∈ after x → Reaches after x y
  | step {x y z : Nat} : y ∈ after x → Reaches after y z → Reaches after x z

/-- The sequence holds every direct prerequisite of each of its steps, and each one before its step. -/
def DirectlyRespected (after : Deps) (sequence : List Nat) : Prop :=
  ∀ x ∈ sequence, ∀ y ∈ after x, y ∈ sequence ∧ sequence.idxOf y < sequence.idxOf x

/-- A sequence of a declaration checks by evaluation. -/
instance (after : Deps) (sequence : List Nat) : Decidable (DirectlyRespected after sequence) := by
  unfold DirectlyRespected
  infer_instance

/-- The direct check suffices: every transitive prerequisite of a step lies in the sequence before it. -/
theorem direct_edges_suffice (after : Deps) (sequence : List Nat)
    (respected : DirectlyRespected after sequence) :
    ∀ x ∈ sequence, ∀ y, Reaches after x y →
      y ∈ sequence ∧ sequence.idxOf y < sequence.idxOf x := by
  intro x member y reaches
  induction reaches with
  | direct edge => exact respected _ member _ edge
  | step edge _ earlier =>
    obtain ⟨inner, before⟩ := respected _ member _ edge
    obtain ⟨found, sooner⟩ := earlier inner
    exact ⟨found, Nat.lt_trans sooner before⟩

end Dotfiles.StepOrder
