//! The obligation manifest and the closure gate that holds it to the source.
//!
//! `policy/obligations.toml` names the verifier of every rule module and the pilots, and `policy/baselines.toml` holds the deterministic metrics.
//! The gate discovers every public function and Kani harness of the decision core from the source, so a list that leaves something out can't let it escape proof.

use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use rules::obligation_rules::{self, Better, Pilot, Reason, Standing, Subject, Verdict};
use schemars::JsonSchema;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use syn::visit::Visit;

/// The manifest of every verification obligation.
pub const MANIFEST: &str = "policy/obligations.toml";

/// The recorded deterministic metrics.
pub const BASELINES: &str = "policy/baselines.toml";

/// Where `just gen` writes the JSON Schema of the two files.
pub const MANIFEST_SCHEMA: &str = "schemas/obligations.schema.json";
pub const BASELINES_SCHEMA: &str = "schemas/baselines.schema.json";

/// The directory of the decision core's rule modules.
pub const CORE_SOURCES: &str = "crates/rules/src";

/// The directories whose Rust sources the gate reads for callers and unsafe sites.
const SOURCE_ROOTS: [&str; 2] = ["crates", "xtask"];

/// The mutation configuration, whose exclusions count as exemptions.
pub const MUTANTS_CONFIG: &str = ".cargo/mutants.toml";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default, rename = "obligation")]
    pub obligations: Vec<Obligation>,
    #[serde(default, rename = "pilot")]
    pub pilots: Vec<PilotEntry>,
}

/// The flags a Kani contract run may pass, pinned with the Kani version in `skill_proofs::KANI_VERSION`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum KaniFlag {
    FunctionContracts,
    LoopContracts,
    Stubbing,
}

impl KaniFlag {
    #[must_use]
    pub const fn argument(self) -> &'static str {
        match self {
            Self::FunctionContracts => "function-contracts",
            Self::LoopContracts => "loop-contracts",
            Self::Stubbing => "stubbing",
        }
    }
}

/// The linters a `lint` obligation may name.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum LintTool {
    Shellcheck,
    Psscriptanalyzer,
}

/// One obligation and the verifier that discharges it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "verifier", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Obligation {
    /// Every harness in `source` must pass, and every harness in `probes` must fail on an assertion.
    Kani {
        source: String,
        /// Harness definitions placed after `mod production;`, which names the module under proof.
        #[serde(default)]
        probes: String,
    },
    /// A function contract in `source`, proved by its `proof_for_contract` harness under `flags`.
    KaniContract {
        source: String,
        function: String,
        flags: Vec<KaniFlag>,
    },
    /// A vendored JSON Schema, which must keep the recorded digest.
    JsonSchema { schema: String, sha256: String },
    /// A linter run with its committed settings.
    Lint { tool: LintTool, settings: String },
}

/// A tool tried on named subjects for one issue, which discharges no obligation.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PilotEntry {
    pub tool: String,
    pub subjects: Vec<String>,
    /// What result adopts the tool.
    pub exit: String,
    pub issue: u32,
    /// The decision record that adopts, rejects, or defers the tool.
    #[serde(default)]
    pub decision: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Baselines {
    #[serde(default, rename = "metric")]
    pub metrics: Vec<Metric>,
}

/// One deterministic metric and its recorded value.
/// Wall time appears only as an advisory note, so no entry can gate on it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "metric", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Metric {
    /// The Kani harnesses of the decision core.
    Harnesses { exact: u64 },
    /// The rejecting probes of the decision core.
    Probes { exact: u64 },
    /// The unsafe blocks, functions, and implementations of the workspace.
    UnsafeSites { exact: u64 },
    /// A wall time measured on one host, which never gates.
    WallMillis { advisory: u64 },
}

/// The JSON Schema of the manifest.
#[must_use]
pub fn manifest_schema() -> String {
    pretty(&schemars::schema_for!(Manifest))
}

/// The JSON Schema of the baselines.
#[must_use]
pub fn baselines_schema() -> String {
    pretty(&schemars::schema_for!(Baselines))
}

fn pretty(schema: &schemars::Schema) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(schema).unwrap_or_default()
    )
}

/// Writes both schemas, or with `check` refuses a stale one.
///
/// # Errors
///
/// Returns an error when writing fails, or with `check` when a committed schema differs from its type.
pub fn schemas(root: &Path, check: bool) -> Result<()> {
    for (path, wanted) in [
        (MANIFEST_SCHEMA, manifest_schema()),
        (BASELINES_SCHEMA, baselines_schema()),
    ] {
        let file = root.join(path);
        if check {
            ensure!(
                fs::read_to_string(&file).is_ok_and(|current| current == wanted),
                "{path} differs from its type; run `just gen` and commit the result"
            );
        } else {
            fs::write(&file, wanted)?;
        }
    }
    Ok(())
}

/// Parses the manifest.
///
/// # Errors
///
/// Returns an error when the text holds no manifest, such as one naming a verifier outside the toolbox.
pub fn parse_manifest(text: &str) -> Result<Manifest> {
    toml::from_str(text).context("parse the obligation manifest")
}

/// Parses the baselines.
///
/// # Errors
///
/// Returns an error when the text holds no baseline file, such as one gating on wall time.
pub fn parse_baselines(text: &str) -> Result<Baselines> {
    toml::from_str(text).context("parse the metric baselines")
}

/// Reads the manifest of the checkout.
///
/// # Errors
///
/// Returns an error when the manifest file doesn't exist or doesn't parse.
pub fn manifest(root: &Path) -> Result<Manifest> {
    parse_manifest(&fs::read_to_string(root.join(MANIFEST)).context(MANIFEST)?)
}

fn has_attribute(attributes: &[syn::Attribute], path: &[&str]) -> bool {
    attributes.iter().any(|attribute| {
        let segments: Vec<String> = attribute
            .path()
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        segments == path
    })
}

/// Whether an item compiles only for tests or proofs.
fn verification_only(attributes: &[syn::Attribute]) -> bool {
    has_attribute(attributes, &["test"])
        || attributes.iter().any(|attribute| {
            attribute.path().is_ident("cfg") && {
                let text = attribute.meta.to_token_stream().to_string();
                text.contains("test") || text.contains("kani")
            }
        })
}

fn is_harness(attributes: &[syn::Attribute]) -> bool {
    has_attribute(attributes, &["kani", "proof"])
        || has_attribute(attributes, &["kani", "proof_for_contract"])
}

fn identifiers(tokens: TokenStream, found: &mut BTreeSet<String>) {
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) => {
                found.insert(ident.to_string());
            }
            TokenTree::Group(group) => identifiers(group.stream(), found),
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
}

/// The lines of an item's documentation.
fn documentation(attributes: &[syn::Attribute]) -> String {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("doc"))
        .filter_map(|attribute| match &attribute.meta {
            syn::Meta::NameValue(value) => match &value.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(text),
                    ..
                }) => Some(text.value().trim().to_owned()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// An exemption beside a subject: `Exempt as <reason>: <why>. Evidence: `<path>::<test>`. Reconsider when <condition>.`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Exemption {
    pub reason: Reason,
    /// The checkout-relative file and the test function that stand in for the obligation.
    pub evidence: (String, String),
}

/// The reason a key names, or `None` for anything outside the closed set.
#[must_use]
pub fn reason(key: &str) -> Option<Reason> {
    [
        Reason::SemanticBoundary,
        Reason::NoDecision,
        Reason::Disproportionate,
    ]
    .into_iter()
    .find(|candidate| candidate.key() == key)
}

/// The exemption a documentation text declares, if any.
///
/// # Errors
///
/// Returns an error when the text starts an exemption without a closed reason, its evidence, or its reconsideration condition.
pub fn exemption(text: &str) -> Result<Option<Exemption>> {
    let Some((_, rest)) = text.split_once("Exempt as ") else {
        return Ok(None);
    };
    let (key, rest) = rest
        .split_once(':')
        .context("an exemption names its reason as `Exempt as <reason>:`")?;
    let reason = reason(key).with_context(|| {
        format!(
            "exemption reason `{key}` is not semantic-boundary, no-decision, or disproportionate"
        )
    })?;
    let (_, rest) = rest
        .split_once("Evidence: `")
        .context("an exemption names its substitute evidence as Evidence: `<path>::<test>`")?;
    let (evidence, rest) = rest
        .split_once('`')
        .context("unterminated exemption evidence")?;
    let (path, qualified) = evidence
        .split_once("::")
        .context("exemption evidence is `<path>::<test>`")?;
    ensure!(
        rest.split_once("Reconsider when ")
            .is_some_and(|(_, condition)| !condition.trim().is_empty()),
        "an exemption names its reconsideration condition as `Reconsider when <condition>`"
    );
    let function = qualified.rsplit("::").next().unwrap_or(qualified);
    Ok(Some(Exemption {
        reason,
        evidence: (path.to_owned(), function.to_owned()),
    }))
}

/// Whether a function body branches or computes.
#[derive(Default)]
struct Decides(bool);

impl<'ast> Visit<'ast> for Decides {
    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        if matches!(
            expression,
            syn::Expr::If(_)
                | syn::Expr::Match(_)
                | syn::Expr::While(_)
                | syn::Expr::ForLoop(_)
                | syn::Expr::Loop(_)
                | syn::Expr::Try(_)
                | syn::Expr::Binary(_)
                | syn::Expr::Unary(_)
                | syn::Expr::Macro(_)
                | syn::Expr::Let(_)
        ) {
            self.0 = true;
        } else {
            syn::visit::visit_expr(self, expression);
        }
    }
}

/// One public function of a rule module.
#[derive(Debug)]
pub struct Discovered {
    pub name: String,
    pub decides: bool,
    pub exemption: Option<Exemption>,
}

/// What the gate reads from one rule module.
#[derive(Debug, Default)]
pub struct Module {
    pub subjects: Vec<Discovered>,
    pub harnesses: BTreeSet<String>,
    /// Every identifier the module's harnesses and verification helpers mention.
    pub verified: BTreeSet<String>,
    /// Every identifier its production items mention, by the item that mentions it.
    pub production: Vec<(String, BTreeSet<String>)>,
}

fn item_name(item: &syn::Item) -> String {
    match item {
        syn::Item::Fn(function) => function.sig.ident.to_string(),
        syn::Item::Const(constant) => constant.ident.to_string(),
        syn::Item::Static(value) => value.ident.to_string(),
        _ => String::new(),
    }
}

fn item_attributes(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Fn(function) => &function.attrs,
        syn::Item::Mod(module) => &module.attrs,
        syn::Item::Impl(implementation) => &implementation.attrs,
        syn::Item::Use(import) => &import.attrs,
        syn::Item::Const(constant) => &constant.attrs,
        syn::Item::Static(value) => &value.attrs,
        syn::Item::Struct(structure) => &structure.attrs,
        syn::Item::Enum(enumeration) => &enumeration.attrs,
        _ => &[],
    }
}

/// Reads one rule module's subjects, harnesses, and mentions.
///
/// # Errors
///
/// Returns an error when the source or one of its exemptions fails to parse.
pub fn module(source: &str) -> Result<Module> {
    let file = syn::parse_file(source).context("parse a rule module")?;
    let mut found = Module::default();
    for item in &file.items {
        let attributes = item_attributes(item);
        if verification_only(attributes) {
            if let syn::Item::Fn(function) = item
                && is_harness(attributes)
            {
                found.harnesses.insert(function.sig.ident.to_string());
            }
            if !has_attribute_cfg_test(attributes) {
                identifiers(item.to_token_stream(), &mut found.verified);
            }
            continue;
        }
        let mut mentioned = BTreeSet::new();
        identifiers(item.to_token_stream(), &mut mentioned);
        found.production.push((item_name(item), mentioned));
        if let syn::Item::Fn(function) = item
            && matches!(function.vis, syn::Visibility::Public(_))
        {
            let mut decides = Decides::default();
            decides.visit_block(&function.block);
            found.subjects.push(Discovered {
                name: function.sig.ident.to_string(),
                decides: decides.0,
                exemption: exemption(&documentation(&function.attrs))
                    .with_context(|| format!("exemption of `{}`", function.sig.ident))?,
            });
        }
    }
    Ok(found)
}

fn has_attribute_cfg_test(attributes: &[syn::Attribute]) -> bool {
    has_attribute(attributes, &["test"])
        || attributes.iter().any(|attribute| {
            attribute.path().is_ident("cfg")
                && attribute
                    .meta
                    .to_token_stream()
                    .to_string()
                    .contains("test")
        })
}

/// The Kani harnesses `probes` declares, which must each fail on the module.
///
/// # Errors
///
/// Returns an error when the probes fail to parse.
pub fn probe_harnesses(probes: &str) -> Result<BTreeSet<String>> {
    let file = syn::parse_file(probes).context("parse the probes")?;
    Ok(file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Fn(function) if is_harness(&function.attrs) => {
                Some(function.sig.ident.to_string())
            }
            _ => None,
        })
        .collect())
}

/// Every Rust source under the source roots, as a checkout-relative path with forward slashes.
fn rust_sources(root: &Path, directory: &Path, found: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if entry.file_type()?.is_dir() {
            if name != "target" && name != "fixtures" && name != "snapshots" {
                rust_sources(root, &path, found)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

fn workspace_sources(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for directory in SOURCE_ROOTS {
        rust_sources(root, &root.join(directory), &mut found)?;
    }
    found.sort();
    Ok(found)
}

/// Whether a path holds integration tests rather than production code.
fn is_test_path(path: &str) -> bool {
    path.split('/').any(|segment| segment == "tests")
}

/// The identifiers each production source mentions outside tests and proofs.
fn production_mentions(root: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let mut mentions = BTreeMap::new();
    for path in workspace_sources(root)? {
        if is_test_path(&path) || path.starts_with(CORE_SOURCES) || path.starts_with("xtask/proofs")
        {
            continue;
        }
        let file = syn::parse_file(&fs::read_to_string(root.join(&path))?)
            .with_context(|| format!("parse {path}"))?;
        let mut found = BTreeSet::new();
        collect_production(&file.items, &mut found);
        mentions.insert(path, found);
    }
    Ok(mentions)
}

fn collect_production(items: &[syn::Item], found: &mut BTreeSet<String>) {
    for item in items {
        if verification_only(item_attributes(item)) {
            continue;
        }
        if let syn::Item::Mod(syn::ItemMod {
            content: Some((_, inner)),
            ..
        }) = item
        {
            collect_production(inner, found);
        } else {
            identifiers(item.to_token_stream(), found);
        }
    }
}

/// Counts unsafe blocks, functions, implementations, and traits.
#[derive(Default)]
struct UnsafeSites(u64);

impl<'ast> Visit<'ast> for UnsafeSites {
    fn visit_expr_unsafe(&mut self, block: &'ast syn::ExprUnsafe) {
        self.0 += 1;
        syn::visit::visit_expr_unsafe(self, block);
    }
    fn visit_signature(&mut self, signature: &'ast syn::Signature) {
        if signature.unsafety.is_some() {
            self.0 += 1;
        }
        syn::visit::visit_signature(self, signature);
    }
    fn visit_item_impl(&mut self, implementation: &'ast syn::ItemImpl) {
        if implementation.unsafety.is_some() {
            self.0 += 1;
        }
        syn::visit::visit_item_impl(self, implementation);
    }
    fn visit_item_trait(&mut self, definition: &'ast syn::ItemTrait) {
        if definition.unsafety.is_some() {
            self.0 += 1;
        }
        syn::visit::visit_item_trait(self, definition);
    }
}

/// The unsafe sites of every workspace source, tests included.
///
/// # Errors
///
/// Returns an error when a source fails to read or parse.
pub fn unsafe_sites(root: &Path) -> Result<u64> {
    let mut sites = UnsafeSites::default();
    for path in workspace_sources(root)? {
        let file = syn::parse_file(&fs::read_to_string(root.join(&path))?)
            .with_context(|| format!("parse {path}"))?;
        sites.visit_file(&file);
    }
    Ok(sites.0)
}

/// Every Kani harness outside the modules the manifest lists, which no run would ever prove.
fn stray_harnesses(root: &Path, listed: &BTreeSet<String>) -> Result<Vec<String>> {
    #[derive(Default)]
    struct Harnesses(Vec<String>);
    impl<'ast> Visit<'ast> for Harnesses {
        fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
            if is_harness(&function.attrs) {
                self.0.push(function.sig.ident.to_string());
            }
            syn::visit::visit_item_fn(self, function);
        }
    }
    let mut found = Vec::new();
    for path in workspace_sources(root)? {
        if listed.contains(&path) {
            continue;
        }
        let file = syn::parse_file(&fs::read_to_string(root.join(&path))?)
            .with_context(|| format!("parse {path}"))?;
        let mut harnesses = Harnesses::default();
        harnesses.visit_file(&file);
        found.extend(harnesses.0.into_iter().map(|name| {
            format!("{path}: the harness `{name}` lies outside every module {MANIFEST} lists, so no run proves it; move it beside its rule in {CORE_SOURCES}")
        }));
    }
    Ok(found)
}

/// Every `#[mutants::skip]` without an exemption in its documentation.
fn unexempted_skips(root: &Path) -> Result<Vec<String>> {
    struct Skips<'a> {
        path: &'a str,
        found: Vec<String>,
        error: Option<anyhow::Error>,
    }
    impl<'ast> Visit<'ast> for Skips<'_> {
        fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
            self.check(&function.attrs, &function.sig.ident);
            syn::visit::visit_item_fn(self, function);
        }
        fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
            self.check(&function.attrs, &function.sig.ident);
            syn::visit::visit_impl_item_fn(self, function);
        }
    }
    impl Skips<'_> {
        fn check(&mut self, attributes: &[syn::Attribute], name: &syn::Ident) {
            if has_attribute(attributes, &["mutants", "skip"]) {
                match exemption(&documentation(attributes)) {
                    Ok(Some(_)) => {}
                    Ok(None) => self.found.push(format!(
                        "{}: `{name}` skips mutation without an exemption beside it",
                        self.path
                    )),
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }
    let mut found = Vec::new();
    for path in workspace_sources(root)? {
        let file = syn::parse_file(&fs::read_to_string(root.join(&path))?)
            .with_context(|| format!("parse {path}"))?;
        let mut skips = Skips {
            path: &path,
            found: Vec::new(),
            error: None,
        };
        skips.visit_file(&file);
        if let Some(error) = skips.error {
            return Err(error.context(path));
        }
        found.extend(skips.found);
    }
    Ok(found)
}

/// Every `exclude_re` entry of the mutation configuration without an exemption comment on the line before it.
///
/// # Errors
///
/// Returns an error when an exemption comment fails to parse.
pub fn unexempted_exclusions(config: &str) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut inside = false;
    let mut previous = "";
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("exclude_re") {
            inside = true;
        } else if inside && trimmed.starts_with('"') {
            if exemption(previous.trim_start_matches('#'))?.is_none() {
                found.push(format!(
                    "{MUTANTS_CONFIG}: {trimmed} excludes mutants without an exemption comment above it"
                ));
            }
        } else if trimmed.starts_with(']') {
            inside = false;
        }
        if !trimmed.is_empty() {
            previous = trimmed;
        }
    }
    Ok(found)
}

/// Whether the substitute evidence of an exemption exists: the file holds a function of that name.
fn evidence_exists(root: &Path, evidence: &(String, String)) -> bool {
    fs::read_to_string(root.join(&evidence.0))
        .is_ok_and(|text| text.contains(&format!("fn {}(", evidence.1)))
}

/// Every `*_rules.rs` module of the decision core, as a checkout-relative path.
///
/// # Errors
///
/// Returns an error when listing the directory fails.
pub fn rule_modules(root: &Path) -> Result<BTreeSet<String>> {
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(root.join(CORE_SOURCES))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.ends_with("_rules.rs") {
            found.insert(format!("{CORE_SOURCES}/{name}"));
        }
    }
    Ok(found)
}

fn stem(source: &str) -> &str {
    source
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".rs"))
        .unwrap_or(source)
}

/// The result of the closure check: the refusals and the measured metrics.
#[derive(Debug, Default)]
pub struct Closure {
    pub refusals: Vec<String>,
    pub harnesses: u64,
    pub probes: u64,
    pub subjects: u64,
    pub exempt: u64,
}

/// Holds the manifest to the source: every rule module has a Kani obligation, every subject an owner, and every verifier its evidence.
///
/// # Errors
///
/// Returns an error when reading or parsing a source or the manifest fails.
pub fn closure(root: &Path, manifest: &Manifest) -> Result<Closure> {
    let mut result = Closure::default();
    let modules = rule_modules(root)?;
    let mut listed = BTreeSet::new();
    let mentions = production_mentions(root)?;
    for obligation in &manifest.obligations {
        match obligation {
            Obligation::Kani { source, probes } => {
                if !listed.insert(source.clone()) {
                    result
                        .refusals
                        .push(format!("{MANIFEST}: {source} has two Kani obligations"));
                }
                if !modules.contains(source) {
                    result.refusals.push(format!(
                        "{MANIFEST}: {source} is not a rule module of {CORE_SOURCES}"
                    ));
                    continue;
                }
                let found = module(&fs::read_to_string(root.join(source))?)
                    .with_context(|| source.clone())?;
                close_module(root, source, &found, &mentions, &mut result);
                let rejected = probe_harnesses(probes).with_context(|| source.clone())?;
                result.probes += u64::try_from(rejected.len())?;
                for name in rejected.intersection(&found.harnesses) {
                    result.refusals.push(format!(
                        "{source}: probe `{name}` shares its name with a harness of the module"
                    ));
                }
            }
            Obligation::KaniContract {
                source, function, ..
            } => {
                let text = fs::read_to_string(root.join(source)).unwrap_or_default();
                if !text.contains(&format!("kani::proof_for_contract({function})")) {
                    result.refusals.push(format!(
                        "{MANIFEST}: no `proof_for_contract({function})` harness in {source}"
                    ));
                }
            }
            Obligation::JsonSchema { schema, sha256 } => match fs::read(root.join(schema)) {
                Ok(bytes) if crate::build_inputs::hex(&Sha256::digest(&bytes)) == *sha256 => {}
                Ok(_) => result.refusals.push(format!(
                    "{schema} differs from the SHA-256 that {MANIFEST} records; vendor the pinned release again"
                )),
                Err(_) => result
                    .refusals
                    .push(format!("{MANIFEST}: the schema {schema} is missing")),
            },
            Obligation::Lint { settings, .. } => {
                if !root.join(settings).is_file() {
                    result.refusals.push(format!(
                        "{MANIFEST}: the lint settings {settings} are missing"
                    ));
                }
            }
        }
    }
    for missing in modules.difference(&listed) {
        result.refusals.push(format!(
            "{missing} has no Kani obligation in {MANIFEST}; add one with its probes"
        ));
    }
    result.refusals.extend(stray_harnesses(root, &listed)?);
    result.refusals.extend(unexempted_skips(root)?);
    if let Ok(config) = fs::read_to_string(root.join(MUTANTS_CONFIG)) {
        result.refusals.extend(unexempted_exclusions(&config)?);
    }
    Ok(result)
}

fn close_module(
    root: &Path,
    source: &str,
    found: &Module,
    mentions: &BTreeMap<String, BTreeSet<String>>,
    result: &mut Closure,
) {
    let stem = stem(source);
    result.harnesses += found.harnesses.len() as u64;
    if found.harnesses.is_empty() {
        result
            .refusals
            .push(format!("{source} declares no Kani harness"));
    }
    for subject in &found.subjects {
        let name = &subject.name;
        let called = mentions
            .values()
            .any(|idents| idents.contains(name) && idents.contains(stem))
            || found
                .production
                .iter()
                .any(|(item, idents)| item != name && idents.contains(name));
        let facts = Subject {
            proved: found.verified.contains(name),
            exemption: subject.exemption.as_ref().map(|exemption| exemption.reason),
            decides: subject.decides,
            pure: true,
            evidence: subject
                .exemption
                .as_ref()
                .is_some_and(|exemption| evidence_exists(root, &exemption.evidence)),
            called,
        };
        result.subjects += 1;
        let refusal = match obligation_rules::verdict(facts) {
            Verdict::Proved => continue,
            Verdict::Exempt => {
                result.exempt += 1;
                continue;
            }
            Verdict::Unowned => "has no harness and no exemption beside it",
            Verdict::StaleExemption => {
                "carries an exemption that no longer holds: a harness proves it, it decides, the core is pure, or its evidence is gone"
            }
            Verdict::Uncalled => {
                "is proved but no production code calls it; call it from its adapter or delete it"
            }
        };
        result
            .refusals
            .push(format!("{source}: `{name}` {refusal}"));
    }
}

/// The state of a pilot's issue: `true` once it closed.
pub type IssueState<'a> = &'a dyn Fn(u32) -> Result<bool>;

/// Reads an issue's state through the GitHub CLI.
///
/// # Errors
///
/// Returns an error when the GitHub CLI can't read the issue.
pub fn github_issue_closed(issue: u32) -> Result<bool> {
    let output = Tool::Gh
        .command()
        .args([
            "issue",
            "view",
            &issue.to_string(),
            "--json",
            "state",
            "--jq",
            ".state",
        ])
        .output()
        .context("start the GitHub CLI")?;
    ensure!(
        output.status.success(),
        "the GitHub CLI can't read issue {issue}; run `gh auth login`: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8(output.stdout)?.trim() == "CLOSED")
}

/// Refusals for pilots whose issue closed without a decision record.
///
/// # Errors
///
/// Returns an error when reading an issue's state fails.
pub fn pilots(root: &Path, manifest: &Manifest, closed: IssueState<'_>) -> Result<Vec<String>> {
    let mut refusals = Vec::new();
    for entry in &manifest.pilots {
        let decided = entry
            .decision
            .as_ref()
            .is_some_and(|record| root.join(record).is_file());
        if entry.decision.is_some() && !decided {
            refusals.push(format!(
                "{MANIFEST}: the decision record of the {} pilot is missing",
                entry.tool
            ));
        }
        if obligation_rules::pilot(closed(entry.issue)?, decided) == Pilot::Undecided {
            refusals.push(format!(
                "the {} pilot's issue {} closed without a decision record; record whether to adopt, reject, or defer it and name the record in {MANIFEST}",
                entry.tool, entry.issue
            ));
        }
    }
    Ok(refusals)
}

/// Refusals for every metric that differs from its baseline.
#[must_use]
pub fn metrics(baselines: &Baselines, measured: &Closure, unsafe_sites: u64) -> Vec<String> {
    let mut refusals = Vec::new();
    for metric in &baselines.metrics {
        let (name, better, baseline, value) = match *metric {
            Metric::Harnesses { exact } => ("harnesses", Better::Higher, exact, measured.harnesses),
            Metric::Probes { exact } => ("probes", Better::Higher, exact, measured.probes),
            Metric::UnsafeSites { exact } => ("unsafe-sites", Better::Lower, exact, unsafe_sites),
            Metric::WallMillis { .. } => continue,
        };
        match obligation_rules::standing(better, baseline, value) {
            Standing::Holds => {}
            Standing::Regressed => refusals.push(format!(
                "{BASELINES}: {name} went from {baseline} to {value}; restore it instead of moving the baseline"
            )),
            Standing::Improved => refusals.push(format!(
                "{BASELINES}: {name} improved from {baseline} to {value}; record {value} so it can't slip back"
            )),
        }
    }
    refusals
}

/// The closure gate of `just check`: the manifest, the pilots, and the metrics.
///
/// # Errors
///
/// Returns an error listing every refusal, or when reading an input fails.
pub fn check(root: &Path, closed: IssueState<'_>) -> Result<()> {
    schemas(root, true)?;
    let manifest = manifest(root)?;
    let baselines = parse_baselines(&fs::read_to_string(root.join(BASELINES)).context(BASELINES)?)?;
    let measured = closure(root, &manifest)?;
    let mut refusals = measured.refusals.clone();
    refusals.extend(pilots(root, &manifest, closed)?);
    refusals.extend(metrics(&baselines, &measured, unsafe_sites(root)?));
    if !refusals.is_empty() {
        bail!(
            "the obligation manifest doesn't cover the code:\n{}",
            refusals.join("\n")
        );
    }
    println!(
        "Every one of {} subjects has an owner ({} exempt beside the code), {} harnesses and {} probes are listed, and every metric holds its baseline",
        measured.subjects, measured.exempt, measured.harnesses, measured.probes
    );
    Ok(())
}
