use anyhow::{Context, Result, ensure};
use quote::ToTokens as _;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const SOURCE: &str = "crates/dotfiles/src/verification/source.rs";
pub const ADAPTER_SELECTION: &str = "(literal_string_array|configuration_body|configuration_change|evidence_path|conditional_verification|module_source_path|module_directory|package_root|package_target|manifest_target_paths|ModuleDeclarations|verification_module_context|production_id|Plan::extends|verification/proof.rs|retain_native|validate_native|reports.rs.*kani|(?:replace | in )verification(?: ->|$))";

fn compact(text: &str) -> String {
    text.split_whitespace().collect()
}

fn test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || matches!(
                attribute.meta.to_token_stream().to_string().as_str(),
                "cfg (test)" | "cfg (kani)" | "cfg (any (test , kani))" | "cfg (any (kani , test))"
            )
    })
}

fn function_bodies(source: &str) -> Result<BTreeMap<String, String>> {
    let mut functions = BTreeMap::new();
    for item in syn::parse_file(source)?.items {
        match item {
            syn::Item::Fn(function) if !test_only(&function.attrs) => {
                let name = function.sig.ident.to_string();
                ensure!(
                    functions
                        .insert(name, function.to_token_stream().to_string())
                        .is_none(),
                    "duplicate function name"
                );
            }
            syn::Item::Impl(implementation) if !test_only(&implementation.attrs) => {
                let ty = implementation.self_ty.to_token_stream();
                let owner = match &implementation.trait_ {
                    Some((_, interface, _)) => {
                        format!("<impl {} for {ty}>", interface.to_token_stream())
                    }
                    None => ty.to_string(),
                };
                for item in implementation.items {
                    if let syn::ImplItem::Fn(function) = item
                        && !test_only(&function.attrs)
                    {
                        let name = compact(&format!("{owner}::{}", function.sig.ident));
                        ensure!(
                            functions
                                .insert(name, function.to_token_stream().to_string())
                                .is_none(),
                            "duplicate method name"
                        );
                    }
                }
            }
            syn::Item::Mod(module) => ensure!(
                test_only(&module.attrs),
                "function inventory cannot resolve module {}",
                module.ident
            ),
            _ => {}
        }
    }
    Ok(functions)
}

pub fn changed_functions(before: &str, after: &str) -> Result<BTreeSet<String>> {
    let before = function_bodies(before)?;
    Ok(function_bodies(after)?
        .into_iter()
        .filter_map(|(name, body)| (before.get(&name) != Some(&body)).then_some(name))
        .collect())
}

pub fn validate_changed(before: &str, after: &str, inventory: &Value) -> Result<()> {
    let entries = inventory
        .as_array()
        .context("mutation inventory is not an array")?;
    let names: BTreeSet<_> = entries
        .iter()
        .filter(|entry| entry["file"] == SOURCE)
        .filter_map(|entry| entry["function"]["function_name"].as_str())
        .map(compact)
        .collect();
    for function in changed_functions(before, after)? {
        ensure!(
            names.contains(&function),
            "mutation inventory omits changed function {function}"
        );
    }
    Ok(())
}

pub fn module_methods(source: &str) -> Result<BTreeSet<String>> {
    let mut methods = BTreeSet::new();
    for item in syn::parse_file(source)?.items {
        let syn::Item::Impl(item) = item else {
            continue;
        };
        let syn::Type::Path(ty) = item.self_ty.as_ref() else {
            continue;
        };
        if ty
            .path
            .segments
            .last()
            .is_none_or(|segment| segment.ident != "ModuleDeclarations")
        {
            continue;
        }
        let (_, interface, _) = item.trait_.context("module visitor has no trait")?;
        let owner = format!(
            "<impl {} for {}>",
            interface.to_token_stream(),
            item.self_ty.to_token_stream()
        );
        for member in item.items {
            if let syn::ImplItem::Fn(method) = member {
                methods.insert(compact(&format!("{owner}::{}", method.sig.ident)));
            }
        }
    }
    ensure!(!methods.is_empty(), "module visitor methods are missing");
    Ok(methods)
}

pub fn validate(source: &str, inventory: &Value) -> Result<()> {
    let entries = inventory
        .as_array()
        .context("mutation inventory is not an array")?;
    let actual: BTreeSet<_> = entries
        .iter()
        .filter(|entry| entry["file"] == SOURCE)
        .filter_map(|entry| entry["function"]["function_name"].as_str())
        .map(compact)
        .collect();
    for method in module_methods(source)? {
        ensure!(
            actual.contains(&method),
            "mutation inventory omits {method}"
        );
    }
    Ok(())
}

fn identities(inventory: &Value) -> Result<BTreeMap<(&str, &str), &Value>> {
    let entries = inventory
        .as_array()
        .context("mutation inventory is not an array")?;
    ensure!(!entries.is_empty(), "empty mutation inventory");
    let mut names = BTreeMap::new();
    for entry in entries {
        let file = entry["file"].as_str().context("mutation has no file")?;
        let name = entry["name"].as_str().context("mutation has no identity")?;
        ensure!(
            names.insert((file, name), entry).is_none(),
            "duplicate mutant {name}"
        );
    }
    Ok(names)
}

pub fn complete(expected: &Value, shards: &[Value]) -> Result<()> {
    let expected = identities(expected)?;
    let mut actual = BTreeMap::new();
    for shard in shards {
        for (identity, entry) in identities(shard)? {
            ensure!(
                actual.insert(identity, entry).is_none(),
                "mutant occurs in multiple shards"
            );
        }
    }
    ensure!(
        actual == expected,
        "mutation shards omit, alter, or add mutants"
    );
    Ok(())
}

pub fn collect_shards(
    count: usize,
    mut run: impl FnMut(usize) -> Result<Value>,
) -> Result<Vec<Value>> {
    ensure!(count > 0, "empty shard campaign");
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for index in 0..count {
        match run(index) {
            Ok(result) => results.push(result),
            Err(error) => failures.push(format!("shard {index}: {error:#}")),
        }
    }
    ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(results)
}
