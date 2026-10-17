import Std

/-! Generated from the checked native Kani postcondition by `just gen`. -/
namespace Dotfiles.Elapsed

def nanosPerMilli : Nat := 1000000

def nativePostcondition (whole remainder actual : Nat) : Prop :=
  remainder < nanosPerMilli ∧ actual = whole + (if remainder = 0 then 0 else 1)

theorem ceiling_upper (whole remainder actual : Nat)
    (post : nativePostcondition whole remainder actual) :
    actual * nanosPerMilli ≥ whole * nanosPerMilli + remainder := by
  by_cases zero : remainder = 0 <;>
    simp [nativePostcondition, nanosPerMilli, zero] at post ⊢ <;> omega

theorem ceiling_minimal (whole remainder actual : Nat)
    (post : nativePostcondition whole remainder actual) :
    actual = 0 ∨ (actual - 1) * nanosPerMilli < whole * nanosPerMilli + remainder := by
  by_cases zero : remainder = 0 <;>
    simp [nativePostcondition, nanosPerMilli, zero] at post ⊢ <;> omega

end Dotfiles.Elapsed
