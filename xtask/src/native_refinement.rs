//! Source-bound native proof composition at standard-library conversion boundaries.

use crate::elapsed_refinement;
use anyhow::{Context, Result, bail, ensure};
use quote::ToTokens;
use std::{fs, path::Path};
use syn::{Item, ext::IdentExt};

pub const SOURCE: &str = "crates/rules/src/pr_rules.rs";

const PROJECTION: &str = r"
#[must_use]
pub fn dependency_skip(
    login: &str,
    author_type: Option<&str>,
    is_bot: bool,
    mergeable: Option<&str>,
) -> Option<DependencySkip> {
    dependency_skip_bytes(
        login.as_bytes(),
        author_type == Some(BOT_AUTHOR_TYPE),
        is_bot,
        mergeable == Some(CONFLICTING_MERGEABILITY),
    )
}
";

const DECLARATIONS: &str = r#"
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DependencySkip {
    Bot,
    BaseConflict,
}
const APP_AUTHOR_PREFIX: &str = "app/";
const BOT_AUTHOR_SUFFIX: &str = "[bot]";
const BOT_AUTHOR_TYPE: &str = "Bot";
const CONFLICTING_MERGEABILITY: &str = "CONFLICTING";
"#;

const NATIVE: &str = r"
fn spec_dependency_skip_bytes(bytes: &[u8], author_bot: bool, is_bot: bool, conflicting: bool) {
    let bot = is_bot
        || author_bot
        || bytes.get(..APP_AUTHOR_PREFIX.len()) == Some(APP_AUTHOR_PREFIX.as_bytes())
        || bytes.get(bytes.len().saturating_sub(BOT_AUTHOR_SUFFIX.len())..)
            == Some(BOT_AUTHOR_SUFFIX.as_bytes());
    let skip = dependency_skip_bytes(bytes, author_bot, is_bot, conflicting);
    assert_eq!(skip == Some(DependencySkip::Bot), bot);
    assert_eq!(skip == Some(DependencySkip::BaseConflict), !bot && conflicting);
    assert_eq!(skip.is_none(), !bot && !conflicting);
}
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(17)]
fn dependency_skips_only_bots_and_prs_conflicting_with_their_base() {
    const LOGIN_BYTES: usize = 16;
    let bytes: [u8; LOGIN_BYTES] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= bytes.len());
    let login = &bytes[..length];
    let author_bot = kani::any();
    let is_bot = kani::any();
    let conflicting = kani::any();
    spec_dependency_skip_bytes(login, author_bot, is_bot, conflicting);
    let skip = dependency_skip_bytes(login, author_bot, is_bot, conflicting);
    kani::cover!(skip == Some(DependencySkip::Bot));
    kani::cover!(skip == Some(DependencySkip::BaseConflict));
    kani::cover!(skip.is_none());
}
";

fn tokens(value: &impl ToTokens) -> String {
    value.to_token_stream().to_string()
}

fn scope(items: &[Item], inner: bool) -> Result<()> {
    for item in items {
        let name = match item {
            Item::Fn(item) => &item.sig.ident,
            Item::Const(item) => &item.ident,
            Item::Enum(item) if !inner => &item.ident,
            Item::Struct(item) if !inner => &item.ident,
            Item::Impl(_) if !inner => continue,
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
            _ => bail!(
                "dependency projection refuses a shadowing or unsupported scope item: {}",
                tokens(item)
            ),
        };
        let name = name.unraw().to_string();
        ensure!(
            ![
                "core",
                "kani",
                "str",
                "bool",
                "u8",
                "usize",
                "Some",
                "None",
                "Option",
                "assert_eq"
            ]
            .contains(&name.as_str()),
            "dependency projection identifier is shadowed: {name}"
        );
        ensure!(
            !inner
                || ![
                    "dependency_skip",
                    "dependency_skip_bytes",
                    "APP_AUTHOR_PREFIX",
                    "BOT_AUTHOR_SUFFIX",
                    "BOT_AUTHOR_TYPE",
                    "CONFLICTING_MERGEABILITY"
                ]
                .contains(&name.as_str()),
            "dependency production binding is shadowed: {name}"
        );
    }
    Ok(())
}

/// Check the exact standard conversion and its native decision contract.
///
/// # Errors
/// Refuses changed projections, bindings, constants, proof domains, assumptions, or attributes.
pub fn projection(source: &str) -> Result<()> {
    let file = syn::parse_file(source)?;
    ensure!(file.attrs.is_empty(), "dependency source attributes differ");
    scope(&file.items, false)?;
    elapsed_refinement::require_functions(&file.items, PROJECTION)?;
    for expected in syn::parse_file(DECLARATIONS)?.items {
        ensure!(
            file.items
                .iter()
                .filter(|actual| tokens(*actual) == tokens(&expected))
                .count()
                == 1,
            "dependency classification declaration is missing, duplicated, or changed"
        );
    }
    let modules: Vec<_> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Mod(module) if module.ident == "proofs" => Some(module),
            _ => None,
        })
        .collect();
    ensure!(
        modules.len() == 1,
        "dependency native proof module is missing or duplicated"
    );
    let module = modules[0];
    let attributes: Vec<_> = module
        .attrs
        .iter()
        .filter(|attribute| !attribute.path().is_ident("doc"))
        .collect();
    ensure!(
        attributes.len() == 1 && tokens(attributes[0]) == "# [cfg (any (test , kani))]",
        "dependency proof module configuration differs"
    );
    let (_, items) = module
        .content
        .as_ref()
        .context("dependency proof module must be inline")?;
    scope(items, true)?;
    ensure!(
        items
            .iter()
            .filter(|item| matches!(item, Item::Use(_)))
            .count()
            == 1,
        "dependency proof imports differ"
    );
    elapsed_refinement::require_functions(items, NATIVE)
}

/// Keep every complete campaign and CI partition bound to both native refinements.
///
/// # Errors
/// Refuses a changed projection, stale generated model, failed verifier, or inputs changed during verification.
pub fn with_source<T>(root: &Path, verify: impl FnOnce() -> Result<T>) -> Result<T> {
    let source = fs::read_to_string(root.join(SOURCE))?;
    projection(&source)?;
    ensure!(
        crate::obligations::manifest(root)?.obligations.iter().filter(|entry| {
            matches!(entry, crate::obligations::Obligation::Kani { source, .. } if source == SOURCE)
        }).count() == 1,
        "dependency native obligation is missing or duplicated"
    );
    elapsed_refinement::with_source(root, || {
        let result = verify()?;
        ensure!(
            fs::read_to_string(root.join(SOURCE))? == source,
            "dependency source changed during verification"
        );
        Ok(result)
    })
}
