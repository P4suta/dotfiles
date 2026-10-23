//! The Lean obligations and the vectors that bind each theorem to the Rust code.
//! The package builds with warnings as errors, each theorem rests on the three standard axioms alone, and each module replays in a fresh kernel.
//! `just gen` writes the vector files from the Rust functions, and Lean proves by evaluation that its definitions give the same results.

use crate::obligations::{self, LeanPackage, Obligation};
use crate::phase_evidence::RawResult;
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// The Lean release that the image pins by digest and the package's `lean-toolchain` names.
pub const LEAN_VERSION: &str = "4.34.1";

/// The directory that defines the image of the Lean obligations.
pub const IMAGE_DIRECTORY: &str = "xtask/proofs/lean";

/// The only axioms a theorem may rest on: propositional extensionality, choice, and quotient soundness.
pub const STANDARD_AXIOMS: [&str; 3] = ["propext", "Classical.choice", "Quot.sound"];

/// The skill vectors that `just gen` writes from `skill_plan_rules::plan`.
pub const SKILL_PLAN_VECTORS: &str = "specs/lean/Dotfiles/SkillPlanVectors.lean";

/// Where the container mounts the read-only checkout.
const SOURCE: &str = "/source";

/// The digest bytes an image name carries.
const DIGEST_BYTES: usize = 8;

/// The refusals of `#print axioms` output: every theorem that rests on an axiom outside the standard three, such as `sorryAx` or `Lean.ofReduceBool`.
#[must_use]
pub fn axiom_refusals(output: &str) -> Vec<String> {
    let mut refusals = Vec::new();
    if output.trim().is_empty() {
        refusals.push("the axiom report is empty".to_owned());
    }
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((theorem, axioms)) = axiom_report(line) else {
            refusals.push(format!("unexpected axiom report: {line}"));
            continue;
        };
        for axiom in axioms {
            if !STANDARD_AXIOMS.contains(&axiom) {
                refusals.push(format!("'{theorem}' rests on the axiom {axiom}"));
            }
        }
    }
    refusals
}

fn axiom_report(line: &str) -> Option<(&str, Vec<&str>)> {
    let (theorem, report) = line.strip_prefix('\'')?.split_once("' ")?;
    if theorem.is_empty()
        || theorem
            .chars()
            .any(|character| character.is_whitespace() || character == '\'')
    {
        return None;
    }
    if report == "does not depend on any axioms" {
        return Some((theorem, Vec::new()));
    }
    let axioms = report
        .strip_prefix("depends on axioms: [")?
        .strip_suffix(']')?;
    if axioms.is_empty() || axioms.contains(['[', ']']) {
        return None;
    }
    let parsed: Vec<_> = axioms.split(',').map(str::trim).collect();
    if parsed
        .iter()
        .any(|axiom| axiom.is_empty() || axiom.chars().any(char::is_whitespace))
    {
        return None;
    }
    Some((theorem, parsed))
}

/// Refuses incomplete, duplicate, unexpected, or unsupported theorem reports.
#[must_use]
pub fn theorem_refusals(output: &str, package: &LeanPackage) -> Vec<String> {
    let mut refusals = axiom_refusals(output);
    let expected: BTreeSet<_> = package
        .theorems
        .iter()
        .map(|theorem| theorem.name.as_str())
        .collect();
    if expected.is_empty() || expected.len() != package.theorems.len() {
        refusals.push("the theorem inventory is empty or duplicated".to_owned());
    }
    let mut reported = BTreeSet::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((name, _)) = axiom_report(line) else {
            continue;
        };
        if !expected.contains(name) {
            refusals.push(format!("the axiom report names unlisted theorem {name}"));
        }
        if !reported.insert(name) {
            refusals.push(format!("the axiom report repeats theorem {name}"));
        }
    }
    for missing in expected.difference(&reported) {
        refusals.push(format!("the axiom report omits theorem {missing}"));
    }
    refusals
}

/// The theorems a package's axiom report must name: `#print axioms <theorem>` once for each.
#[must_use]
pub fn unreported(axioms: &str, package: &LeanPackage) -> Vec<String> {
    package
        .theorems
        .iter()
        .filter(|theorem| {
            !axioms
                .lines()
                .any(|line| line.trim() == format!("#print axioms {}", theorem.name))
        })
        .map(|theorem| theorem.name.clone())
        .collect()
}

/// The image name for the Dockerfile, which changes with it.
///
/// # Errors
///
/// Returns an error when reading the Dockerfile fails.
pub fn image(root: &Path) -> Result<String> {
    let path = root.join(IMAGE_DIRECTORY).join("Dockerfile");
    let definition = fs::read(&path).with_context(|| path.display().to_string())?;
    Ok(format!(
        "dotfiles-lean:{}",
        crate::build_inputs::hex(&Sha256::digest(definition)[..DIGEST_BYTES])
    ))
}

pub(crate) fn build(root: &Path, image: &str) -> Result<()> {
    if crate::container::amd64_image_present(image)? {
        return Ok(());
    }
    let mut command = Tool::Docker.command();
    command
        .args([
            "build",
            "--platform",
            crate::container::LINUX_AMD64,
            "--tag",
            image,
        ])
        .arg(root.join(IMAGE_DIRECTORY));
    let output = crate::phase_evidence::output(&mut command, "build the pinned Lean image")?;
    ensure!(
        output.status.success(),
        "cannot build the pinned Lean image: {}\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Prepare the same pinned image for per-answer proof checks.
///
/// # Errors
///
/// Reports build failures without recording a usable prover.
pub fn prepare_image(root: &Path, image: &str) -> Result<()> {
    build(root, image)
}

/// Builds a writable package copy, prints the axiom report, and replays each module in a fresh kernel.
///
/// # Errors
///
/// Returns an error when assembling the script fails.
pub fn script(package: &LeanPackage, modules: &[String]) -> Result<String> {
    let mut script = format!(
        "set -e; cp -r {SOURCE}/{} /tmp/package; cd /tmp/package; lake build --wfail; echo AXIOMS; lake env lean Axioms.lean",
        package.package
    );
    for module in modules {
        write!(script, "; lake env leanchecker --fresh {module}")?;
    }
    Ok(script)
}

/// The arguments that run `script` in the image with no network and the checkout read-only.
#[must_use]
pub fn run_arguments(root: &Path, image: &str, script: &str) -> Vec<OsString> {
    // Docker reads `--mount` as one CSV record, so quoting keeps a comma in the path inside the field.
    let mut mount = OsString::from("type=bind,\"source=");
    mount.push(root);
    mount.push(format!("\",target={SOURCE},readonly"));
    let mut arguments: Vec<OsString> = [
        "run",
        "--rm",
        "--platform",
        crate::container::LINUX_AMD64,
        "--network",
        "none",
        "--mount",
    ]
    .map(OsString::from)
    .into();
    arguments.push(mount);
    arguments.extend(dotfiles::bounded::resource_arguments());
    arguments.extend([image, "bash", "-c", script].map(OsString::from));
    arguments
}

/// The modules whose theorems a package names, each once.
fn modules(package: &LeanPackage) -> Vec<String> {
    let mut found: Vec<String> = package
        .theorems
        .iter()
        .map(|theorem| theorem.module.clone())
        .collect();
    found.sort();
    found.dedup();
    found
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reports {
    inputs_sha256: String,
    manifest_sha256: String,
    image_tag: String,
    image_id: String,
    packages: Vec<PackageReport>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PackageReport {
    package: String,
    script: String,
    result: RawResult,
}

pub(crate) fn collect(root: &Path, manifest: &obligations::Manifest) -> Result<Reports> {
    let root = crate::canonical(root)?;
    vectors(&root, true)?;
    let image_tag = image(&root)?;
    build(&root, &image_tag)?;
    let identity = crate::verification_env::inspect(&image_tag, crate::container::LINUX_AMD64)?;
    let mut reports = Reports {
        inputs_sha256: crate::phase_evidence::model_inputs(&root)?,
        manifest_sha256: crate::build_inputs::hex(&Sha256::digest(format!("{manifest:?}"))),
        image_tag,
        image_id: identity.id,
        packages: Vec::new(),
    };
    for obligation in &manifest.obligations {
        if let Obligation::Lean(package) = obligation {
            let script = script(package, &modules(package))?;
            let mut command = Tool::Docker.command();
            command.args(run_arguments(&root, &reports.image_id, &script));
            let output = crate::phase_evidence::output(&mut command, "Lean theorem replay")?;
            reports.packages.push(PackageReport {
                package: package.package.clone(),
                script,
                result: output.into(),
            });
        }
    }
    Ok(reports)
}

pub(crate) fn validate(
    root: &Path,
    manifest: &obligations::Manifest,
    reports: &Reports,
) -> Result<()> {
    ensure!(
        reports.manifest_sha256
            == crate::build_inputs::hex(&Sha256::digest(format!("{manifest:?}"))),
        "Lean report manifest differs"
    );
    ensure!(
        reports.inputs_sha256 == crate::phase_evidence::model_inputs(root)?,
        "Lean report source inputs differ"
    );
    vectors(root, true)?;
    ensure!(
        reports.image_tag == image(root)?,
        "Lean report image definition differs"
    );
    crate::verification_env::parse_image(
        &serde_json::to_string(&serde_json::json!({
            "Id":reports.image_id, "Os":"linux", "Architecture":"amd64"
        }))?,
        crate::container::LINUX_AMD64,
    )?;
    let packages = manifest
        .obligations
        .iter()
        .filter_map(|obligation| {
            if let Obligation::Lean(package) = obligation {
                Some(package)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    ensure!(
        packages.len() == reports.packages.len(),
        "Lean package report inventory is incomplete or unexpected"
    );
    let mut proved = 0;
    for (package, report) in packages.iter().zip(&reports.packages) {
        ensure!(
            report.package == package.package
                && report.script == script(package, &modules(package))?,
            "Lean report operation differs from the current manifest"
        );
        let stdout = String::from_utf8_lossy(&report.result.stdout);
        ensure!(
            report.result.success(),
            "the Lean package {} failed: {:?}\n{stdout}{}",
            package.package,
            report.result.status,
            String::from_utf8_lossy(&report.result.stderr)
        );
        let report = stdout.split_once("AXIOMS").map_or("", |(_, report)| report);
        let refusals = theorem_refusals(
            &report
                .lines()
                .filter(|line| line.contains("axioms"))
                .collect::<Vec<_>>()
                .join("\n"),
            package,
        );
        ensure!(
            refusals.is_empty(),
            "a theorem rests on an axiom outside {STANDARD_AXIOMS:?}:\n{}",
            refusals.join("\n")
        );
        proved += package.theorems.len();
    }
    println!(
        "Proved all {proved} Lean theorems offline with Lean {LEAN_VERSION}: each builds without a warning, rests on the standard axioms alone, and replays in a fresh kernel"
    );
    Ok(())
}

/// Discharges every Lean obligation of the manifest in the pinned image, offline.
///
/// # Errors
///
/// Returns an error when Docker or Lean fails, the build warns, a theorem uses another axiom, or fresh replay fails.
pub fn verify(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    validate(root, manifest, &collect(root, manifest)?)
}

/// One skill plan the vectors pin: a catalog, the skills it requests, and what `skill_plan_rules::plan` answers.
struct Vector {
    name: String,
    catalog: String,
    count: usize,
    requested: Vec<usize>,
    planned: String,
}

fn list(items: &[usize]) -> String {
    format!(
        "[{}]",
        items
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn catalog(name: &str, deps: &[Vec<usize>]) -> Result<String> {
    let mut text = format!("def {name} : Deps\n");
    for (skill, needs) in deps
        .iter()
        .enumerate()
        .filter(|(_, needs)| !needs.is_empty())
    {
        writeln!(text, "  | {skill} => {}", list(needs))?;
    }
    text.push_str("  | _ => []\n");
    Ok(text)
}

fn vector(name: &str, catalog: &str, deps: &[Vec<usize>], requested: Vec<usize>) -> Vector {
    let slices: Vec<&[usize]> = deps.iter().map(Vec::as_slice).collect();
    let planned = match rules::skill_plan_rules::input(&slices, &requested)
        .and_then(rules::skill_plan_rules::plan)
    {
        Ok(plan) => format!(".ok {}", list(plan.skills())),
        Err(rules::skill_plan_rules::Failure::Cycle) => ".error .cycle".to_owned(),
        Err(rules::skill_plan_rules::Failure::Unknown) => ".error .unknown".to_owned(),
    };
    Vector {
        name: name.to_owned(),
        catalog: catalog.to_owned(),
        count: deps.len(),
        requested,
        planned,
    }
}

/// A skill index that lies outside the one-skill catalog that names it, so its plan fails as unknown.
const UNLISTED_SKILL: usize = 2;

/// The skill plan vectors: the shared skill catalog for each bundle and for every skill, and catalogs with a cycle and an unknown skill.
///
/// # Errors
///
/// Returns an error when reading the catalog or policy fails.
pub fn skill_plan_vectors(root: &Path) -> Result<String> {
    let names: Vec<String> = skill_ops::catalog(&root.join("home/dot_agents/skills"))?
        .skills
        .keys()
        .cloned()
        .collect();
    let policy: skill_ops::Policy =
        skill_ops::read_json(&root.join("home/dot_config/skill-ops/policy.json"))?;
    let index = |name: &str| names.iter().position(|candidate| candidate == name);
    let mut deps = vec![Vec::new(); names.len()];
    for requirement in &policy.requirements {
        let skill = index(&requirement.skill).context("a requirement names a catalog skill")?;
        if let Some(needs) = deps.get_mut(skill) {
            *needs = requirement
                .dependencies
                .iter()
                .filter_map(|name| index(name))
                .collect();
        }
    }
    let mut vectors = Vec::new();
    for (bundle, skills) in &policy.bundles {
        let requested = skills.iter().filter_map(|name| index(name)).collect();
        vectors.push(vector(
            &format!("plan_{}", bundle.replace('-', "_")),
            "catalog",
            &deps,
            requested,
        ));
    }
    vectors.push(vector(
        "plan_every_skill",
        "catalog",
        &deps,
        (0..names.len()).collect(),
    ));
    let cyclic = vec![vec![1], vec![0]];
    let unknown = vec![vec![UNLISTED_SKILL]];
    vectors.push(vector("plan_cycle", "cyclic", &cyclic, vec![0]));
    vectors.push(vector("plan_unknown", "unknown", &unknown, vec![0]));
    let mut text = String::from(
        "import Dotfiles.SkillPlan\n\n/-!\nThe skill plans that `skill_plan_rules::plan` gives, which `just gen` writes and a Rust test holds to the Rust results.\nEach theorem checks by evaluation that the Lean definition gives the same plan.\n-/\n\nnamespace Dotfiles.SkillPlan.Vectors\n\nopen Dotfiles.SkillPlan\n\n",
    );
    text.push_str("/-- The shared skills in sorted order:");
    for (skill, name) in names.iter().enumerate() {
        write!(text, " {skill} {name}")?;
        text.push(if skill + 1 == names.len() { '.' } else { ',' });
    }
    text.push_str(" -/\n");
    text.push_str(&catalog("catalog", &deps)?);
    text.push_str("\n/-- Two skills that rely on each other. -/\n");
    text.push_str(&catalog("cyclic", &cyclic)?);
    text.push_str("\n/-- A skill that relies on a skill outside the catalog. -/\n");
    text.push_str(&catalog("unknown", &unknown)?);
    for vector in vectors {
        write!(
            text,
            "\ntheorem {} : plan {} {} {} = {} := by decide +kernel\n",
            vector.name,
            vector.catalog,
            vector.count,
            list(&vector.requested),
            vector.planned
        )?;
    }
    text.push_str("\nend Dotfiles.SkillPlan.Vectors\n");
    Ok(text)
}

/// The step order vectors: for each OS family, the run order `platform::step::sequence` derives and each step's prerequisites in that family.
///
/// # Errors
///
/// Returns an error when a family's steps can't run in order.
pub fn step_order_vectors() -> Result<String> {
    use platform::step::{Family, Step, sequence};
    let index = |step: Step| Step::ALL.iter().position(|other| *other == step);
    let mut text = String::from(
        "import Dotfiles.StepOrder\n\n/-!\nThe run order of each family that `platform::step::sequence` derives, which `just gen` writes and a Rust test holds to the Rust results.\nLean checks by evaluation that each order respects every direct prerequisite, so `direct_edges_suffice` gives every transitive one.\n-/\n\nnamespace Dotfiles.StepOrder.Vectors\n\nopen Dotfiles.StepOrder\n\n",
    );
    text.push_str("/-! The steps in declaration order:");
    for (position, step) in Step::ALL.iter().enumerate() {
        write!(text, " {position} {}", step.name())?;
        text.push(if position + 1 == Step::ALL.len() {
            '.'
        } else {
            ','
        });
    }
    text.push_str(" -/\n");
    for family in Family::ALL {
        let order = sequence(family).map_err(|disorder| {
            anyhow::anyhow!("{family:?} steps can't run in order: {disorder:?}")
        })?;
        let name = family.directory();
        writeln!(
            text,
            "\n/-- Each {name} step's prerequisites among the {name} steps. -/"
        )?;
        writeln!(text, "def {name}After : Deps")?;
        for step in &order {
            let needs: Vec<usize> = step
                .prerequisites()
                .into_iter()
                .filter(|prerequisite| order.contains(prerequisite))
                .filter_map(index)
                .collect();
            if let (Some(position), false) = (index(*step), needs.is_empty()) {
                writeln!(text, "  | {position} => {}", list(&needs))?;
            }
        }
        text.push_str("  | _ => []\n");
        let positions: Vec<usize> = order.iter().copied().filter_map(index).collect();
        writeln!(text, "\n/-- The {name} launchers in run order. -/")?;
        writeln!(text, "def {name}Order : List Nat := {}", list(&positions))?;
        writeln!(
            text,
            "\ntheorem {name}_respected : DirectlyRespected {name}After {name}Order := by decide +kernel"
        )?;
        writeln!(
            text,
            "\ntheorem {name}_transitive : ∀ x ∈ {name}Order, ∀ y, Reaches {name}After x y →\n    y ∈ {name}Order ∧ {name}Order.idxOf y < {name}Order.idxOf x :=\n  direct_edges_suffice _ _ {name}_respected"
        )?;
    }
    text.push_str("\nend Dotfiles.StepOrder.Vectors\n");
    Ok(text)
}

/// The step vectors that `just gen` writes from `platform::step::sequence`.
pub const STEP_ORDER_VECTORS: &str = "specs/lean/Dotfiles/StepOrderVectors.lean";

/// Writes the vector files, or with `check` refuses a stale one.
///
/// # Errors
///
/// Returns an error when generating or writing fails, or with `check` when a committed file differs from what the Rust code gives.
pub fn vectors(root: &Path, check: bool) -> Result<()> {
    for (path, wanted) in [
        (SKILL_PLAN_VECTORS, skill_plan_vectors(root)?),
        (STEP_ORDER_VECTORS, step_order_vectors()?),
        (
            crate::contrast_refinement::GENERATED,
            crate::contrast_refinement::translate(&fs::read_to_string(
                root.join(crate::contrast_refinement::SOURCE),
            )?)?,
        ),
        (
            crate::elapsed_refinement::GENERATED,
            crate::elapsed_refinement::translate(&fs::read_to_string(
                root.join(crate::elapsed_refinement::SOURCE),
            )?)?,
        ),
    ] {
        let file = root.join(path);
        if check {
            ensure!(
                fs::read_to_string(&file).is_ok_and(|current| current == wanted),
                "{path} differs from what the Rust code gives; run `just gen` and commit the result"
            );
        } else {
            fs::write(&file, wanted)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod report_tests {
    use super::*;
    const IMAGE_HEX_DIGITS: usize = 64;

    #[test]
    fn lean_reports_bind_every_package_script_theorem_and_terminal_outcome() -> Result<()> {
        let root = crate::canonical(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .context("workspace root")?,
        )?;
        let manifest = obligations::manifest(&root)?;
        let mut packages = Vec::new();
        for obligation in &manifest.obligations {
            if let Obligation::Lean(package) = obligation {
                let mut stdout = String::new();
                for theorem in &package.theorems {
                    writeln!(stdout, "'{}' does not depend on any axioms", theorem.name)?;
                }
                packages.push(PackageReport {
                    package: package.package.clone(),
                    script: script(package, &modules(package))?,
                    result: RawResult {
                        status: Some(0),
                        stdout: format!("AXIOMS\n{stdout}").into_bytes(),
                        stderr: Vec::new(),
                    },
                });
            }
        }
        ensure!(!packages.is_empty(), "Lean fixture inventory missing");
        let reports = Reports {
            inputs_sha256: crate::phase_evidence::model_inputs(&root)?,
            manifest_sha256: crate::build_inputs::hex(&Sha256::digest(format!("{manifest:?}"))),
            image_tag: image(&root)?,
            image_id: format!("sha256:{}", "a".repeat(IMAGE_HEX_DIGITS)),
            packages,
        };
        let decoded: Reports = serde_json::from_slice(&serde_json::to_vec(&reports)?)?;
        validate(&root, &manifest, &decoded)?;
        for mutation in 0..9 {
            let mut changed = reports.clone();
            match mutation {
                0 => {
                    changed.packages.pop();
                }
                1 => changed.packages.push(changed.packages[0].clone()),
                2 => changed.packages[0].script.clear(),
                3 => changed.packages[0].result.status = None,
                4 => changed.packages[0].result.stdout = b"AXIOMS\n".to_vec(),
                5 => changed.packages[0].result.status = Some(1),
                6 => changed.inputs_sha256.clear(),
                7 => changed.image_id.clear(),
                _ => changed.manifest_sha256.clear(),
            }
            assert!(
                validate(&root, &manifest, &changed).is_err(),
                "accepted mutation {mutation}"
            );
        }
        Ok(())
    }
}
