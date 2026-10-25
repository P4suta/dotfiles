//! The full-domain native postconditions used by the elapsed and campaign Lean theorems.

use crate::obligations::{self, LeanRefinement, Obligation};
use anyhow::{Context, Result, bail, ensure};
use quote::ToTokens;
use std::{fs, path::Path};
use syn::{Item, ext::IdentExt};

pub const SOURCE: &str = "crates/rules/src/proof_budget_rules.rs";
pub const GENERATED: &str = "specs/lean/Dotfiles/Elapsed.lean";
pub const HARNESS: &str = "proofs::host_elapsed_rounds_up_without_overflow_or_understatement";
pub const CAMPAIGN_HARNESS: &str = "proofs::campaign_parts_match_checked_ceiling_arithmetic";
pub const ZERO_HARNESS: &str = "proofs::campaign_zero_inputs_have_no_budget";
pub const BRIDGE: &str =
    "xtask/tests/elapsed_refinement.rs::the_generated_postcondition_is_bound_to_the_native_proof";
pub const THEOREMS: [&str; 4] = [
    "Dotfiles.Elapsed.ceiling_upper",
    "Dotfiles.Elapsed.ceiling_minimal",
    "Dotfiles.Elapsed.campaign_budget_covers",
    "Dotfiles.Elapsed.campaign_budget_minimal",
];

const CAMPAIGN_FUNCTION: &str = r"
#[must_use]
pub fn campaign_seconds(harnesses: u32, parallelism: u16, overhead: u32) -> Option<u32> {
    if harnesses == 0 || parallelism == 0 {
        return None;
    }
    let divisor = u32::from(parallelism);
    let quotient = harnesses / divisor;
    let remainder = harnesses % divisor;
    campaign_parts(quotient, remainder != 0, overhead)
}
fn campaign_parts(quotient: u32, partial: bool, overhead: u32) -> Option<u32> {
    let waves = u64::from(quotient) + u64::from(partial);
    u32::try_from(waves * TIMEOUT_SECONDS + u64::from(overhead)).ok()
}
";

const POSTCONDITION: &str = r"
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(1)]
fn host_elapsed_rounds_up_without_overflow_or_understatement() {
    let whole_millis: u128 = kani::any();
    let remainder_nanos: u32 = kani::any();
    let elapsed = elapsed_millis(whole_millis, remainder_nanos);
    assert_eq!(elapsed.is_some(), remainder_nanos < NANOS_PER_MILLI
        && whole_millis <= u128::from(u64::MAX)
        && (remainder_nanos == 0 || whole_millis < u128::from(u64::MAX)));
    if let Some(actual) = elapsed {
        assert_eq!(u128::from(actual), whole_millis + u128::from(remainder_nanos != 0));
    }
    cover_elapsed_boundaries(whole_millis, remainder_nanos, elapsed);
}
#[cfg(kani)]
fn cover_elapsed_boundaries(whole_millis: u128, remainder_nanos: u32, elapsed: Option<u64>) {
    kani::cover!(whole_millis == 0 && remainder_nanos == 0 && elapsed == Some(0));
    kani::cover!(whole_millis == u128::from(u64::MAX)
        && remainder_nanos == NANOS_PER_MILLI - 1 && elapsed.is_none());
}

pub fn spec_campaign_seconds(harnesses: u32, parallelism: u16, overhead: u32) -> Option<u32> {
    let budget = campaign_seconds(harnesses, parallelism, overhead);
    if harnesses == 0 || parallelism == 0 {
        assert!(budget.is_none());
    } else {
        let divisor = u32::from(parallelism);
        let quotient = harnesses / divisor;
        let remainder = harnesses % divisor;
        let expected = spec_campaign_parts(quotient, remainder != 0, overhead);
        assert_eq!(budget, expected);
    }
    budget
}
#[cfg(kani)]
#[kani::proof]
fn campaign_zero_inputs_have_no_budget() {
    let harnesses: u32 = kani::any();
    let parallelism: u16 = kani::any();
    let overhead: u32 = kani::any();
    assert_eq!(campaign_seconds(0, parallelism, overhead), None);
    assert_eq!(campaign_seconds(harnesses, 0, overhead), None);
    kani::cover!(harnesses == 0 && parallelism == 0);
    kani::cover!(harnesses != 0 && parallelism != 0);
}
pub fn spec_campaign_parts(quotient: u32, partial: bool, overhead: u32) -> Option<u32> {
    let budget = campaign_parts(quotient, partial, overhead);
    let expected = (u64::from(quotient) + u64::from(partial)) * TIMEOUT_SECONDS + u64::from(overhead);
    assert_eq!(budget.is_some(), u32::try_from(expected).is_ok());
    if let Some(actual) = budget { assert_eq!(u64::from(actual), expected); }
    budget
}
#[cfg(kani)]
#[kani::proof]
fn campaign_parts_match_checked_ceiling_arithmetic() {
    let quotient: u32 = kani::any();
    let partial: bool = kani::any();
    let overhead: u32 = kani::any();
    let budget = spec_campaign_parts(quotient, partial, overhead);
    kani::cover!(budget.is_some());
    kani::cover!(budget == Some(0));
    kani::cover!(budget.is_none());
}
";

fn tokens(value: &impl ToTokens) -> String {
    value.to_token_stream().to_string()
}

fn scope(items: &[Item], inner: bool) -> Result<()> {
    for item in items {
        let name = match item {
            Item::Fn(item) => &item.sig.ident,
            Item::Const(item) if !inner => &item.ident,
            Item::Enum(item) if !inner => &item.ident,
            Item::Mod(item) if !inner && item.ident == "proofs" => continue,
            Item::Mod(item)
                if !inner
                    && item.ident == "tests"
                    && item.attrs.len() == 1
                    && tokens(&item.attrs[0]) == "# [cfg (test)]" =>
            {
                continue;
            }
            Item::Use(item) if inner && tokens(item) == "use super :: * ;" => continue,
            _ => bail!("elapsed refinement refuses a shadowing or unsupported scope item"),
        };
        let name = name.unraw();
        ensure!(
            ![
                "kani",
                "u128",
                "u64",
                "u32",
                "u16",
                "bool",
                "Some",
                "None",
                "Option",
                "assert_eq",
                "assert"
            ]
            .contains(&name.to_string().as_str()),
            "elapsed refinement identifier is shadowed: {name}"
        );
        ensure!(
            !inner
                || ![
                    "elapsed_millis",
                    "campaign_seconds",
                    "campaign_parts",
                    "TIMEOUT_SECONDS"
                ]
                .contains(&name.to_string().as_str()),
            "elapsed production call is shadowed"
        );
    }
    Ok(())
}

pub(crate) fn require_functions(items: &[Item], expected_source: &str) -> Result<()> {
    for expected in syn::parse_file(expected_source)?.items {
        let Item::Fn(expected) = expected else {
            bail!("native expected item must be a function")
        };
        let actual: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Item::Fn(function) if function.sig.ident == expected.sig.ident => Some(function),
                _ => None,
            })
            .collect();
        ensure!(
            actual.len() == 1,
            "native function is missing or duplicated: {}",
            expected.sig.ident
        );
        let mut actual = actual[0].clone();
        actual
            .attrs
            .retain(|attribute| !attribute.path().is_ident("doc"));
        ensure!(
            tokens(&actual) == tokens(&expected),
            "native postcondition, arithmetic, or coverage differs: {}",
            expected.sig.ident
        );
    }
    Ok(())
}

/// Translates the admitted native postconditions without changing their meaning.
/// The canonical campaign verifies every native harness and generated Lean theorem.
///
/// # Errors
/// Refuses altered assertions, assumptions, stubs, calls, constants, or name resolution.
pub fn translate(source: &str) -> Result<String> {
    let file = syn::parse_file(source)?;
    ensure!(
        file.attrs.is_empty(),
        "elapsed source has unexpected crate attributes"
    );
    scope(&file.items, false)?;
    require_functions(&file.items, CAMPAIGN_FUNCTION)?;
    let timeout: syn::ItemConst = syn::parse_str("pub const TIMEOUT_SECONDS: u64 = 60;")?;
    ensure!(
        file.items
            .iter()
            .filter(|item| match item {
                Item::Const(item) if item.ident == timeout.ident => {
                    let mut actual = item.clone();
                    actual
                        .attrs
                        .retain(|attribute| !attribute.path().is_ident("doc"));
                    tokens(&actual) == tokens(&timeout)
                }
                _ => false,
            })
            .count()
            == 1,
        "campaign timeout constant differs"
    );
    let matching: Vec<_> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Mod(module) if module.ident == "proofs" => Some(module),
            _ => None,
        })
        .collect();
    ensure!(
        matching.len() == 1,
        "elapsed proof module is missing or duplicated"
    );
    let module = matching[0];
    let attributes: Vec<_> = module
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("doc"))
        .collect();
    ensure!(
        attributes.len() == 1 && tokens(attributes[0]) == "# [cfg (any (test , kani))]",
        "elapsed proof module configuration differs"
    );
    let (_, items) = module
        .content
        .as_ref()
        .context("elapsed proof module must be inline")?;
    scope(items, true)?;
    ensure!(
        items
            .iter()
            .filter(|item| matches!(item, Item::Use(_)))
            .count()
            == 1,
        "elapsed proof imports differ"
    );
    require_functions(items, POSTCONDITION)?;
    let constant: syn::ItemConst = syn::parse_str("pub const NANOS_PER_MILLI: u32 = 1_000_000;")?;
    ensure!(file.items.iter().filter(|item| matches!(item, Item::Const(item) if item.ident == constant.ident && tokens(item) == tokens(&constant))).count() == 1, "elapsed nanosecond unit differs");
    let signature: syn::ItemFn = syn::parse_str(
        "#[must_use] pub fn elapsed_millis(whole_millis: u128, remainder_nanos: u32) -> Option<u64> {}",
    )?;
    ensure!(file.items.iter().filter(|item| matches!(item, Item::Fn(item) if tokens(&item.sig) == tokens(&signature.sig) && item.attrs.iter().map(tokens).eq(signature.attrs.iter().map(tokens)))).count() == 1, "elapsed production entrypoint differs");
    let syn::Expr::Lit(literal) = constant.expr.as_ref() else {
        bail!("elapsed unit must be literal")
    };
    let syn::Lit::Int(unit) = &literal.lit else {
        bail!("elapsed unit must be unsigned")
    };
    Ok(generated(unit.base10_parse::<u32>()?))
}

fn generated(nanos_per_milli: u32) -> String {
    format!(
        r"import Std

/-! Generated from the checked native Kani postcondition by `just gen`. -/
namespace Dotfiles.Elapsed

def nanosPerMilli : Nat := {nanos_per_milli}

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
"
    )
}

/// Holds every complete campaign and each mandatory CI partition to the same source as its generated claims.
///
/// # Errors
/// Refuses stale generated claims, a failed verifier, or source changes during verification.
pub fn with_source<T>(root: &Path, verify: impl FnOnce() -> Result<T>) -> Result<T> {
    let manifest = fs::read_to_string(root.join(obligations::MANIFEST))?;
    let registered = obligations::parse_manifest(&manifest)?;
    ensure!(
        registered
            .obligations
            .iter()
            .filter(|entry| matches!(entry, Obligation::Kani { source, .. } if source == SOURCE))
            .count()
            == 1,
        "elapsed native Kani obligation is missing or duplicated"
    );
    let theorems: Vec<_> = registered
        .obligations
        .iter()
        .filter_map(|entry| match entry {
            Obligation::Lean(package) => Some(package),
            _ => None,
        })
        .flat_map(|package| {
            package
                .theorems
                .iter()
                .map(move |theorem| (&package.package, theorem))
        })
        .filter(|(_, theorem)| {
            theorem.module == "Dotfiles.Elapsed" || THEOREMS.contains(&theorem.name.as_str())
        })
        .collect();
    ensure!(
        theorems.len() == THEOREMS.len()
            && THEOREMS.iter().all(|name| theorems
                .iter()
                .filter(|(package, theorem)| package.as_str() == "specs/lean"
                    && theorem.name == *name
                    && theorem.module == "Dotfiles.Elapsed"
                    && theorem.bridge == BRIDGE
                    && matches!(theorem.refinement, LeanRefinement::SourceTranslation))
                .count()
                == 1),
        "elapsed Lean obligations are missing, duplicated, or altered"
    );
    let source = fs::read_to_string(root.join(SOURCE))?;
    let generated = translate(&source)?;
    ensure!(
        fs::read_to_string(root.join(GENERATED))? == generated,
        "elapsed Lean claims are stale"
    );
    let result = verify()?;
    ensure!(
        fs::read_to_string(root.join(SOURCE))? == source
            && fs::read_to_string(root.join(GENERATED))? == generated
            && fs::read_to_string(root.join(obligations::MANIFEST))? == manifest,
        "elapsed source, claims, or obligations changed during verification"
    );
    Ok(result)
}
