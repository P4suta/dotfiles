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

def campaignTimeout : Nat := 60

def campaignWaves (h p : Nat) : Nat :=
  h / p + (if h % p = 0 then 0 else 1)

def campaignNativePostcondition (h p overhead actual : Nat) : Prop :=
  0 < h ∧ 0 < p ∧ actual = campaignWaves h p * campaignTimeout + overhead

private theorem campaign_waves_covers (h p : Nat) (positive : 0 < p) :
    h ≤ campaignWaves h p * p := by
  have decomposition : h / p * p + h % p = h := by
    simpa [Nat.mul_comm, Nat.add_comm] using Nat.mod_add_div h p
  have remainder := Nat.mod_lt h positive
  by_cases zero : h % p = 0
  · simp [campaignWaves, zero]
    omega
  · simp [campaignWaves, zero, Nat.add_mul]
    omega

private theorem campaign_waves_minimal (h p : Nat) (work : 0 < h) (positive : 0 < p) :
    (campaignWaves h p - 1) * p < h := by
  have decomposition : h / p * p + h % p = h := by
    simpa [Nat.mul_comm, Nat.add_comm] using Nat.mod_add_div h p
  by_cases zero : h % p = 0
  · have quotient_positive : 0 < h / p := by
      by_cases quotient_zero : h / p = 0
      · simp [quotient_zero, zero] at decomposition
        omega
      · exact Nat.pos_of_ne_zero quotient_zero
    have previous_wave : (h / p - 1) * p + p = h := by
      calc
        (h / p - 1) * p + p = (h / p - 1 + 1) * p := by simp [Nat.add_mul]
        _ = h / p * p := by rw [Nat.sub_add_cancel quotient_positive]
        _ = h := by omega
    simp [campaignWaves, zero]
    omega
  · have remainder_positive : 0 < h % p := Nat.pos_of_ne_zero zero
    simp [campaignWaves, zero]
    omega

theorem campaign_budget_covers (h p overhead actual : Nat)
    (post : campaignNativePostcondition h p overhead actual) :
    h ≤ ((actual - overhead) / campaignTimeout) * p := by
  rcases post with ⟨work, positive, rfl⟩
  simpa [campaignTimeout] using campaign_waves_covers h p positive

theorem campaign_budget_minimal (h p overhead actual : Nat)
    (post : campaignNativePostcondition h p overhead actual) :
    (((actual - overhead) / campaignTimeout) - 1) * p < h := by
  rcases post with ⟨work, positive, rfl⟩
  simpa [campaignTimeout] using campaign_waves_minimal h p work positive

end Dotfiles.Elapsed
