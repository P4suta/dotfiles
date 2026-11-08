use anyhow::{Context, Result};
use quote::ToTokens as _;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use syn::visit_mut::VisitMut as _;

use crate::assure::Tree;

pub(super) fn digest(bytes: &[u8]) -> String {
    platform::build_inputs::hex(&Sha256::digest(bytes))
}

pub(super) fn relevant(path: &Path) -> bool {
    if path
        .components()
        .any(|part| part.as_os_str() == ".chezmoitemplates")
    {
        return true;
    }
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "rs" | "go"
                | "py"
                | "pyi"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "mts"
                | "cts"
                | "java"
                | "kt"
                | "kts"
                | "swift"
                | "c"
                | "h"
                | "cc"
                | "cpp"
                | "hpp"
                | "cs"
                | "rb"
                | "php"
                | "ex"
                | "exs"
                | "erl"
                | "hs"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "ps1"
                | "psm1"
                | "nu"
                | "toml"
                | "json"
                | "yaml"
                | "yml"
                | "qnt"
                | "lean"
                | "tmpl"
                | "lua"
                | "dart"
                | "zig"
                | "nix"
                | "sql"
                | "graphql"
                | "proto"
                | "tf"
                | "hcl"
                | "vue"
                | "svelte"
                | "html"
                | "css"
                | "scss"
                | "xml"
                | "plist"
                | "ini"
                | "cfg"
        )
    ) || matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some(
            "Cargo.lock"
                | "mise.lock"
                | "justfile"
                | "Makefile"
                | "Dockerfile"
                | "package-lock.json"
                | "uv.lock"
                | "bun.lock"
                | "pnpm-lock.yaml"
                | "yarn.lock"
                | "go.sum"
        )
    )
}

pub(super) fn configuration(path: &str) -> bool {
    let path = Path::new(path);
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("toml" | "json" | "yaml" | "yml" | "xml" | "plist" | "ini" | "cfg" | "lock")
    ) || matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("go.sum" | "justfile" | "Justfile" | ".justfile")
    )
}

fn literal_string_array(item: &syn::Item) -> bool {
    let syn::Item::Const(item) = item else {
        return false;
    };
    if !item.attrs.is_empty() {
        return false;
    }
    let syn::Type::Reference(outer) = &*item.ty else {
        return false;
    };
    let syn::Type::Slice(slice) = &*outer.elem else {
        return false;
    };
    let syn::Type::Reference(inner) = &*slice.elem else {
        return false;
    };
    let syn::Type::Path(value) = &*inner.elem else {
        return false;
    };
    let syn::Expr::Reference(reference) = &*item.expr else {
        return false;
    };
    let syn::Expr::Array(array) = &*reference.expr else {
        return false;
    };
    outer.mutability.is_none() && inner.mutability.is_none()
        && value.qself.is_none() && value.path.is_ident("str")
        && reference.mutability.is_none() && reference.attrs.is_empty() && array.attrs.is_empty()
        && array.elems.iter().all(|value| matches!(value, syn::Expr::Lit(literal) if literal.attrs.is_empty() && matches!(literal.lit, syn::Lit::Str(_))))
}

fn configuration_body(path: &str, bytes: Option<&[u8]>) -> Result<Vec<u8>> {
    let Some(bytes) = bytes else {
        return Ok(Vec::new());
    };
    let normalized = production(path, bytes)?;
    let mut file = syn::parse_file(std::str::from_utf8(&normalized)?)?;
    file.items.retain(|item| !literal_string_array(item));
    Ok(file.to_token_stream().to_string().into_bytes())
}

/// # Errors
/// Refuses invalid Rust rather than treating a failed parse as configuration.
pub fn configuration_change(
    path: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<bool> {
    use rules::verification_rules::{ConfigurationSource, configuration_admitted};
    if configuration(path) {
        return Ok(configuration_admitted(ConfigurationSource::Document, true));
    }
    if Path::new(path)
        .extension()
        .is_none_or(|value| value != "rs")
    {
        return Ok(configuration_admitted(
            ConfigurationSource::Executable,
            false,
        ));
    }
    Ok(configuration_admitted(
        ConfigurationSource::Rust,
        configuration_body(path, before)? == configuration_body(path, after)?,
    ))
}

fn test_path(path: &Path) -> bool {
    path.components().any(|part| {
        matches!(
            part.as_os_str().to_str(),
            Some("tests" | "test" | "fixtures" | "testdata" | "fuzz" | "benches")
        )
    }) || path
        .file_name()
        .and_then(|part| part.to_str())
        .is_some_and(|name| {
            name.contains(".test.")
                || name.contains(".spec.")
                || name.ends_with("_test.go")
                || name.starts_with("test_")
                || name.ends_with("_test.py")
        })
}

fn conditional_verification(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        matches!(
            attribute.meta.to_token_stream().to_string().as_str(),
            "cfg (test)" | "cfg (kani)" | "cfg (any (test , kani))" | "cfg (any (kani , test))"
        )
    })
}

fn verification(attributes: &[syn::Attribute]) -> bool {
    conditional_verification(attributes)
        || attributes.iter().any(|attribute| {
            matches!(&attribute.meta, syn::Meta::Path(path) if path.is_ident("test"))
                || attribute
                    .path()
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    == ["kani", "proof"]
        })
}

fn attributes(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Fn(value) => &value.attrs,
        syn::Item::Mod(value) => &value.attrs,
        syn::Item::Impl(value) => &value.attrs,
        syn::Item::Use(value) => &value.attrs,
        syn::Item::Const(value) => &value.attrs,
        syn::Item::Static(value) => &value.attrs,
        syn::Item::Struct(value) => &value.attrs,
        syn::Item::Enum(value) => &value.attrs,
        syn::Item::Type(value) => &value.attrs,
        syn::Item::Trait(value) => &value.attrs,
        syn::Item::Macro(value) => &value.attrs,
        _ => &[],
    }
}

struct Production;

impl syn::visit_mut::VisitMut for Production {
    fn visit_file_mut(&mut self, node: &mut syn::File) {
        node.attrs
            .retain(|attribute| !attribute.path().is_ident("doc"));
        node.items.retain(|item| !verification(attributes(item)));
        syn::visit_mut::visit_file_mut(self, node);
    }

    fn visit_item_mod_mut(&mut self, node: &mut syn::ItemMod) {
        if let Some((_, items)) = &mut node.content {
            items.retain(|item| !verification(attributes(item)));
        }
        syn::visit_mut::visit_item_mod_mut(self, node);
    }

    fn visit_item_mut(&mut self, node: &mut syn::Item) {
        let attributes = match node {
            syn::Item::Fn(value) => Some(&mut value.attrs),
            syn::Item::Mod(value) => Some(&mut value.attrs),
            syn::Item::Impl(value) => Some(&mut value.attrs),
            syn::Item::Use(value) => Some(&mut value.attrs),
            syn::Item::Const(value) => Some(&mut value.attrs),
            syn::Item::Static(value) => Some(&mut value.attrs),
            syn::Item::Struct(value) => Some(&mut value.attrs),
            syn::Item::Enum(value) => Some(&mut value.attrs),
            syn::Item::Type(value) => Some(&mut value.attrs),
            syn::Item::Trait(value) => Some(&mut value.attrs),
            _ => None,
        };
        if let Some(attributes) = attributes {
            attributes.retain(|attribute| !attribute.path().is_ident("doc"));
        }
        syn::visit_mut::visit_item_mut(self, node);
    }
}

/// # Errors
/// Refuses invalid UTF-8 or Rust syntax instead of inventing a production identity.
pub fn production(path: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    if test_path(Path::new(path)) {
        return Ok(Vec::new());
    }
    if Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "rs")
    {
        let mut file = syn::parse_file(std::str::from_utf8(bytes)?)
            .with_context(|| format!("parse production source {path}"))?;
        Production.visit_file_mut(&mut file);
        return Ok(file.to_token_stream().to_string().into_bytes());
    }
    Ok(bytes.to_vec())
}

pub(super) fn within(path: &str, prefix: &str) -> bool {
    Path::new(path).starts_with(prefix)
}

pub(super) fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub(super) fn evidence_path(root: &Path, relative: &Path) -> Result<std::path::PathBuf> {
    anyhow::ensure!(
        relative.to_str().is_some_and(safe_path),
        "unsafe native evidence path"
    );
    let root = platform::paths::canonical(root)?;
    let file = platform::paths::canonical(&root.join(relative))?;
    anyhow::ensure!(
        file.starts_with(&root),
        "native evidence escapes its directory"
    );
    Ok(file)
}

fn module_source_path(base: &Path, relative: &str) -> Option<String> {
    if relative.contains('\\') || Path::new(relative).is_absolute() {
        return None;
    }
    let joined = base.join(relative);
    let mut parts = Vec::new();
    for component in joined.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_str()?),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}

fn module_directory(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    match path.file_name().and_then(|value| value.to_str()) {
        Some("lib.rs" | "main.rs" | "mod.rs") => parent.to_path_buf(),
        _ => parent.join(path.file_stem().unwrap_or_default()),
    }
}

fn package_root<'a>(
    sources: &'a BTreeMap<String, Vec<u8>>,
    target: &str,
) -> Result<Option<&'a str>> {
    let candidate = sources
        .iter()
        .filter_map(|(path, bytes)| {
            if Path::new(path).file_name()? != "Cargo.toml" {
                return None;
            }
            let root = Path::new(path).parent()?.to_str()?;
            if !within(target, root) {
                return None;
            }
            Some((root, bytes))
        })
        .max_by_key(|(root, _)| root.len());
    let Some((root, bytes)) = candidate else {
        return Ok(None);
    };
    let manifest: toml::Table = std::str::from_utf8(bytes)?.parse()?;
    let Some(package) = manifest.get("package") else {
        return Ok(None);
    };
    package
        .as_table()
        .context("package metadata is not a table")?;
    Ok(Some(root))
}

fn implicit_package_target(root: &str, target: &str) -> bool {
    let Ok(relative) = Path::new(target).strip_prefix(root) else {
        return false;
    };
    let parts: Vec<_> = relative.iter().filter_map(|part| part.to_str()).collect();
    match parts.as_slice() {
        ["build.rs"]
        | ["src", "lib.rs" | "main.rs"]
        | ["src", "bin", _, "main.rs"]
        | ["examples" | "tests" | "benches", _, "main.rs"] => true,
        ["src", "bin", file] | ["examples" | "tests" | "benches", file] => Path::new(file)
            .extension()
            .is_some_and(|value| value == "rs"),
        _ => false,
    }
}

fn manifest_target_paths(manifest: &toml::Table) -> Result<Vec<&str>> {
    let mut entries = Vec::new();
    entries.extend(manifest.get("lib"));
    for name in ["bin", "example", "test", "bench"] {
        if let Some(value) = manifest.get(name) {
            entries.extend(
                value
                    .as_array()
                    .context("package targets are not an array")?,
            );
        }
    }
    let mut paths = Vec::new();
    for entry in entries {
        let target = entry.as_table().context("package target is not a table")?;
        if let Some(path) = target.get("path") {
            paths.push(
                path.as_str()
                    .context("package target path is not a string")?,
            );
        }
    }
    if let Some(build) = manifest.get("package").and_then(|value| value.get("build")) {
        match build {
            toml::Value::Boolean(_) => {}
            toml::Value::String(path) => paths.push(path),
            _ => anyhow::bail!("package build is not a path or Boolean"),
        }
    }
    Ok(paths)
}

fn package_target(sources: &BTreeMap<String, Vec<u8>>, root: &str, target: &str) -> Result<bool> {
    let path = if root.is_empty() {
        "Cargo.toml".into()
    } else {
        format!("{root}/Cargo.toml")
    };
    let bytes = sources
        .get(&path)
        .context("package manifest is unavailable")?;
    let manifest: toml::Table = std::str::from_utf8(bytes)?.parse()?;
    if implicit_package_target(root, target) {
        return Ok(true);
    }
    for path in manifest_target_paths(&manifest)? {
        let resolved = module_source_path(Path::new(root), path)
            .context("package target path cannot be resolved")?;
        if resolved == target {
            return Ok(true);
        }
    }
    Ok(false)
}

struct ModuleDeclarations<'a> {
    sources: &'a BTreeMap<String, Vec<u8>>,
    target: &'a str,
    base: PathBuf,
    explicit_base: PathBuf,
    gated: bool,
    incoming: Vec<bool>,
    uncertain: bool,
    local: bool,
}

impl syn::visit_mut::VisitMut for ModuleDeclarations<'_> {
    fn visit_item_mod_mut(&mut self, node: &mut syn::ItemMod) {
        let saved = (self.base.clone(), self.explicit_base.clone(), self.gated);
        self.gated |= conditional_verification(&node.attrs);
        let paths: Vec<_> = node
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("path"))
            .collect();
        let explicit = paths.first().and_then(|attr| match &attr.meta {
            syn::Meta::NameValue(value) => match &value.value {
                syn::Expr::Lit(value) => match &value.lit {
                    syn::Lit::Str(value) => Some(value.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        });
        let unknown = paths.len() > 1
            || (!paths.is_empty() && explicit.is_none())
            || (node.content.is_some() && !paths.is_empty())
            || node
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("cfg_attr"));
        self.uncertain |= unknown;
        if let Some((_, items)) = &mut node.content {
            self.base = match explicit {
                Some(path) => self.explicit_base.join(path),
                None => self.base.join(node.ident.to_string()),
            };
            self.explicit_base = self.base.clone();
            for item in items {
                self.visit_item_mut(item);
            }
        } else {
            let candidates = match explicit {
                Some(path) => vec![module_source_path(&self.explicit_base, &path)],
                None => vec![
                    module_source_path(&self.base, &format!("{}.rs", node.ident)),
                    module_source_path(&self.base, &format!("{}/mod.rs", node.ident)),
                ],
            };
            let present: Vec<_> = candidates
                .iter()
                .filter_map(Option::as_ref)
                .filter(|path| self.sources.contains_key(*path))
                .collect();
            self.uncertain |= rules::verification_rules::unresolved_source_requires_production(
                self.local,
                self.gated,
                present.is_empty()
                    || present.iter().any(|path| {
                        Path::new(path)
                            .extension()
                            .is_none_or(|extension| extension != "rs")
                    }),
            );
            if candidates.iter().any(Option::is_none) {
                self.uncertain = true;
            }
            if present.iter().any(|path| path.as_str() == self.target) {
                self.incoming
                    .push(self.gated && !unknown && present.len() == 1);
            }
        }
        (self.base, self.explicit_base, self.gated) = saved;
    }

    fn visit_item_macro_mut(&mut self, node: &mut syn::ItemMacro) {
        let saved = self.gated;
        self.gated |= conditional_verification(&node.attrs);
        self.uncertain |= rules::verification_rules::unresolved_source_requires_production(
            self.local, self.gated, true,
        );
        syn::visit_mut::visit_item_macro_mut(self, node);
        self.gated = saved;
    }

    fn visit_macro_mut(&mut self, node: &mut syn::Macro) {
        if node
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "include")
        {
            self.uncertain |= rules::verification_rules::unresolved_source_requires_production(
                self.local, self.gated, true,
            );
            if let Ok(path) = syn::parse2::<syn::LitStr>(node.tokens.clone())
                && module_source_path(&self.explicit_base, &path.value()).as_deref()
                    == Some(self.target)
            {
                self.incoming.push(self.gated);
            }
        }
        syn::visit_mut::visit_macro_mut(self, node);
    }
}

/// # Errors
/// Refuses malformed package metadata rather than discarding a source owner.
pub fn verification_module_context(
    sources: &BTreeMap<String, Vec<u8>>,
    target: &str,
) -> Result<bool> {
    let Some(root) = package_root(sources, target)? else {
        return Ok(false);
    };
    if package_target(sources, root, target)? {
        return Ok(false);
    }
    let mut incoming = Vec::new();
    let mut uncertain = false;
    for (path, bytes) in sources.iter().filter(|(path, _)| {
        Path::new(path)
            .extension()
            .is_some_and(|value| value == "rs")
    }) {
        let local = within(path, root) && package_root(sources, path)? == Some(root);
        let file = std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| syn::parse_file(text).ok());
        let Some(mut file) = file else {
            uncertain |= rules::verification_rules::unresolved_source_requires_production(
                local,
                test_path(Path::new(path)),
                true,
            );
            continue;
        };
        let mut declarations = ModuleDeclarations {
            sources,
            target,
            base: module_directory(Path::new(path)),
            explicit_base: Path::new(path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_path_buf(),
            gated: test_path(Path::new(path)) || conditional_verification(&file.attrs),
            incoming: Vec::new(),
            uncertain: false,
            local,
        };
        declarations.visit_file_mut(&mut file);
        incoming.extend(declarations.incoming);
        uncertain |= declarations.uncertain;
    }
    Ok(rules::verification_rules::verification_module(
        u64::try_from(incoming.len())?,
        u64::try_from(incoming.iter().filter(|gated| **gated).count())?,
        uncertain,
    ))
}

pub(super) fn production_id(root: &Path, tree: &Tree, prefix: &str) -> Result<String> {
    let paths = crate::assure::paths(root, tree)?;
    let wanted: Vec<_> = paths
        .iter()
        .filter(|path| relevant(Path::new(path)) && within(path, prefix))
        .cloned()
        .collect();
    let sources = crate::assure::contents(
        root,
        tree,
        &paths
            .into_iter()
            .filter(|path| {
                Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension == "rs")
                    || Path::new(path)
                        .file_name()
                        .is_some_and(|name| name == "Cargo.toml")
            })
            .collect::<Vec<_>>(),
    )?;
    let mut values = Vec::new();
    for (path, bytes) in crate::assure::contents(root, tree, &wanted)? {
        if Path::new(&path)
            .extension()
            .is_some_and(|extension| extension == "rs")
            && verification_module_context(&sources, &path)?
        {
            continue;
        }
        let production = production(&path, &bytes)?;
        if !production.is_empty() {
            values.extend_from_slice(path.as_bytes());
            values.push(0);
            values.extend_from_slice(&production);
            values.push(0);
        }
    }
    Ok(digest(&values))
}

pub(super) fn snapshot(root: &Path) -> Result<(String, String)> {
    let state = crate::assure::worktree_snapshot(root)?;
    let tree = platform::issue_context::git(
        root,
        &["rev-parse", &format!("{}^{{tree}}", state.commit)],
        None,
    )?;
    Ok((state.commit, tree.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_module_context_respects_all_incoming_module_declarations() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        std::fs::create_dir(root.join("src"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )?;
        let output = platform::tool::Tool::Git
            .command()
            .arg("-C")
            .arg(root)
            .args(["init", "--quiet"])
            .output()?;
        anyhow::ensure!(output.status.success(), "{output:?}");
        std::fs::write(root.join("src/lib.rs"), "#[cfg(kani)] mod contract;")?;
        std::fs::write(
            root.join("src/contract.rs"),
            "fn helper() -> bool { false }",
        )?;
        assert_eq!(
            production_id(root, &Tree::WorkingTree, "src/contract.rs")?,
            digest(&[])
        );
        let output = platform::tool::Tool::Git
            .command()
            .arg("-C")
            .arg(root)
            .args(["add", "."])
            .output()?;
        anyhow::ensure!(output.status.success(), "{output:?}");
        std::fs::write(root.join("src/lib.rs"), "mod contract;")?;
        assert_eq!(
            production_id(root, &Tree::Index, "src/contract.rs")?,
            digest(&[])
        );
        let first = production_id(root, &Tree::WorkingTree, "src/contract.rs")?;
        assert_ne!(first, digest(&[]));
        std::fs::write(root.join("src/contract.rs"), "fn helper() -> bool { true }")?;
        assert_ne!(
            production_id(root, &Tree::WorkingTree, "src/contract.rs")?,
            first
        );
        Ok(())
    }

    fn module_sources(declaration: &str, target: &str) -> BTreeMap<String, Vec<u8>> {
        [
            ("Cargo.toml", "[package]\nname='fixture'\nversion='0.1.0'\n"),
            ("src/lib.rs", declaration),
            (target, "fn helper() -> bool { false }"),
        ]
        .into_iter()
        .map(|(path, text)| (path.to_owned(), text.as_bytes().to_vec()))
        .collect()
    }

    #[test]
    fn verification_module_context_rejects_production_aliases_and_unknown_expansion() -> Result<()>
    {
        let target = "src/contract.rs";
        for declaration in [
            "#[cfg(kani)] mod contract;",
            "#[cfg(test)] mod contract;",
            "#[cfg(any(test,kani))] mod contract;",
            "#[cfg(any(kani,test))] mod contract;",
            "#[cfg(kani)] mod contract; #[cfg(kani)] include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\"));",
        ] {
            assert!(
                verification_module_context(&module_sources(declaration, target), target)?,
                "{declaration}"
            );
        }
        for declaration in [
            "",
            "mod contract;",
            "#[cfg(feature=\"test\")] mod contract;",
            "#[kani::proof] mod contract;",
            "#[test] mod contract;",
            "#[cfg(kani)] mod contract; #[path=\"contract.rs\"] mod alias;",
            "#[cfg(kani)] mod contract; #[path=\"./contract.rs\"] mod alias;",
            "#[cfg(kani)] mod contract; #[cfg_attr(any(),path=\"contract.rs\")] mod alias;",
            "#[cfg(kani)] #[cfg_attr(any(),path=\"other.rs\")] mod contract;",
            "#[cfg(kani)] #[path=concat!(\"contract\",\".rs\")] mod contract;",
            "#[cfg(kani)] #[path=123] mod contract;",
            "#[cfg(kani)] #[path] mod contract;",
            "#[cfg(kani)] #[path(\"contract.rs\")] mod contract;",
            "#[cfg(kani)] #[path=\"contract.rs\"] #[path=\"contract.rs\"] mod contract;",
            "#[cfg(kani)] mod contract; #[cfg(kani)] #[path=\"nested\"] mod inline {}",
            "#[cfg(kani)] mod contract; #[path=concat!(\"contract\",\".rs\")] mod alias;",
            "#[cfg(kani)] mod contract; #[path=\"contract.rs\"] #[path=\"other.rs\"] mod alias;",
            "#[cfg(kani)] mod contract; include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\"));",
            "#[cfg(kani)] mod contract; fn run() { std::include!(\"contract.rs\"); }",
            "#[cfg(kani)] mod contract; fn run() { let _ = std::include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\")); }",
            "#[cfg(kani)] mod contract; unknown!();",
            "#[cfg(kani)] mod contract; mod missing;",
            "#[cfg(kani)] mod contract; #[path=\"opaque.txt\"] mod opaque;",
            "#[cfg(kani)] mod contract; macro_rules! hidden { () => { mod alias; } }",
            "mod broken(",
        ] {
            assert!(
                !verification_module_context(&module_sources(declaration, target), target)?,
                "{declaration}"
            );
        }
        for (file, declaration, target) in [
            (
                "src/main.rs",
                "#[cfg(kani)] mod contract;",
                "src/contract/mod.rs",
            ),
            (
                "src/parent.rs",
                "#[cfg(kani)] mod contract;",
                "src/parent/contract.rs",
            ),
            (
                "src/parent/mod.rs",
                "#[cfg(kani)] mod contract;",
                "src/parent/contract.rs",
            ),
            (
                "src/lib.rs",
                "mod parent { #[cfg(kani)] mod contract; }",
                "src/parent/contract.rs",
            ),
            (
                "src/lib.rs",
                "#[cfg(kani)] mod parent { mod contract; }",
                "src/parent/contract.rs",
            ),
            (
                "src/lib.rs",
                "#[cfg(kani)] #[path=\"shared/body.rs\"] mod contract;",
                "src/shared/body.rs",
            ),
            (
                "src/lib.rs",
                "#![cfg(kani)] mod contract;",
                "src/contract.rs",
            ),
        ] {
            let mut sources = module_sources("", target);
            sources.insert(file.into(), declaration.as_bytes().to_vec());
            assert!(
                verification_module_context(&sources, target)?,
                "{file}: {declaration}"
            );
        }
        Ok(())
    }

    #[test]
    fn verification_module_context_enforces_package_targets_and_paths() -> Result<()> {
        let target = "src/contract.rs";
        let original = module_sources("#[cfg(kani)] mod contract;", target);
        let mut opaque = module_sources(
            "#[cfg(kani)] mod contract; #[path=\"opaque.txt\"] mod opaque;",
            target,
        );
        opaque.insert(
            "src/opaque.txt".into(),
            b"#[path=\"contract.rs\"] mod alias;".to_vec(),
        );
        assert!(!verification_module_context(&opaque, target)?);
        let mut sources = original.clone();
        sources.insert(
            "Cargo.toml".into(),
            b"[package]\nname='fixture'\nbuild='src/contract.rs'\n".to_vec(),
        );
        assert!(!verification_module_context(&sources, target)?);
        for target in [
            "build.rs",
            "src/bin/tool.rs",
            "src/bin/tool/main.rs",
            "examples/demo.rs",
            "examples/demo/main.rs",
        ] {
            let mut sources =
                module_sources("#[cfg(kani)] #[path=\"../build.rs\"] mod contract;", target);
            let path = target
                .strip_prefix("src/")
                .map_or_else(|| format!("../{target}"), str::to_owned);
            sources.insert(
                "src/lib.rs".into(),
                format!("#[cfg(kani)] #[path=\"{path}\"] mod contract;").into_bytes(),
            );
            assert!(!verification_module_context(&sources, target)?, "{target}");
        }
        let mut sources = original.clone();
        sources.insert("src/contract/mod.rs".into(), b"fn other() {}".to_vec());
        assert!(!verification_module_context(&sources, target)?);
        for kind in ["lib", "bin", "example", "test", "bench"] {
            let mut sources = original.clone();
            let table = if kind == "lib" {
                "[lib]".to_owned()
            } else {
                format!("[[{kind}]]")
            };
            sources
                .get_mut("Cargo.toml")
                .unwrap()
                .extend_from_slice(format!("{table}\npath='src/contract.rs'\n").as_bytes());
            assert!(!verification_module_context(&sources, target)?, "{kind}");
        }
        for text in [b"mod broken(".as_slice(), b"\xff".as_slice()] {
            let mut sources = original.clone();
            sources.insert("src/broken.rs".into(), text.to_vec());
            assert!(!verification_module_context(&sources, target)?);
        }
        let mut sources = original.clone();
        sources.insert(
            "other/Cargo.toml".into(),
            b"[package]\nname='other'\n".to_vec(),
        );
        sources.insert(
            "other/src/lib.rs".into(),
            b"include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\"));".to_vec(),
        );
        assert!(verification_module_context(&sources, target)?);
        sources.insert(
            "other/src/lib.rs".into(),
            b"#[path=\"../../src/contract.rs\"] mod alias;".to_vec(),
        );
        assert!(!verification_module_context(&sources, target)?);
        sources.insert(
            "other/src/lib.rs".into(),
            b"include!(\"../../src/contract.rs\");".to_vec(),
        );
        assert!(!verification_module_context(&sources, target)?);
        let mut sources = original.clone();
        sources.insert("Cargo.toml".into(), b"[package".to_vec());
        assert!(verification_module_context(&sources, target).is_err());
        sources.remove("Cargo.toml");
        assert!(!verification_module_context(&sources, target)?);
        for (base, path, expected) in [
            ("src", "./contract.rs", Some("src/contract.rs")),
            ("src/inner", "../contract.rs", Some("src/contract.rs")),
            ("", "../contract.rs", None),
            ("src", "/contract.rs", None),
            ("src", "..\\contract.rs", None),
        ] {
            assert_eq!(
                module_source_path(Path::new(base), path).as_deref(),
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn source_context_scopes_separate_ordinary_siblings_and_invalid_fixtures() -> Result<()> {
        let target = "src/contract.rs";
        let original = module_sources("#[cfg(kani)] mod contract;", target);
        let mut siblings = module_sources("#[cfg(kani)] mod contract; mod ordinary;", target);
        siblings.insert("src/ordinary.rs".into(), b"fn ordinary() {}".to_vec());
        assert!(verification_module_context(&siblings, target)?);
        let generated = module_sources(
            "#[cfg(kani)] mod contract; #[cfg(kani)] mod generated;",
            target,
        );
        assert!(verification_module_context(&generated, target)?);
        for (path, expected) in [
            ("src/invalid.rs", false),
            ("tests/fixtures/invalid.rs", true),
            ("other/src/invalid.rs", true),
            ("other/tests/fixtures/invalid.rs", true),
        ] {
            for payload in [b"\xff".as_slice(), b"mod broken(".as_slice()] {
                let mut sources = original.clone();
                sources.insert(
                    "other/Cargo.toml".into(),
                    b"[package]\nname='other'\n".to_vec(),
                );
                sources.insert(path.into(), payload.to_vec());
                assert_eq!(
                    verification_module_context(&sources, target)?,
                    expected,
                    "{path}"
                );
            }
        }
        let mut external = original;
        external.insert(
            "other/Cargo.toml".into(),
            b"[package]\nname='other'\n".to_vec(),
        );
        external.insert("other/src/lib.rs".into(), b"mod unavailable;".to_vec());
        assert!(verification_module_context(&external, target)?);
        Ok(())
    }

    #[test]
    fn verification_attributes_have_independent_disjunctive_witnesses() -> Result<()> {
        let cases = [
            ("#[cfg(test)]", true, true),
            ("#[cfg(kani)]", true, true),
            ("#[cfg(any(test,kani))]", true, true),
            ("#[cfg(any(kani,test))]", true, true),
            ("#[test]", false, true),
            ("#[kani::proof]", false, true),
            ("#[cfg(feature=\"std\")]", false, false),
            ("#[cfg_attr(test,test)]", false, false),
            ("#[other::test]", false, false),
            ("#[kani::unwind(3)]", false, false),
            ("#[doc=\"description\"]", false, false),
        ];
        assert!(!verification(&[]));
        assert!(!conditional_verification(&[]));
        for (first, conditional_first, any_first) in cases {
            for (second, conditional_second, any_second) in cases {
                let function: syn::ItemFn =
                    syn::parse_str(&format!("{first} {second} fn sample() {{}}"))?;
                assert_eq!(
                    conditional_verification(&function.attrs),
                    conditional_first || conditional_second
                );
                assert_eq!(verification(&function.attrs), any_first || any_second);
            }
        }
        Ok(())
    }

    #[test]
    fn manifest_target_types_are_explicit_and_fail_closed() -> Result<()> {
        let target = "src/contract.rs";
        let original = module_sources("#[cfg(kani)] mod contract;", target);
        let package = "[package]\nname='fixture'\n";
        for value in ["true", "false", "'build.rs'"] {
            let mut sources = original.clone();
            sources.insert(
                "Cargo.toml".into(),
                format!("{package}build={value}\n").into_bytes(),
            );
            assert!(verification_module_context(&sources, target)?, "{value}");
        }
        for value in ["0", "1.5", "[]", "{}", "1979-05-27T07:32:00Z"] {
            let mut sources = original.clone();
            sources.insert(
                "Cargo.toml".into(),
                format!("{package}build={value}\n").into_bytes(),
            );
            assert!(
                verification_module_context(&sources, target).is_err(),
                "{value}"
            );
        }
        for declaration in [
            "package=false\n",
            "lib=false\n[package]\nname='fixture'\n",
            "bin=false\n[package]\nname='fixture'\n",
            "bin=['file.rs']\n[package]\nname='fixture'\n",
        ] {
            let mut sources = original.clone();
            sources.insert("Cargo.toml".into(), declaration.as_bytes().to_vec());
            assert!(
                verification_module_context(&sources, target).is_err(),
                "{declaration}"
            );
        }
        for value in ["false", "0", "1.5", "[]", "{}", "1979-05-27T07:32:00Z"] {
            let mut sources = original.clone();
            sources.insert(
                "Cargo.toml".into(),
                format!("{package}[lib]\npath={value}\n").into_bytes(),
            );
            assert!(
                verification_module_context(&sources, target).is_err(),
                "{value}"
            );
        }
        Ok(())
    }

    #[test]
    fn configuration_requires_literal_metadata_and_unchanged_executable_tokens() -> Result<()> {
        for path in [
            "justfile",
            "Justfile",
            ".justfile",
            "Cargo.toml",
            "settings.json",
        ] {
            assert!(configuration_change(path, Some(b"before"), Some(b"after"))?);
        }
        assert!(!configuration_change(
            "main.sh",
            Some(b"same"),
            Some(b"same")
        )?);
        let original = br#"const NAMES: &[&str] = &["first"]; pub fn choose() -> bool { false }"#;
        for metadata in [
            r#"const NAMES: &[&str] = &["second", "third"];"#,
            "pub const NAMES: &[&str] = &[];",
            "",
        ] {
            let changed = format!("{metadata} pub fn choose() -> bool {{ false }}");
            for (before, after) in [
                (original.as_slice(), changed.as_bytes()),
                (changed.as_bytes(), original.as_slice()),
            ] {
                assert!(configuration_change(
                    "src/inventory.rs",
                    Some(before),
                    Some(after)
                )?);
            }
        }
        for changed in [
            r#"const NAMES: &[&str] = &["second"]; pub fn choose() -> bool { true }"#,
            "const NAMES: &[&str] = &[make_name()]; pub fn choose() -> bool { false }",
            r#"#[used] const NAMES: &[&str] = &["first"]; pub fn choose() -> bool { false }"#,
            "const NAMES: &[u8] = &[1]; pub fn choose() -> bool { false }",
            r#"static NAMES: &[&str] = &["first"]; pub fn choose() -> bool { false }"#,
            r#"const NAMES<const N: usize>: &[&str] = &["first"]; pub fn choose() -> bool { false }"#,
            r#"const NAMES: &[&str] where str: Sized = &["first"]; pub fn choose() -> bool { false }"#,
            r#"const NAMES: &mut [&str] = &mut ["first"]; pub fn choose() -> bool { false }"#,
            "const NAMES: &[&mut str] = &[]; pub fn choose() -> bool { false }",
            r#"const NAMES: &[&str] = &mut ["first"]; pub fn choose() -> bool { false }"#,
            r#"const NAMES: &[&str] = #[cfg(test)] &["first"]; pub fn choose() -> bool { false }"#,
            r#"const NAMES: &[&str] = &#[cfg(test)] ["first"]; pub fn choose() -> bool { false }"#,
            r#"const NAMES: &[&str] = &[#[cfg(test)] "first"]; pub fn choose() -> bool { false }"#,
            r#"const NAMES: &[&str] = &["first"]; macro_rules! choose { () => {false} }"#,
            r#"const NAMES: &[&str] = &["first"];"#,
        ] {
            assert!(
                configuration_change("src/inventory.rs", Some(original), Some(changed.as_bytes()))
                    .ok()
                    != Some(true),
                "{changed}"
            );
        }
        assert!(configuration_change(
            "src/inventory.rs",
            None,
            Some(br#"const NAMES: &[&str] = &["a"];"#)
        )?);
        assert!(!configuration_change(
            "src/lib.rs",
            None,
            Some(b"fn choose() {}")
        )?);
        assert!(configuration_change("src/lib.rs", None, Some(b"invalid rust!")).is_err());
        Ok(())
    }

    #[test]
    fn rust_tests_and_documentation_are_not_production_but_decisions_are() -> Result<()> {
        let original = b"pub fn allowed() -> bool { false }";
        let tests = b"/// The decision.\npub fn allowed() -> bool { false } #[cfg(test)] mod tests { #[test] fn rejects() { assert!(!super::allowed()); } }";
        assert_eq!(
            production("src/lib.rs", original)?,
            production("src/lib.rs", tests)?
        );
        assert_ne!(
            production("src/lib.rs", original)?,
            production("src/lib.rs", b"pub fn allowed() -> bool { true }")?
        );
        assert_eq!(production("tests/contract.rs", tests)?, Vec::<u8>::new());
        assert!(!safe_path("../outside.rs"));
        assert!(!safe_path("/outside.rs"));
        assert!(!within("src-other/lib.rs", "src"));
        Ok(())
    }
}
