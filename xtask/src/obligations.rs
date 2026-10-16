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
/// The consumer schema directory.
pub const VENDORED_SCHEMAS: &str = "schemas/vendor/";

/// The directory of the decision core's rule modules.
pub const CORE_SOURCES: &str = "crates/rules/src";

/// The directories whose Rust sources the gate reads for callers and unsafe sites.
const SOURCE_ROOTS: [&str; 2] = ["crates", "xtask"];

const GITHUB_REPOSITORY: &str = "P4suta/dotfiles";
const MIN_CFG_ATTR_ARGUMENTS: usize = 2;

/// The mutation configuration, whose exclusions count as exemptions.
pub const MUTANTS_CONFIG: &str = ".cargo/mutants.toml";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default, rename = "obligation")]
    pub obligations: Vec<Obligation>,
    #[serde(default, rename = "pilot")]
    pub pilots: Vec<PilotEntry>,
    /// The mutants that run forever by construction, as cargo-mutants names them, whose timeouts the gate admits.
    #[serde(default, rename = "non-terminating")]
    pub non_terminating: Vec<NonTerminating>,
}

/// A mutant known to run forever, with the reason no test can end it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NonTerminating {
    pub mutant: String,
    pub reason: String,
}

/// The flags a Kani obligation may pass, pinned with the Kani version in `skill_proofs::KANI_VERSION`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum KaniFlag {
    FunctionContracts,
    LoopContracts,
    Quantifiers,
    Stubbing,
}

impl KaniFlag {
    #[must_use]
    pub const fn argument(self) -> &'static str {
        match self {
            Self::FunctionContracts => "function-contracts",
            Self::LoopContracts => "loop-contracts",
            Self::Quantifiers => "quantifiers",
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

impl LintTool {
    /// The settings every gate passes to this linter.
    #[must_use]
    pub const fn settings(self) -> &'static str {
        match self {
            Self::Shellcheck => crate::quality::SHELLCHECK_SETTINGS,
            Self::Psscriptanalyzer => crate::lint::SCRIPT_ANALYZER_SETTINGS,
        }
    }
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
        #[serde(default)]
        flags: Vec<KaniFlag>,
    },
    /// A function contract in `source`, proved by its `proof_for_contract` harness under `flags`.
    KaniContract {
        source: String,
        function: String,
        flags: Vec<KaniFlag>,
    },
    /// A consumer schema and its governed targets.
    JsonSchema {
        schema: String,
        sha256: String,
        targets: Vec<String>,
        #[serde(default)]
        encoded_sha256: Option<String>,
        #[serde(default)]
        resources: Vec<Vendored>,
        #[serde(default)]
        close_root: bool,
        #[serde(default)]
        undeclared: Vec<String>,
        #[serde(default)]
        extensions: Vec<SchemaExtension>,
    },
    /// A linter run with its committed settings.
    Lint {
        tool: LintTool,
        settings: String,
        probe: String,
        finding: String,
    },
    /// A Quint model, which `just proofs` checks offline in its pinned image.
    Quint(Box<QuintModel>),
    /// An Alloy 6 model, which `just proofs` checks offline in its pinned image.
    Alloy(AlloyModel),
    /// A Lean 4 package, which `just proofs` builds and checks offline in its pinned image.
    Lean(LeanPackage),
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaExtension {
    ClaudeCommandFailurePolicy,
}

/// A Lean package with an explicit implementation evidence scope for each theorem.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeanPackage {
    /// The package directory, which holds `lean-toolchain`, the lakefile, and `Axioms.lean`.
    pub package: String,
    pub theorems: Vec<LeanTheorem>,
}

/// One theorem and the bridge that binds it to the Rust code.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeanTheorem {
    /// The full name, such as `Dotfiles.SkillPlan.plan_correct`.
    pub name: String,
    /// The module that states it, such as `Dotfiles.SkillPlan`.
    pub module: String,
    /// The Kani harness or Rust test that binds it, as `<path>::<function>`.
    pub bridge: String,
    pub refinement: LeanRefinement,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LeanRefinement {
    BoundedKani {
        source: String,
        constant: String,
        value: u64,
    },
    GeneratedVectors,
    SourceTranslation,
    SelectedCases {
        entrypoint: String,
    },
}

/// Every public theorem under `directory`, as its module and short name, excluding build outputs under `.lake`.
fn lean_theorems(package: &Path, directory: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != ".lake" {
                found.extend(lean_theorems(package, &path));
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension == "lean")
        {
            let module = path
                .strip_prefix(package)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace(['\\', '/'], ".");
            let text = fs::read_to_string(&path).unwrap_or_default();
            for line in text.lines() {
                if let Some(name) = line
                    .strip_prefix("theorem ")
                    .and_then(|rest| rest.split_whitespace().next())
                {
                    found.push((module.clone(), name.to_owned()));
                }
            }
        }
    }
    found
}

/// The refusals of a Lean obligation that the source alone decides: the package and its pinned toolchain, each theorem in its module and its axiom report, and each bridge.
#[must_use]
pub fn close_lean(root: &Path, manifest: &Manifest, lean: &LeanPackage) -> Vec<String> {
    let package = root.join(&lean.package);
    let mut refusals = Vec::new();
    let names: BTreeSet<_> = lean.theorems.iter().map(|theorem| &theorem.name).collect();
    if names.is_empty() || names.len() != lean.theorems.len() {
        refusals.push(format!(
            "{MANIFEST}: the Lean theorem inventory is empty or duplicated"
        ));
    }
    let toolchain = fs::read_to_string(package.join("lean-toolchain")).unwrap_or_default();
    if toolchain.trim() != format!("leanprover/lean4:v{}", crate::lean::LEAN_VERSION) {
        refusals.push(format!(
            "{}: lean-toolchain names {}, and the image pins Lean {}",
            lean.package,
            toolchain.trim(),
            crate::lean::LEAN_VERSION
        ));
    }
    for (module, short) in lean_theorems(&package, &package) {
        if !lean
            .theorems
            .iter()
            .any(|theorem| theorem.module == module && theorem.name.ends_with(&format!(".{short}")))
        {
            refusals.push(format!(
                "{MANIFEST}: the theorem {short} of {module} names no bridge; list it with the test or harness that binds it, or make it private"
            ));
        }
    }
    let axioms = fs::read_to_string(package.join("Axioms.lean")).unwrap_or_default();
    for missing in crate::lean::unreported(&axioms, lean) {
        refusals.push(format!(
            "{}/Axioms.lean doesn't print the axioms of {missing}",
            lean.package
        ));
    }
    for theorem in &lean.theorems {
        let file = package.join(format!("{}.lean", theorem.module.replace('.', "/")));
        let short = theorem.name.rsplit('.').next().unwrap_or(&theorem.name);
        if !fs::read_to_string(&file).is_ok_and(|text| text.contains(&format!("theorem {short} ")))
        {
            refusals.push(format!(
                "{MANIFEST}: the module {} states no theorem {short}",
                theorem.module
            ));
        }
        if !lean_bridge_registered(root, manifest, theorem) {
            refusals.push(format!(
                "{MANIFEST}: the bridge {} of {} has no enabled registered verification root at its declared refinement scope",
                theorem.bridge, theorem.name
            ));
        }
    }
    refusals
}

fn lean_bridge_registered(root: &Path, manifest: &Manifest, theorem: &LeanTheorem) -> bool {
    theorem
        .bridge
        .split_once("::")
        .is_some_and(|(path, function)| {
            let modes = evidence_modes(root, &(path.to_owned(), function.to_owned()), manifest);
            match &theorem.refinement {
                LeanRefinement::BoundedKani {
                    source,
                    constant,
                    value,
                } => {
                    path == source
                        && modes.contains(&EvidenceMode::Kani)
                        && constant_value(root, source, constant) == Some(*value)
                        && evidence_uses(
                            root,
                            path,
                            function,
                            manifest,
                            EvidenceMode::Kani,
                            constant,
                            EvidenceUse::Reference,
                        )
                }
                LeanRefinement::GeneratedVectors => {
                    modes.contains(&EvidenceMode::Test)
                        && evidence_uses(
                            root,
                            path,
                            function,
                            manifest,
                            EvidenceMode::Test,
                            "xtask::lean::vectors",
                            EvidenceUse::Call,
                        )
                }
                LeanRefinement::SourceTranslation if theorem.module == "Dotfiles.Elapsed" => {
                    crate::elapsed_refinement::THEOREMS.contains(&theorem.name.as_str())
                        && theorem.bridge == crate::elapsed_refinement::BRIDGE
                        && modes.contains(&EvidenceMode::Test)
                        && evidence_modes(
                            root,
                            &(
                                crate::elapsed_refinement::SOURCE.to_owned(),
                                crate::elapsed_refinement::HARNESS.to_owned(),
                            ),
                            manifest,
                        )
                        .contains(&EvidenceMode::Kani)
                        && crate::elapsed_refinement::with_source(root, || Ok(())).is_ok()
                }
                LeanRefinement::SourceTranslation => {
                    modes.contains(&EvidenceMode::Test)
                        && evidence_uses(
                            root,
                            path,
                            function,
                            manifest,
                            EvidenceMode::Test,
                            "xtask::contrast_refinement::translate",
                            EvidenceUse::Call,
                        )
                        && ["SOURCE", "GENERATED"].iter().all(|name| {
                            evidence_uses(
                                root,
                                path,
                                function,
                                manifest,
                                EvidenceMode::Test,
                                &format!("xtask::contrast_refinement::{name}"),
                                EvidenceUse::Reference,
                            )
                        })
                }
                LeanRefinement::SelectedCases { entrypoint } => {
                    modes.contains(&EvidenceMode::Test)
                        && evidence_uses(
                            root,
                            path,
                            function,
                            manifest,
                            EvidenceMode::Test,
                            entrypoint,
                            EvidenceUse::Call,
                        )
                }
            }
        })
}

/// The Boolean satisfiability solvers an Alloy obligation may pin, each running in Java without a native library.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AlloySolver {
    Sat4j,
}

impl AlloySolver {
    #[must_use]
    pub const fn argument(self) -> &'static str {
        match self {
            Self::Sat4j => "sat4j",
        }
    }
}

/// An Alloy model whose every check pairs an assertion, an instance, and a counterexample over a mutated fact.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlloyModel {
    pub spec: String,
    pub solver: AlloySolver,
    pub checks: Vec<AlloyCheck>,
}

/// One property of an Alloy model, as three named commands that share one scope.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlloyCheck {
    /// A `check` command that must find no counterexample.
    pub assertion: String,
    /// A `run` command that must find an instance, ruling out a vacuous assertion.
    pub instance: String,
    /// A `check` command over a mutated fact that must find a counterexample, which shows the assertion can fail.
    pub counterexample: String,
    /// The scope each of the three commands states, such as `for 4 but 12 steps`.
    pub scope: String,
}

/// The refusals of an Alloy obligation that the source alone decides: each command exists with its kind and the recorded scope.
#[must_use]
pub fn close_alloy(root: &Path, alloy: &AlloyModel) -> Vec<String> {
    let spec = &alloy.spec;
    let text = fs::read_to_string(root.join(spec)).unwrap_or_default();
    let mut refusals = Vec::new();
    if alloy.checks.is_empty() {
        refusals.push(format!("{MANIFEST}: {spec} names no check"));
    }
    for check in &alloy.checks {
        for (name, kind) in [
            (&check.assertion, "check"),
            (&check.instance, "run"),
            (&check.counterexample, "check"),
        ] {
            let declared = text.lines().any(|line| {
                line.trim_start().starts_with(&format!("{name}: {kind} "))
                    && line.trim_end().ends_with(&check.scope)
            });
            if !declared {
                refusals.push(format!(
                    "{MANIFEST}: {spec} declares no `{name}: {kind}` command {}",
                    check.scope
                ));
            }
        }
    }
    refusals
}

/// The refusals of a lint obligation: missing settings, settings no gate reads, or no probe.
fn close_lint(
    root: &Path,
    tool: LintTool,
    settings: &str,
    probe: &str,
    finding: &str,
) -> Vec<String> {
    let mut refusals = Vec::new();
    if !root.join(settings).is_file() {
        refusals.push(format!(
            "{MANIFEST}: the lint settings {settings} are missing"
        ));
    }
    let read = tool.settings();
    if settings != read {
        refusals.push(format!("{MANIFEST}: the gates read {read}, not {settings}"));
    }
    if probe.trim().is_empty() || finding.trim().is_empty() {
        refusals.push(format!(
            "{MANIFEST}: the {settings} obligation has no probe, so nothing shows the linter can fail"
        ));
    }
    refusals
}

/// A vendored schema file and the SHA-256 it must keep.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vendored {
    pub schema: String,
    pub sha256: String,
    #[serde(default)]
    pub encoded_sha256: Option<String>,
}

/// Reads the original consumer bytes and checks both digests for a compressed schema.
///
/// # Errors
///
/// Returns an error for a missing file, an invalid archive, or missing archive metadata.
pub fn vendored_bytes(root: &Path, vendored: &Vendored) -> Result<(bool, Vec<u8>)> {
    use std::io::Read as _;
    let bytes = fs::read(root.join(&vendored.schema))?;
    let digest = |bytes: &[u8]| crate::build_inputs::hex(&Sha256::digest(bytes));
    if crate::has_extension(&vendored.schema, "gz") {
        let encoded = vendored
            .encoded_sha256
            .as_ref()
            .context("a compressed schema needs its archive digest")?;
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice()).read_to_end(&mut decoded)?;
        Ok((
            digest(&bytes) == *encoded && digest(&decoded) == vendored.sha256,
            decoded,
        ))
    } else {
        ensure!(
            vendored.encoded_sha256.is_none(),
            "an uncompressed schema names an archive digest"
        );
        Ok((digest(&bytes) == vendored.sha256, bytes))
    }
}

/// The refusals for a vendored schema: one outside the vendor directory, a missing file, or a digest that moved.
fn close_vendored(root: &Path, vendored: &Vendored) -> Vec<String> {
    let schema = &vendored.schema;
    let mut refusals = Vec::new();
    if !schema.starts_with(VENDORED_SCHEMAS) {
        refusals.push(format!(
            "{MANIFEST}: {schema} lies outside {VENDORED_SCHEMAS}; a serde type owns the data this repository defines, so only a consumer's vendored schema governs a target"
        ));
    }
    match vendored_bytes(root, vendored) {
        Ok((true, _)) => {}
        Ok(_) => refusals.push(format!(
            "{schema} differs from the SHA-256 that {MANIFEST} records; vendor the pinned release again"
        )),
        Err(_) => refusals.push(format!("{MANIFEST}: the schema {schema} is missing")),
    }
    refusals
}

/// A Quint model whose `main` module holds `invariant` in exactly `distinct_states` reachable states and whose `probe` module breaks it.
/// A simulation at `seed` must witness every name in `witnesses` and write the committed `traces` again.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuintModel {
    pub spec: String,
    pub main: String,
    pub probe: String,
    pub invariant: String,
    pub witnesses: Vec<String>,
    pub seed: String,
    pub samples: u32,
    pub steps: u32,
    pub distinct_states: u64,
    pub traces: Vec<String>,
    /// The committed reachable state graph, which `TLC` must dump again and a test replays edge for edge.
    #[serde(default)]
    pub graph: Option<String>,
    /// An inductive invariant that Apalache proves for every limit the model chooses, which a bounded Apalache run of `quint::APALACHE_STEPS` steps backs.
    #[serde(default)]
    pub inductive: Option<String>,
    /// The module and constant initializer of a symbolic Apalache check.
    #[serde(default)]
    pub symbolic: Option<SymbolicQuint>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolicQuint {
    pub main: String,
    pub constants: String,
}

/// The refusals of a Quint obligation that the source alone decides: its modules, witnesses, and traces.
fn close_quint(root: &Path, quint: &QuintModel) -> Vec<String> {
    let mut refusals = Vec::new();
    let spec = &quint.spec;
    let text = fs::read_to_string(root.join(spec)).unwrap_or_default();
    for module in [&quint.main, &quint.probe]
        .into_iter()
        .chain(quint.symbolic.iter().map(|symbolic| &symbolic.main))
    {
        if !text.contains(&format!("module {module} {{")) {
            refusals.push(format!("{MANIFEST}: {spec} declares no module {module}"));
        }
    }
    if quint.symbolic.is_some() && quint.inductive.is_none() {
        refusals.push(format!(
            "{MANIFEST}: {spec} names symbolic constants without an inductive invariant"
        ));
    }
    if quint.witnesses.is_empty() {
        refusals.push(format!(
            "{MANIFEST}: {spec} names no witness, so nothing shows its runs reach an outcome"
        ));
    }
    for trace in &quint.traces {
        if !root.join(trace).is_file() {
            refusals.push(format!("{MANIFEST}: the trace {trace} is missing"));
        }
    }
    if let Some(graph) = &quint.graph
        && !root.join(graph).is_file()
    {
        refusals.push(format!("{MANIFEST}: the state graph {graph} is missing"));
    }
    refusals
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
    /// The public functions of the decision core that a Kani harness proves, which merging two harnesses of one decision leaves unchanged.
    ProvedSubjects { exact: u64 },
    /// The rejecting probes of the decision core.
    Probes { exact: u64 },
    /// The unsafe blocks, functions, and implementations of the workspace.
    UnsafeSites { exact: u64 },
    /// The mutants of the decision core that its tests let survive, which `just proofs` measures.
    MutantsMissed { exact: u64 },
    /// The mutants of the decision core that timed out without a non-terminating entry, which `just proofs` measures.
    MutantsTimedOut { exact: u64 },
    /// A wall time measured on one host, which never gates.
    WallMillis { advisory: u64 },
    /// The lowest text contrast in thousandths and the palette pair that reaches it.
    MinimumContrast { exact: u64, pair: String },
    /// The external programs a shell starts while reading its startup files.
    ShellExecCount { shell: String, exact: u64 },
    /// The gate processes one Git hook spawns, which `crates/dotfiles/tests/hook_exec_count.rs` counts through stand-ins.
    ExecCount { hook: String, exact: u64 },
    /// The instructions a benchmark executes under Valgrind.
    Ir { bench: String, exact: u64 },
    /// The bytes live at a benchmark's peak heap usage.
    HeapPeak { bench: String, exact: u64 },
    /// The bytes of the stripped binary that every host installs.
    BinarySize { exact: u64 },
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

/// The names that `tokens` call, as an identifier right before a parenthesized group, macro arguments included.
/// A local variable that shares a function's name never counts, because nothing calls it.
fn calls(tokens: TokenStream, found: &mut BTreeSet<String>) {
    let mut previous: Option<String> = None;
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) => previous = Some(ident.to_string()),
            TokenTree::Group(group) => {
                if group.delimiter() == proc_macro2::Delimiter::Parenthesis
                    && let Some(name) = previous.take()
                {
                    found.insert(name);
                }
                calls(group.stream(), found);
                previous = None;
            }
            TokenTree::Punct(_) | TokenTree::Literal(_) => previous = None,
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
    Ok(Some(Exemption {
        reason,
        evidence: (path.to_owned(), qualified.to_owned()),
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
            harnesses(item, "", &mut found.harnesses);
            if compiles_for_kani(attributes) {
                calls(item.to_token_stream(), &mut found.verified);
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

/// Whether a verification-only item also compiles under Kani, so what it mentions counts as proved.
fn compiles_for_kani(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && attribute
                .meta
                .to_token_stream()
                .to_string()
                .contains("kani")
    })
}

/// The Kani harnesses an item declares, named by the paths in compiler metadata.
fn harnesses(item: &syn::Item, prefix: &str, found: &mut BTreeSet<String>) {
    match item {
        syn::Item::Fn(function) if is_harness(&function.attrs) => {
            found.insert(format!("{prefix}{}", function.sig.ident));
        }
        syn::Item::Mod(syn::ItemMod {
            ident,
            content: Some((_, items)),
            ..
        }) => {
            let inner = format!("{prefix}{ident}::");
            for item in items {
                harnesses(item, &inner, found);
            }
        }
        _ => {}
    }
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
    // The comment lines right before an entry, one sentence per line, read as one exemption.
    let mut comment = String::new();
    for line in config.lines() {
        let trimmed = line.trim();
        if let Some(text) = trimmed.strip_prefix('#') {
            comment.push_str(text);
            comment.push(' ');
            continue;
        }
        if trimmed.starts_with("exclude_re") {
            inside = true;
        } else if inside && trimmed.starts_with('"') {
            if exemption(&comment)?.is_none() {
                found.push(format!(
                    "{MUTANTS_CONFIG}: {trimmed} excludes mutants without an exemption comment above it"
                ));
            }
        } else if trimmed.starts_with(']') {
            inside = false;
        }
        comment.clear();
    }
    Ok(found)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum EvidenceMode {
    Test,
    Kani,
}

struct EvidenceFunction {
    name: String,
    available: BTreeSet<EvidenceMode>,
    roots: BTreeSet<EvidenceMode>,
    calls: BTreeSet<String>,
    references: BTreeSet<String>,
}

#[derive(Default)]
struct EvidenceCalls {
    calls: BTreeSet<String>,
    references: BTreeSet<String>,
    shadowed: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for EvidenceCalls {
    fn visit_item(&mut self, _: &'ast syn::Item) {}
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}

    fn visit_expr_path(&mut self, expression: &'ast syn::ExprPath) {
        if expression.qself.is_none() && expression.path.leading_colon.is_none() {
            let name = expression
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if !self
                .shadowed
                .contains(name.split("::").next().unwrap_or(&name))
            {
                self.references.insert(name);
            }
        }
        syn::visit::visit_expr_path(self, expression);
    }

    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = expression.func.as_ref()
            && path.qself.is_none()
            && path.path.leading_colon.is_none()
        {
            let name = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if !self
                .shadowed
                .contains(name.split("::").next().unwrap_or(&name))
            {
                self.calls.insert(name);
            }
        }
        syn::visit::visit_expr_call(self, expression);
    }

    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        self.visit_expr(&expression.cond);
        match expression.cond.as_ref() {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Bool(value),
                ..
            }) if value.value => self.visit_block(&expression.then_branch),
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Bool(value),
                ..
            }) if !value.value => {
                if let Some((_, branch)) = &expression.else_branch {
                    self.visit_expr(branch);
                }
            }
            _ => {}
        }
    }

    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        self.visit_expr(&expression.expr);
    }

    fn visit_expr_while(&mut self, expression: &'ast syn::ExprWhile) {
        self.visit_expr(&expression.cond);
    }

    fn visit_expr_loop(&mut self, _: &'ast syn::ExprLoop) {}
    fn visit_expr_match(&mut self, expression: &'ast syn::ExprMatch) {
        self.visit_expr(&expression.expr);
    }

    fn visit_macro(&mut self, expression: &'ast syn::Macro) {
        if expression
            .path
            .get_ident()
            .is_some_and(|name| self.shadowed.contains(&name.to_string()))
        {
            return;
        }
        if !["assert", "assert_eq", "assert_ne"]
            .iter()
            .any(|name| expression.path.is_ident(name))
        {
            return;
        }
        if let Ok(arguments) = syn::parse::Parser::parse2(
            syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
            expression.tokens.clone(),
        ) {
            for argument in &arguments {
                self.visit_expr(argument);
            }
        }
    }
}

#[derive(Default)]
struct EvidenceBindings(BTreeSet<String>, usize);

impl<'ast> Visit<'ast> for EvidenceBindings {
    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.0.insert(pattern.ident.to_string());
        syn::visit::visit_pat_ident(self, pattern);
    }

    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        self.0.insert(function.sig.ident.to_string());
    }

    fn visit_item_use(&mut self, _: &'ast syn::ItemUse) {
        self.1 += 1;
    }

    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if let Some(name) = &item.ident {
            self.0.insert(name.to_string());
        }
    }
}

fn use_names(tree: &syn::UseTree, prefix: &str, imported: &mut BTreeMap<String, String>) {
    match tree {
        syn::UseTree::Path(path) => {
            use_names(&path.tree, &format!("{prefix}{}::", path.ident), imported);
        }
        syn::UseTree::Name(name) => {
            imported.insert(name.ident.to_string(), format!("{prefix}{}", name.ident));
        }
        syn::UseTree::Rename(name) => {
            imported.insert(name.rename.to_string(), format!("{prefix}{}", name.ident));
        }
        syn::UseTree::Group(group) => {
            for entry in &group.items {
                use_names(entry, prefix, imported);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

fn evidence_calls(function: &syn::ItemFn, imports: &BTreeMap<String, String>) -> EvidenceCalls {
    let mut bindings = EvidenceBindings::default();
    for input in &function.sig.inputs {
        bindings.visit_fn_arg(input);
    }
    bindings.visit_block(&function.block);
    let mut imports = imports.clone();
    let mut local_imports = 0;
    for statement in &function.block.stmts {
        if let syn::Stmt::Item(syn::Item::Use(imported)) = statement {
            use_names(&imported.tree, "", &mut imports);
            local_imports += 1;
        }
    }
    let mut found = EvidenceCalls {
        shadowed: bindings.0,
        ..EvidenceCalls::default()
    };
    if bindings.1 != local_imports {
        return found;
    }
    found.visit_block(&function.block);
    for names in [&mut found.calls, &mut found.references] {
        *names = names
            .iter()
            .map(|name| {
                let (head, tail) = name
                    .split_once("::")
                    .map_or((name.as_str(), ""), |(head, tail)| (head, tail));
                imports.get(head).map_or_else(
                    || name.clone(),
                    |imported| {
                        if tail.is_empty() {
                            imported.clone()
                        } else {
                            format!("{imported}::{tail}")
                        }
                    },
                )
            })
            .collect();
    }
    found
}

fn cfg_value(meta: &syn::Meta, mode: EvidenceMode) -> Option<bool> {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => Some(mode == EvidenceMode::Test),
        syn::Meta::Path(path) if path.is_ident("kani") => Some(mode == EvidenceMode::Kani),
        syn::Meta::Path(path) if path.is_ident("miri") => Some(false),
        syn::Meta::List(list) => {
            let entries = list
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .ok()?;
            let values: Vec<_> = entries.iter().map(|entry| cfg_value(entry, mode)).collect();
            if list.path.is_ident("not") && values.len() == 1 {
                return values[0].map(|value| !value);
            }
            if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    return Some(true);
                }
                return (!values.contains(&None)).then_some(false);
            }
            if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    return Some(false);
                }
                return (!values.contains(&None)).then_some(true);
            }
            None
        }
        _ => None,
    }
}

fn expand_attribute(
    attribute: &syn::Meta,
    mode: EvidenceMode,
    expanded: &mut Vec<syn::Meta>,
) -> Option<()> {
    if !attribute.path().is_ident("cfg_attr") {
        expanded.push(attribute.clone());
        return Some(());
    }
    let syn::Meta::List(list) = attribute else {
        return None;
    };
    let arguments = list
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .ok()?;
    if arguments.len() < MIN_CFG_ATTR_ARGUMENTS {
        return None;
    }
    if cfg_value(arguments.first()?, mode)? {
        for nested in arguments.iter().skip(1) {
            expand_attribute(nested, mode, expanded)?;
        }
    }
    Some(())
}

fn expanded_attributes(
    attributes: &[syn::Attribute],
    mode: EvidenceMode,
) -> Option<Vec<syn::Meta>> {
    let mut expanded = Vec::new();
    for attribute in attributes {
        expand_attribute(&attribute.meta, mode, &mut expanded)?;
    }
    Some(expanded)
}

fn has_effective_attribute(
    attributes: &[syn::Attribute],
    path: &[&str],
    mode: EvidenceMode,
) -> bool {
    expanded_attributes(attributes, mode).is_some_and(|attributes| {
        attributes.iter().any(|attribute| {
            attribute.path().segments.len() == path.len()
                && attribute
                    .path()
                    .segments
                    .iter()
                    .zip(path)
                    .all(|(segment, expected)| segment.ident == *expected)
        })
    })
}

fn available_modes(
    attributes: &[syn::Attribute],
    inherited: &BTreeSet<EvidenceMode>,
) -> BTreeSet<EvidenceMode> {
    inherited
        .iter()
        .copied()
        .filter(|mode| {
            expanded_attributes(attributes, *mode).is_some_and(|attributes| {
                attributes.iter().all(|attribute| {
                    if attribute.path().is_ident("ignore") {
                        return false;
                    }
                    if !attribute.path().is_ident("cfg") {
                        return true;
                    }
                    let syn::Meta::List(list) = attribute else {
                        return false;
                    };
                    list.parse_args::<syn::Meta>()
                        .is_ok_and(|meta| cfg_value(&meta, *mode) == Some(true))
                })
            })
        })
        .collect()
}

fn evidence_functions(
    items: &[syn::Item],
    prefix: &str,
    inherited: &BTreeSet<EvidenceMode>,
    registered: &BTreeSet<EvidenceMode>,
    inherited_imports: &BTreeMap<String, String>,
    functions: &mut Vec<EvidenceFunction>,
) {
    let mut imports = inherited_imports.clone();
    for item in items {
        if let syn::Item::Use(imported) = item {
            use_names(&imported.tree, "", &mut imports);
        }
    }
    for item in items {
        let available = available_modes(item_attributes(item), inherited);
        match item {
            syn::Item::Fn(function) if !has_attribute(&function.attrs, &["ignore"]) => {
                let mut roots = BTreeSet::new();
                if has_effective_attribute(&function.attrs, &["test"], EvidenceMode::Test)
                    && registered.contains(&EvidenceMode::Test)
                    && available.contains(&EvidenceMode::Test)
                {
                    roots.insert(EvidenceMode::Test);
                }
                if (has_effective_attribute(
                    &function.attrs,
                    &["kani", "proof"],
                    EvidenceMode::Kani,
                ) || has_effective_attribute(
                    &function.attrs,
                    &["kani", "proof_for_contract"],
                    EvidenceMode::Kani,
                )) && registered.contains(&EvidenceMode::Kani)
                    && available.contains(&EvidenceMode::Kani)
                {
                    roots.insert(EvidenceMode::Kani);
                }
                let called = evidence_calls(function, &imports);
                functions.push(EvidenceFunction {
                    name: format!("{prefix}{}", function.sig.ident),
                    available,
                    roots,
                    calls: called.calls,
                    references: called.references,
                });
            }
            syn::Item::Mod(module) => {
                if let Some((_, items)) = &module.content {
                    evidence_functions(
                        items,
                        &format!("{prefix}{}::", module.ident),
                        &available,
                        registered,
                        &imports,
                        functions,
                    );
                }
            }
            _ => {}
        }
    }
}

fn verification_source(path: &str) -> bool {
    let relative = Path::new(path);
    !relative.is_absolute()
        && relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        && relative
            .extension()
            .is_some_and(|extension| extension == "rs")
        && (path.starts_with("crates/") || path.starts_with("xtask/"))
}

fn module_source(root: &Path, path: &str, mode: EvidenceMode) -> bool {
    let relative = Path::new(path);
    let Some(parent) = relative.parent() else {
        return false;
    };
    if parent.file_name().is_none_or(|name| name != "src") {
        return false;
    }
    let Some(package) = parent.parent() else {
        return false;
    };
    let Some(cargo) = fs::read_to_string(root.join(package).join("Cargo.toml"))
        .ok()
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
    else {
        return false;
    };
    let Some(stem) = relative.file_stem().and_then(|name| name.to_str()) else {
        return false;
    };
    ["lib.rs", "main.rs"].into_iter().any(|entry| {
        let target = if entry == "lib.rs" { "autolib" } else { "autobins" };
        if cargo.get("package").is_none_or(|package| package.get(target).and_then(toml::Value::as_bool) == Some(false))
            || cargo.get("lib").is_some() || cargo.get("bin").is_some()
        { return false; }
        let file = root.join(parent).join(entry);
        fs::read_to_string(file).ok().and_then(|text| syn::parse_file(&text).ok()).is_some_and(|parsed| {
            let inherited = available_modes(&parsed.attrs, &BTreeSet::from([mode]));
            parsed.items.iter().any(|item| matches!(item, syn::Item::Mod(module) if module.ident == stem && module.content.is_none() && !has_effective_attribute(&module.attrs, &["path"], mode) && available_modes(&module.attrs, &inherited).contains(&mode)))
        })
    })
}

fn test_source(root: &Path, path: &str) -> bool {
    if !verification_source(path) {
        return false;
    }
    let relative = Path::new(path);
    let Some(parent) = relative.parent() else {
        return false;
    };
    if parent.file_name().is_some_and(|name| name == "tests") {
        let Some(package) = parent.parent() else {
            return false;
        };
        return fs::read_to_string(root.join(package).join("Cargo.toml"))
            .ok()
            .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
            .is_some_and(|table| {
                table.get("package").is_some()
                    && table
                        .get("package")
                        .and_then(|package| package.get("autotests"))
                        .and_then(toml::Value::as_bool)
                        != Some(false)
                    && table.get("test").is_none()
            });
    }
    module_source(root, path, EvidenceMode::Test)
}

fn evidence_graph(root: &Path, path: &str, manifest: &Manifest) -> Vec<EvidenceFunction> {
    if !verification_source(path) {
        return Vec::new();
    }
    let Ok(text) = fs::read_to_string(root.join(path)) else {
        return Vec::new();
    };
    let Ok(file) = syn::parse_file(&text) else {
        return Vec::new();
    };
    let mut registered = BTreeSet::new();
    if test_source(root, path) {
        registered.insert(EvidenceMode::Test);
    }
    if module_source(root, path, EvidenceMode::Kani)
        && manifest.obligations.iter().any(
            |obligation| matches!(obligation, Obligation::Kani { source, .. } if source == path),
        )
    {
        registered.insert(EvidenceMode::Kani);
    }
    let mut functions = Vec::new();
    let available = available_modes(
        &file.attrs,
        &BTreeSet::from([EvidenceMode::Test, EvidenceMode::Kani]),
    );
    evidence_functions(
        &file.items,
        "",
        &available,
        &registered,
        &BTreeMap::new(),
        &mut functions,
    );
    functions
}

fn resolved_name(scope: &str, called: &str) -> String {
    if let Some(relative) = called.strip_prefix("crate::") {
        return relative.to_owned();
    }
    if let Some(relative) = called.strip_prefix("self::") {
        return if scope.is_empty() {
            relative.to_owned()
        } else {
            format!("{scope}::{relative}")
        };
    }
    if let Some(relative) = called.strip_prefix("super::") {
        return resolved_name(
            scope.rsplit_once("::").map_or("", |(parent, _)| parent),
            &format!("self::{relative}"),
        );
    }
    if scope.is_empty() {
        called.to_owned()
    } else {
        format!("{scope}::{called}")
    }
}

fn reachable(
    functions: &[EvidenceFunction],
    start: BTreeSet<String>,
    mode: EvidenceMode,
) -> BTreeSet<String> {
    let mut reached = start;
    loop {
        let before = reached.len();
        for function in functions {
            if !reached.contains(&function.name) {
                continue;
            }
            let scope = function
                .name
                .rsplit_once("::")
                .map_or("", |(scope, _)| scope);
            for called in &function.calls {
                let name = resolved_name(scope, called);
                if functions
                    .iter()
                    .any(|candidate| candidate.name == name && candidate.available.contains(&mode))
                {
                    reached.insert(name);
                }
            }
        }
        if reached.len() == before {
            return reached;
        }
    }
}

fn evidence_modes(
    root: &Path,
    evidence: &(String, String),
    manifest: &Manifest,
) -> BTreeSet<EvidenceMode> {
    let (path, requested) = evidence;
    let functions = evidence_graph(root, path, manifest);
    let candidates: Vec<_> = functions
        .iter()
        .filter(|function| {
            function.name == *requested
                || (!requested.contains("::")
                    && function.name.rsplit("::").next() == Some(requested))
        })
        .collect();
    if candidates.len() != 1 {
        return BTreeSet::new();
    }
    let target = &candidates[0].name;
    let mut proved = BTreeSet::new();
    for mode in [EvidenceMode::Test, EvidenceMode::Kani] {
        let roots = functions
            .iter()
            .filter(|function| function.roots.contains(&mode))
            .map(|function| function.name.clone())
            .collect();
        let reached = reachable(&functions, roots, mode);
        if reached.contains(target) {
            proved.insert(mode);
        }
    }
    proved
}

#[derive(Clone, Copy)]
enum EvidenceUse {
    Call,
    Reference,
}

fn evidence_uses(
    root: &Path,
    path: &str,
    requested: &str,
    manifest: &Manifest,
    mode: EvidenceMode,
    reference: &str,
    usage: EvidenceUse,
) -> bool {
    let functions = evidence_graph(root, path, manifest);
    let names: BTreeSet<_> = functions
        .iter()
        .filter(|function| {
            function.name == requested
                || (!requested.contains("::")
                    && function.name.rsplit("::").next() == Some(requested))
        })
        .map(|function| function.name.clone())
        .collect();
    if names.len() != 1 {
        return false;
    }
    let reached = reachable(&functions, names, mode);
    functions.iter().any(|function| {
        reached.contains(&function.name)
            && match usage {
                EvidenceUse::Call => &function.calls,
                EvidenceUse::Reference => &function.references,
            }
            .iter()
            .any(|name| {
                name == reference
                    || resolved_name(
                        function
                            .name
                            .rsplit_once("::")
                            .map_or("", |(scope, _)| scope),
                        name,
                    ) == reference
            })
    })
}

fn constant_value(root: &Path, source: &str, requested: &str) -> Option<u64> {
    fn find(items: &[syn::Item], prefix: &str, requested: &str) -> Option<u64> {
        for item in items {
            match item {
                syn::Item::Const(constant)
                    if format!("{prefix}{}", constant.ident) == requested =>
                {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Int(value),
                        ..
                    }) = constant.expr.as_ref()
                    {
                        return value.base10_parse().ok();
                    }
                }
                syn::Item::Mod(module) => {
                    if let Some((_, items)) = &module.content
                        && let Some(value) =
                            find(items, &format!("{prefix}{}::", module.ident), requested)
                    {
                        return Some(value);
                    }
                }
                _ => {}
            }
        }
        None
    }
    let file = syn::parse_file(&fs::read_to_string(root.join(source)).ok()?).ok()?;
    find(&file.items, "", requested)
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
    pub proved: u64,
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
            Obligation::Kani { source, probes, .. } => {
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
                close_module(root, manifest, source, &found, &mentions, &mut result);
                let rejected = probe_harnesses(probes).with_context(|| source.clone())?;
                if rejected.is_empty() {
                    result.refusals.push(format!(
                        "{MANIFEST}: {source} has no rejecting probe, so nothing shows its harnesses can fail; add a probe that asserts a refused decision"
                    ));
                }
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
            Obligation::JsonSchema {
                schema,
                sha256,
                targets,
                resources,
                encoded_sha256,
                ..
            } => {
                if targets.is_empty() {
                    result
                        .refusals
                        .push(format!("{MANIFEST}: {schema} governs no rendered target"));
                }
                let own = Vendored {
                    schema: schema.clone(),
                    sha256: sha256.clone(),
                    encoded_sha256: encoded_sha256.clone(),
                };
                for vendored in std::iter::once(&own).chain(resources) {
                    result.refusals.extend(close_vendored(root, vendored));
                }
            }
            Obligation::Lint {
                tool,
                settings,
                probe,
                finding,
            } => result
                .refusals
                .extend(close_lint(root, *tool, settings, probe, finding)),
            Obligation::Quint(quint) => result.refusals.extend(close_quint(root, quint)),
            Obligation::Alloy(alloy) => result.refusals.extend(close_alloy(root, alloy)),
            Obligation::Lean(lean) => result.refusals.extend(close_lean(root, manifest, lean)),
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
    manifest: &Manifest,
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
            evidence: subject.exemption.as_ref().is_some_and(|exemption| {
                !evidence_modes(root, &exemption.evidence, manifest).is_empty()
            }),
            called,
        };
        result.subjects += 1;
        let refusal = match obligation_rules::verdict(facts) {
            Verdict::Proved => {
                result.proved += 1;
                continue;
            }
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
            "api",
            &format!("repos/{GITHUB_REPOSITORY}/issues/{issue}"),
            "--jq",
            "if .pull_request.merged_at then \"MERGED\" else .state | ascii_upcase end",
        ])
        .output()
        .context("start the GitHub CLI")?;
    ensure!(
        output.status.success(),
        "the GitHub CLI can't read issue {issue} in {GITHUB_REPOSITORY}: {}",
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
        if !decided && obligation_rules::pilot(closed(entry.issue)?, decided) == Pilot::Undecided {
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
    compare(baselines, |metric| match *metric {
        Metric::ProvedSubjects { exact } => {
            Some(("proved-subjects", Better::Higher, exact, measured.proved))
        }
        Metric::Probes { exact } => Some(("probes", Better::Higher, exact, measured.probes)),
        Metric::UnsafeSites { exact } => Some(("unsafe-sites", Better::Lower, exact, unsafe_sites)),
        Metric::MutantsMissed { .. }
        | Metric::MutantsTimedOut { .. }
        | Metric::WallMillis { .. }
        | Metric::MinimumContrast { .. }
        | Metric::ShellExecCount { .. }
        | Metric::ExecCount { .. }
        | Metric::Ir { .. }
        | Metric::HeapPeak { .. }
        | Metric::BinarySize { .. } => None,
    })
}

/// Refusals for every measured shell whose startup program count differs from its baseline or has none.
#[must_use]
pub fn exec_counts(baselines: &Baselines, measured: &[(&str, u64)]) -> Vec<String> {
    let mut refusals = Vec::new();
    for (shell, value) in measured {
        let recorded = baselines.metrics.iter().find_map(|metric| match metric {
            Metric::ShellExecCount {
                shell: named,
                exact,
            } if named == shell => Some(*exact),
            _ => None,
        });
        let Some(baseline) = recorded else {
            refusals.push(format!(
                "{BASELINES} records no exec-count metric for {shell}; record {value}"
            ));
            continue;
        };
        match obligation_rules::standing(Better::Lower, baseline, *value) {
            Standing::Holds => {}
            Standing::Regressed => refusals.push(format!(
                "{BASELINES}: {shell} starts {value} programs at startup instead of {baseline}; cache the initialization that forks instead of moving the baseline"
            )),
            Standing::Improved => refusals.push(format!(
                "{BASELINES}: {shell} now starts {value} programs at startup instead of {baseline}; record {value} so it can't slip back"
            )),
        }
    }
    refusals
}

/// Refusals for a lowest contrast pair that differs from the one the baselines record, or a baseline file that records none.
/// `lowest` holds the pair's name, such as `comment on bg`, and its contrast in thousandths.
#[must_use]
pub fn contrast_metric(baselines: &Baselines, lowest: (&str, u64)) -> Vec<String> {
    let (name, value) = lowest;
    let recorded: Vec<(&str, u64)> = baselines
        .metrics
        .iter()
        .filter_map(|metric| match metric {
            Metric::MinimumContrast { exact, pair } => Some((pair.as_str(), *exact)),
            _ => None,
        })
        .collect();
    let [(pair, exact)] = recorded.as_slice() else {
        return vec![format!(
            "{BASELINES} records no single minimum-contrast metric; record {name} at {value}"
        )];
    };
    let mut refusals = compare(baselines, |metric| match metric {
        Metric::MinimumContrast { exact, .. } => {
            Some(("minimum-contrast", Better::Higher, *exact, value))
        }
        _ => None,
    });
    if *pair != name && *exact == value {
        refusals.push(format!(
            "{BASELINES}: the lowest contrast pair is now {name}, not {pair}; record it"
        ));
    } else if *pair != name {
        refusals.push(format!(
            "{BASELINES}: the lowest contrast pair moved from {pair} to {name}"
        ));
    }
    refusals
}

/// What one mutation campaign over the decision core measured.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Campaign {
    pub missed: u64,
    /// The timeouts that no non-terminating entry names.
    pub timed_out: u64,
}

/// Refusals for every mutation metric that differs from its baseline, and for a baseline file without both of them.
#[must_use]
pub fn mutation_metrics(baselines: &Baselines, measured: Campaign) -> Vec<String> {
    let mut refusals = compare(baselines, |metric| match *metric {
        Metric::MutantsMissed { exact } => {
            Some(("mutants-missed", Better::Lower, exact, measured.missed))
        }
        Metric::MutantsTimedOut { exact } => Some((
            "mutants-timed-out",
            Better::Lower,
            exact,
            measured.timed_out,
        )),
        _ => None,
    });
    for (name, present) in [
        (
            "mutants-missed",
            baselines
                .metrics
                .iter()
                .any(|metric| matches!(metric, Metric::MutantsMissed { .. })),
        ),
        (
            "mutants-timed-out",
            baselines
                .metrics
                .iter()
                .any(|metric| matches!(metric, Metric::MutantsTimedOut { .. })),
        ),
    ] {
        if !present {
            refusals.push(format!("{BASELINES} records no {name} metric"));
        }
    }
    refusals
}

/// The timeouts of a campaign's `timeout.txt` that no non-terminating entry names.
#[must_use]
pub fn unexplained_timeouts(manifest: &Manifest, timeouts: &str) -> Vec<String> {
    timeouts
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            !manifest
                .non_terminating
                .iter()
                .any(|entry| entry.mutant == *line)
        })
        .map(str::to_owned)
        .collect()
}

type Measured = (&'static str, Better, u64, u64);

fn compare(baselines: &Baselines, measure: impl Fn(&Metric) -> Option<Measured>) -> Vec<String> {
    let mut refusals = Vec::new();
    for (name, better, baseline, value) in baselines.metrics.iter().filter_map(measure) {
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
    let contrasts = crate::theme::contrasts(&crate::profiles::source_data(root)?)?;
    if let Some((foreground, background, ratio, _)) = contrasts.first() {
        refusals.extend(contrast_metric(
            &baselines,
            (&format!("{foreground} on {background}"), *ratio),
        ));
    }
    if !refusals.is_empty() {
        bail!(
            "the obligation manifest doesn't cover the code:\n{}",
            refusals.join("\n")
        );
    }
    println!(
        "Every one of {} subjects has an owner, {} proved and {} exempt beside the code, the modules declare {} harnesses and {} probes, and every metric holds its baseline",
        measured.subjects, measured.proved, measured.exempt, measured.harnesses, measured.probes
    );
    Ok(())
}
