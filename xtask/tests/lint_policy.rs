//! The workspace lint policy holds in every crate.
//! Unsafe code stays forbidden except in the functions `policy/unsafe-sites.toml` lists.
//! The crates that hold them deny it and keep the workspace's other lints unchanged.
//! The Git guard keeps its link-time optimization.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the package sits inside the workspace")
        .to_path_buf()
}

fn manifest(path: &Path) -> toml::Table {
    toml::from_str(&fs::read_to_string(path).expect("readable manifest")).expect("valid manifest")
}

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("readable source directory") {
        let path = entry.expect("readable entry").path();
        if path.is_dir() {
            if path.file_name().is_none_or(|name| name != "target") {
                rust_sources(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The function after each lint expectation that names `unsafe_code`, or `<module>` for an inner attribute that covers a whole module.
fn unsafe_expectations(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("expect(") {
        let inner = rest[..start].ends_with("#![");
        let outer = rest[..start].ends_with("#[");
        rest = &rest[start..];
        let end = rest.find(")]").unwrap_or(rest.len());
        let names_unsafe = rest[..end].contains("unsafe_code");
        rest = &rest[end..];
        if names_unsafe && inner {
            found.push("<module>".to_owned());
        } else if names_unsafe && outer {
            let name = rest
                .split("fn ")
                .nth(1)
                .and_then(|tail| {
                    tail.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                })
                .unwrap_or_default();
            found.push(name.to_owned());
        }
    }
    found
}

fn listed_sites(root: &Path) -> BTreeSet<(String, String)> {
    let policy = manifest(&root.join("policy/unsafe-sites.toml"));
    policy["site"]
        .as_array()
        .expect("site entries")
        .iter()
        .map(|site| {
            (
                site["path"].as_str().expect("site path").to_owned(),
                site["function"].as_str().expect("site function").to_owned(),
            )
        })
        .collect()
}

fn source_sites(root: &Path) -> BTreeSet<(String, String)> {
    let mut sources = Vec::new();
    for directory in ["xtask", "crates"] {
        rust_sources(&root.join(directory), &mut sources);
    }
    let this = root.join("xtask/tests/lint_policy.rs");
    sources
        .iter()
        .filter(|path| **path != this)
        .flat_map(|path| {
            let relative = path
                .strip_prefix(root)
                .expect("source inside the workspace")
                .to_string_lossy()
                .replace('\\', "/");
            unsafe_expectations(&fs::read_to_string(path).expect("readable source"))
                .into_iter()
                .map(move |function| (relative.clone(), function))
        })
        .collect()
}

#[test]
fn every_unsafe_site_says_why_miri_cannot_reach_it() {
    let policy = manifest(&root().join("policy/unsafe-sites.toml"));
    for site in policy["site"].as_array().expect("site entries") {
        assert!(
            site.get("outside_miri")
                .and_then(toml::Value::as_str)
                .is_some_and(|reason| !reason.trim().is_empty()),
            "{site:?} needs an outside_miri reason"
        );
    }
}

/// The documentation of the function `name` anywhere in `file`.
fn function_documentation(file: &syn::File, name: &str) -> Option<String> {
    use syn::visit::Visit;
    struct Find<'a>(&'a str, Option<String>);
    impl<'ast> Visit<'ast> for Find<'_> {
        fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
            if function.sig.ident == self.0 {
                let lines: Vec<String> = function
                    .attrs
                    .iter()
                    .filter(|attribute| attribute.path().is_ident("doc"))
                    .filter_map(|attribute| match &attribute.meta {
                        syn::Meta::NameValue(syn::MetaNameValue {
                            value:
                                syn::Expr::Lit(syn::ExprLit {
                                    lit: syn::Lit::Str(text),
                                    ..
                                }),
                            ..
                        }) => Some(text.value().trim().to_owned()),
                        _ => None,
                    })
                    .collect();
                self.1 = Some(lines.join(" "));
            }
            syn::visit::visit_item_fn(self, function);
        }
    }
    let mut find = Find(name, None);
    find.visit_file(file);
    find.1
}

#[test]
fn every_unsafe_site_is_a_semantic_boundary_exemption_with_existing_evidence() {
    let root = root();
    for (path, function) in listed_sites(&root) {
        let file = syn::parse_file(&fs::read_to_string(root.join(&path)).expect("readable site"))
            .expect("parsable site");
        let documentation = function_documentation(&file, &function)
            .unwrap_or_else(|| panic!("{path} defines no function {function}"));
        let exemption = xtask::obligations::exemption(&documentation)
            .unwrap_or_else(|error| panic!("{path}::{function}: {error:#}"))
            .unwrap_or_else(|| panic!("{path}::{function} carries no exemption"));
        assert_eq!(
            exemption.reason,
            rules::obligation_rules::Reason::SemanticBoundary,
            "{path}::{function}"
        );
        let (evidence, test) = &exemption.evidence;
        assert!(
            fs::read_to_string(root.join(evidence))
                .is_ok_and(|text| text.contains(&format!("fn {test}("))),
            "{path}::{function} names missing evidence {evidence}::{test}"
        );
    }
}

#[test]
fn an_unsafe_block_without_a_safety_comment_fails_clippy() {
    let workspace = manifest(&root().join("Cargo.toml"));
    assert_eq!(
        workspace["workspace"]["lints"]["clippy"]["undocumented_unsafe_blocks"].as_str(),
        Some("deny")
    );
    let lints = manifest(&root().join("crates/dotfiles/Cargo.toml"))["lints"].clone();
    let documented = "#[expect(unsafe_code, reason = \"Exercises the foreign-interface lint boundary.\")]\npub fn read(value: &u8) -> u8 {\n    // SAFETY: The borrowed byte remains valid throughout this read.\n    unsafe { core::ptr::from_ref(value).read() }\n}\n";
    assert!(compile_probe(&lints, documented, "clippy").status.success());
    let undocumented = documented.replace(
        "    // SAFETY: The borrowed byte remains valid throughout this read.\n",
        "",
    );
    let rejected = compile_probe(&lints, &undocumented, "clippy");
    assert!(!rejected.status.success());
    assert!(
        has_error(&rejected, "clippy::undocumented_unsafe_blocks"),
        "{}",
        String::from_utf8_lossy(&rejected.stdout)
    );
}

fn compile_probe(lints: &toml::Value, source: &str, command: &str) -> std::process::Output {
    let scope = tempfile::tempdir().unwrap();
    fs::create_dir(scope.path().join("src")).unwrap();
    let manifest: toml::Table = [("lints".to_owned(), lints.clone())].into_iter().collect();
    fs::write(scope.path().join("Cargo.toml"), format!("[package]\nname = 'unsafe-lint-probe'\nversion = '0.0.0'\nedition = '2024'\n[workspace]\n{}", toml::to_string(&manifest).unwrap())).unwrap();
    fs::write(scope.path().join("src/lib.rs"), source).unwrap();
    xtask::tool::Tool::Cargo
        .command()
        .current_dir(scope.path())
        .env("CARGO_TARGET_DIR", scope.path().join("target"))
        .args([command, "--quiet", "--message-format=json"])
        .output()
        .unwrap()
}

fn has_error(output: &std::process::Output, lint: &str) -> bool {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|message| {
            message["message"]["level"] == "error" && message["message"]["code"]["code"] == lint
        })
}

#[test]
fn an_unlisted_unsafe_block_is_rejected_by_the_compiler() {
    let source = "pub fn read(value: &u8) -> u8 { unsafe { core::ptr::from_ref(value).read() } }\n";
    let workspace = manifest(&root().join("Cargo.toml"));
    let forbidden = compile_probe(&workspace["workspace"]["lints"], source, "check");
    assert!(!forbidden.status.success());
    assert!(has_error(&forbidden, "unsafe_code"));
    let foreign = manifest(&root().join("crates/dotfiles/Cargo.toml"));
    let denied = compile_probe(&foreign["lints"], source, "check");
    assert!(!denied.status.success());
    assert!(has_error(&denied, "unsafe_code"));
}

#[test]
fn no_test_mutates_the_process_environment() {
    let root = root();
    let mut sources = Vec::new();
    for directory in ["xtask", "crates"] {
        rust_sources(&root.join(directory), &mut sources);
    }
    let this = root.join("xtask/tests/lint_policy.rs");
    let calls: Vec<String> = sources
        .iter()
        .filter(|path| **path != this)
        .filter(|path| {
            let text = fs::read_to_string(path).expect("readable source");
            ["set_var(", "remove_var("]
                .iter()
                .any(|call| text.contains(call))
        })
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        calls.is_empty(),
        "pass variables through Command::env: {calls:?}"
    );
}

#[test]
fn unsafe_code_is_expected_only_at_the_listed_foreign_interface_sites() {
    let root = root();
    assert_eq!(source_sites(&root), listed_sites(&root));
}

#[test]
fn the_expectation_scan_finds_functions_and_whole_modules() {
    let text = "#[expect(\n    unsafe_code,\n    reason = \"r\"\n)]\nfn call() {}\n#[expect(clippy::too_many_lines, reason = \"r\")]\nfn long() {}\n#![expect(unsafe_code, reason = \"r\")]\n";
    assert_eq!(unsafe_expectations(text), ["call", "<module>"]);
}

#[test]
fn every_crate_checks_the_workspace_lints_and_only_foreign_interface_crates_deny_unsafe_code() {
    let root = root();
    let workspace = manifest(&root.join("Cargo.toml"));
    let lints = workspace["workspace"]["lints"]
        .as_table()
        .expect("workspace lints")
        .clone();
    assert_eq!(lints["rust"]["unsafe_code"].as_str(), Some("forbid"));
    assert_eq!(lints["clippy"]["pedantic"]["level"].as_str(), Some("warn"));
    let mut denying = lints.clone();
    denying["rust"]
        .as_table_mut()
        .expect("rust lints")
        .insert("unsafe_code".to_owned(), "deny".into());
    let sites = listed_sites(&root);
    let mut members = vec![root.join("xtask")];
    for entry in fs::read_dir(root.join("crates")).expect("readable crates directory") {
        members.push(entry.expect("readable entry").path());
    }
    for member in members {
        let relative = member
            .strip_prefix(&root)
            .expect("member inside the workspace")
            .to_string_lossy()
            .replace('\\', "/");
        let own = manifest(&member.join("Cargo.toml"))["lints"].clone();
        let holds_sites = sites
            .iter()
            .any(|(path, _)| path.starts_with(&format!("{relative}/")));
        if holds_sites {
            assert_eq!(own, toml::Value::Table(denying.clone()), "{relative}");
        } else {
            assert_eq!(
                own.get("workspace").and_then(toml::Value::as_bool),
                Some(true),
                "{relative} inherits the workspace lints"
            );
        }
    }
}

#[test]
fn the_git_guard_installs_from_its_link_time_optimized_profile() {
    let workspace = manifest(&root().join("Cargo.toml"));
    let profile = &workspace["profile"][dotfiles::setup::GUARD_PROFILE];
    assert_eq!(profile["inherits"].as_str(), Some("release"));
    assert_eq!(profile["lto"].as_bool(), Some(true));
    assert_eq!(dotfiles::setup::GUARD_CRATE, "crates/dotguard");
}
