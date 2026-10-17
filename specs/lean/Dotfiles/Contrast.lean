import Std
import Dotfiles.ContrastSource

/-!
The production contrast expressions translated by `xtask::contrast_refinement`.
Kani establishes unsigned arithmetic safety and the full RGB luminance bound.
These theorems establish the division bounds and the exact 4.5:1 decision over every admitted luminance.
-/

namespace Dotfiles.Contrast

open Source

theorem ordered_bounds (first second : Nat)
    (first_bound : first ≤ MAX_LUMINANCE) (second_bound : second ≤ MAX_LUMINANCE) :
    FLARE ≤ (ordered_luminances first second).2 ∧
    (ordered_luminances first second).2 ≤ (ordered_luminances first second).1 ∧
    (ordered_luminances first second).1 ≤ MAX_LUMINANCE + FLARE := by
  dsimp only [ordered_luminances]
  by_cases order : FLARE + first ≥ FLARE + second <;> simp [order] <;> omega

theorem division_bounds (lighter darker : Nat)
    (floor : FLARE ≤ darker) (ordered : darker ≤ lighter)
    (ceiling : lighter ≤ MAX_LUMINANCE + FLARE) :
    MILLIS ≤ ratio_ordered lighter darker ∧ ratio_ordered lighter darker ≤ MAX_RATIO := by
  unfold FLARE at floor
  unfold FLARE MAX_LUMINANCE MAX_LINEAR at ceiling
  have positive : 0 < darker := by omega
  simp only [ratio_ordered, MILLIS, MAX_RATIO, Nat.reduceMul]
  constructor
  ·
    rw [Nat.le_div_iff_mul_le positive]
    omega
  ·
    have upper : lighter * 1000 / darker < 21001 := by
      rw [Nat.div_lt_iff_lt_mul positive]
      omega
    omega

theorem division_threshold (lighter darker : Nat) (positive : 0 < darker) :
    (LEAST_NUMERATOR * MILLIS / LEAST_DENOMINATOR ≤ ratio_ordered lighter darker) ↔
    LEAST_NUMERATOR * darker ≤ LEAST_DENOMINATOR * lighter := by
  simp only [ratio_ordered, LEAST_NUMERATOR, LEAST_DENOMINATOR, MILLIS,
    Nat.reduceMul, Nat.reduceDiv]
  rw [Nat.le_div_iff_mul_le positive]
  omega

theorem reported_ratio (first second : Nat)
    (first_bound : first ≤ MAX_LUMINANCE) (second_bound : second ≤ MAX_LUMINANCE) :
    MILLIS ≤ ratio_luminances first second ∧ ratio_luminances first second ≤ MAX_RATIO ∧
    ((LEAST_NUMERATOR * MILLIS / LEAST_DENOMINATOR ≤ ratio_luminances first second) ↔
      keeps_least_contrast first second = true) := by
  have flare_positive : 0 < FLARE := by decide
  by_cases order : FLARE + first ≥ FLARE + second
  · have bounds := division_bounds (FLARE + first) (FLARE + second)
      (by omega) order (by omega)
    have threshold := division_threshold (FLARE + first) (FLARE + second) (by omega)
    simpa only [ratio_luminances, keeps_least_contrast, ordered_luminances,
      order, decide_true, Bool.true_eq, ite_true, decide_eq_true_eq] using
      And.intro bounds.1 (And.intro bounds.2 threshold)
  · have bounds := division_bounds (FLARE + second) (FLARE + first)
      (by omega) (by omega) (by omega)
    have threshold := division_threshold (FLARE + second) (FLARE + first) (by omega)
    simpa only [ratio_luminances, keeps_least_contrast, ordered_luminances,
      order, decide_false, Bool.false_eq_true, ite_false, decide_eq_true_eq] using
      And.intro bounds.1 (And.intro bounds.2 threshold)

end Dotfiles.Contrast
