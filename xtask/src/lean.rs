//! The Lean obligations and the vectors that bind each theorem to the Rust code.
//! The package builds with warnings as errors, each theorem rests on the three standard axioms alone, and each module replays in a fresh kernel.
//! `just gen` writes the vector files from the Rust functions, and Lean proves by evaluation that its definitions give the same results.

use crate::obligations::{self, LeanPackage, Obligation};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Stdio;

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
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.ends_with("does not depend on any axioms") {
            continue;
        }
        let Some((theorem, axioms)) = line.split_once(" depends on axioms: ") else {
            refusals.push(format!("unexpected axiom report: {line}"));
            continue;
        };
        let axioms = axioms.trim_start_matches('[').trim_end_matches(']');
        for axiom in axioms.split(',').map(str::trim) {
            if !STANDARD_AXIOMS.contains(&axiom) {
                refusals.push(format!("{theorem} rests on the axiom {axiom}"));
            }
        }
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

fn build(root: &Path, image: &str) -> Result<()> {
    let present = Tool::Docker
        .command()
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if present.success() {
        return Ok(());
    }
    let status = Tool::Docker
        .command()
        .args(["build", "--tag", image])
        .arg(root.join(IMAGE_DIRECTORY))
        .status()
        .context("build the pinned Lean image")?;
    ensure!(
        status.success(),
        "cannot build the pinned Lean image: {status}"
    );
    Ok(())
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
    let mut arguments: Vec<OsString> = ["run", "--rm", "--network", "none", "--mount"]
        .map(OsString::from)
        .into();
    arguments.push(mount);
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

/// Discharges every Lean obligation of the manifest in the pinned image, offline.
///
/// # Errors
///
/// Returns an error when Docker or Lean fails, the build warns, a theorem uses another axiom, or fresh replay fails.
pub fn verify(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    let root = crate::canonical(root)?;
    let image = image(&root)?;
    build(&root, &image)?;
    let mut proved = 0;
    for obligation in &manifest.obligations {
        if let Obligation::Lean(package) = obligation {
            let output = Tool::Docker
                .command()
                .args(run_arguments(
                    &root,
                    &image,
                    &script(package, &modules(package))?,
                ))
                .output()
                .context("start the Lean container")?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            ensure!(
                output.status.success(),
                "the Lean package {} failed: {}\n{stdout}{}",
                package.package,
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
            let report = stdout.split_once("AXIOMS").map_or("", |(_, report)| report);
            let refusals = axiom_refusals(
                &report
                    .lines()
                    .filter(|line| line.contains("axioms"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            ensure!(
                refusals.is_empty(),
                "a theorem rests on an axiom outside {STANDARD_AXIOMS:?}:\n{}",
                refusals.join("\n")
            );
            proved += package.theorems.len();
        }
    }
    println!(
        "Proved all {proved} Lean theorems offline with Lean {LEAN_VERSION}: each builds without a warning, rests on the standard axioms alone, and replays in a fresh kernel"
    );
    Ok(())
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
