//! The exact full-domain Kani postcondition used by the elapsed-time Lean theorems.

use crate::obligations::{self, LeanRefinement, Obligation};
use anyhow::{Context, Result, bail, ensure};
use quote::ToTokens;
use std::{fs, path::Path};
use syn::Item;

pub const SOURCE: &str = "crates/rules/src/proof_budget_rules.rs";
pub const GENERATED: &str = "specs/lean/Dotfiles/Elapsed.lean";
pub const HARNESS: &str = "proofs::host_elapsed_rounds_up_without_overflow_or_understatement";
pub const BRIDGE: &str =
    "xtask/tests/elapsed_refinement.rs::the_generated_postcondition_is_bound_to_the_native_proof";
pub const THEOREMS: [&str; 2] = [
    "Dotfiles.Elapsed.ceiling_upper",
    "Dotfiles.Elapsed.ceiling_minimal",
];

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
        ensure!(
            !["kani", "u128", "u64", "u32", "Some", "Option", "assert_eq"]
                .contains(&name.to_string().as_str()),
            "elapsed refinement identifier is shadowed: {name}"
        );
        ensure!(
            !inner || name != "elapsed_millis",
            "elapsed production call is shadowed"
        );
    }
    Ok(())
}

/// Translates the admitted native postcondition without changing its meaning.
/// The canonical campaign must also verify this exact Kani harness and both Lean theorems.
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
    for expected in syn::parse_file(POSTCONDITION)?.items {
        let Item::Fn(expected) = expected else {
            bail!("elapsed expected item must be a function")
        };
        let actual: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Item::Fn(function) if function.sig.ident == expected.sig.ident => Some(function),
                _ => None,
            })
            .collect();
        ensure!(
            actual.len() == 1 && tokens(actual[0]) == tokens(&expected),
            "elapsed native postcondition or coverage differs: {}",
            expected.sig.ident
        );
    }
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
    Ok(format!(
        r"import Std

/-! Generated from the checked native Kani postcondition by `just gen`. -/
namespace Dotfiles.Elapsed

def nanosPerMilli : Nat := {}

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
",
        unit.base10_parse::<u32>()?
    ))
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
