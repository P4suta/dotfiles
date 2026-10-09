//! The Quint obligations.
//! Each model typechecks, passes its tests, and holds its invariant in every reachable state, whose count the manifest records.
//! It also witnesses every outcome and writes its traces again, while its probe module breaks the invariant.

use crate::obligations::{self, Obligation, QuintModel};
use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Output, Stdio};

/// The Quint release that the image's lockfile pins.
pub const QUINT_VERSION: &str = "0.33.0";

/// The Apalache release that Quint 0.33.0 expects, whose bundled model checker visits every state.
pub const APALACHE_VERSION: &str = "0.62.1";

/// The directory that defines the image of the Quint obligations.
pub const IMAGE_DIRECTORY: &str = "xtask/proofs/quint";

/// The files whose bytes name the image, so a changed pin builds a new one.
const IMAGE_INPUTS: [&str; 3] = ["Dockerfile", "package.json", "package-lock.json"];

/// Where the container mounts the read-only checkout and the writable output directory.
pub const SOURCE: &str = "/source";
pub const OUTPUT: &str = "/out";

/// Apalache writes its run directory under the working directory, which the read-only checkout can't hold.
const WORKDIR: &str = "/tmp";

/// The digest bytes an image name carries.
const DIGEST_BYTES: usize = 8;

/// The runs that may search for the probe's violation, which the seed makes repeatable.
const PROBE_SAMPLES: &str = "1000";

/// One worker keeps the model checker's state count exact and the run repeatable.
const TLC_CONFIG: &str = r#"{"workers": 1}"#;

/// The steps of the bounded Apalache check that backs an inductive invariant.
pub const APALACHE_STEPS: &str = "16";

/// The verbosity at which Quint passes on the model checker's own report, which holds the state count.
const TLC_REPORT: &str = "3";

/// The image name for the definition files, which changes with any of them.
///
/// # Errors
///
/// Returns an error when reading a definition file fails.
pub fn image(root: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    for input in IMAGE_INPUTS {
        let path = root.join(IMAGE_DIRECTORY).join(input);
        digest.update(fs::read(&path).with_context(|| path.display().to_string())?);
    }
    Ok(format!(
        "dotfiles-quint:{}",
        crate::build_inputs::hex(&digest.finalize()[..DIGEST_BYTES])
    ))
}

/// Builds the pinned image unless it exists.
///
/// # Errors
///
/// Returns an error when Docker can't build it.
pub fn build(root: &Path, image: &str) -> Result<()> {
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
        .context("build the pinned Quint image")?;
    ensure!(
        status.success(),
        "cannot build the pinned Quint image: {status}"
    );
    Ok(())
}

/// The arguments that run `quint` in the image with no network, the checkout read-only, and `output` writable.
#[must_use]
pub fn run_arguments(root: &Path, output: &Path, image: &str, quint: &[&str]) -> Vec<OsString> {
    let mut command = vec!["quint"];
    command.extend_from_slice(quint);
    container_arguments(root, output, image, &command)
}

/// The arguments that run `command` in the image with no network, the checkout read-only, and `output` writable.
#[must_use]
pub fn container_arguments(
    root: &Path,
    output: &Path,
    image: &str,
    command: &[&str],
) -> Vec<OsString> {
    let mount = |source: &Path, target: &str, readonly: bool| {
        // Docker reads `--mount` as one CSV record, so quoting keeps a comma in the path inside the field.
        let mut mount = OsString::from("type=bind,\"source=");
        mount.push(source);
        mount.push(format!("\",target={target}"));
        if readonly {
            mount.push(",readonly");
        }
        mount
    };
    let mut arguments: Vec<OsString> = ["run", "--rm", "--network", "none", "--mount"]
        .map(OsString::from)
        .into();
    arguments.push(mount(root, SOURCE, true));
    arguments.push("--mount".into());
    arguments.push(mount(output, OUTPUT, false));
    arguments.extend(["--workdir", WORKDIR, image].map(OsString::from));
    arguments.extend(command.iter().map(OsString::from));
    arguments
}

/// The distinct states that the model checker reports in `output`.
#[must_use]
pub fn distinct_states(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        let (_, rest) = line.split_once("states generated, ")?;
        rest.strip_suffix(" distinct states found, 0 states left on queue.")?
            .parse()
            .ok()
    })
}

/// How many traces reached the witness `name` in a `quint run` report.
#[must_use]
pub fn witnessed(output: &str, name: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        let rest = line
            .strip_prefix(name)?
            .strip_prefix(" was witnessed in ")?;
        rest.split_once(' ')?.0.parse().ok()
    })
}

/// The states of a trace, which leave out the run's own metadata.
fn states(text: &str) -> Result<Value> {
    let document: Value = serde_json::from_str(text)?;
    Ok(document["states"].clone())
}

/// Whether a committed trace belongs to the model `main` rather than to its probe: `<main>.<n>.itf.json`.
fn of_model(trace: &str, main: &str) -> bool {
    Path::new(trace)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(&format!("{main}.")))
}

/// The pinned image, a writable output directory, and the checkout of one verification run.
struct Runner<'a> {
    root: &'a Path,
    output: &'a Path,
    image: String,
}

impl Runner<'_> {
    fn quint(&self, arguments: &[&str]) -> Result<Output> {
        Tool::Docker
            .command()
            .args(run_arguments(
                self.root,
                self.output,
                &self.image,
                arguments,
            ))
            .output()
            .context("start the Quint container")
    }

    /// Runs `quint` and requires success, returning its standard output.
    fn passing(&self, what: &str, arguments: &[&str]) -> Result<String> {
        let output = self.quint(arguments)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        ensure!(
            output.status.success(),
            "{what} failed: {}\n{stdout}{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(stdout)
    }

    /// The model checker with one worker on `module`, which returns Quint's output.
    fn tlc(&self, model: &str, module: &str, invariant: &str) -> Result<Output> {
        let config = format!("{OUTPUT}/tlc.json");
        self.quint(&[
            "verify",
            model,
            "--main",
            module,
            "--backend",
            "tlc",
            "--tlc-config",
            &config,
            "--invariant",
            invariant,
            "--verbosity",
            TLC_REPORT,
        ])
    }

    /// Typechecks the model, runs its tests and those of its probe, and checks the invariant in every reachable state.
    fn check(&self, model: &str, quint: &QuintModel) -> Result<()> {
        self.passing("quint typecheck", &["typecheck", model])?;
        for module in [&quint.main, &quint.probe] {
            self.passing(
                &format!("the tests of {module}"),
                &["test", model, "--backend", "typescript", "--main", module],
            )?;
        }
        let checked = self.tlc(model, &quint.main, &quint.invariant)?;
        let report = String::from_utf8_lossy(&checked.stdout);
        let found = distinct_states(&report);
        ensure!(
            checked.status.success() && found == Some(quint.distinct_states),
            "TLC found {found:?} distinct states of {} and {}, and {} records {}; review the model change, then record the new count:\n{report}",
            quint.main,
            checked.status,
            obligations::MANIFEST,
            quint.distinct_states
        );
        if let Some(inductive) = &quint.inductive {
            let main = quint
                .symbolic
                .as_ref()
                .map_or(&quint.main, |symbolic| &symbolic.main);
            let mut options = Vec::new();
            if let Some(symbolic) = &quint.symbolic {
                fs::write(
                    self.output.join("apalache.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"checker": {"cinit": symbolic.constants}}),
                    )?,
                )?;
                options.extend(["--apalache-config", "/out/apalache.json"]);
            }
            let mut arguments = vec!["verify", model, "--main", main, "--backend", "apalache"];
            arguments.extend(options);
            // Apalache proves the invariant inductive, so it holds after any number of steps, and a bounded run backs it with concrete runs.
            self.passing(
                &format!("the inductive check of {inductive} in {}", quint.main),
                &[
                    arguments.as_slice(),
                    &[
                        "--inductive-invariant",
                        inductive,
                        "--invariant",
                        &quint.invariant,
                    ],
                ]
                .concat(),
            )?;
            self.passing(
                &format!("the bounded Apalache check of {}", quint.main),
                &[
                    arguments.as_slice(),
                    &[
                        "--max-steps",
                        APALACHE_STEPS,
                        "--invariant",
                        &quint.invariant,
                    ],
                ]
                .concat(),
            )?;
        }
        let probed = self.tlc(model, &quint.probe, &quint.invariant)?;
        ensure!(
            !probed.status.success()
                && String::from_utf8_lossy(&probed.stderr).contains("found a counterexample"),
            "the model checker found no violation of {} in {}, so nothing shows the check can fail",
            quint.invariant,
            quint.probe
        );
        Ok(())
    }

    /// Simulates the model at its seed and requires a trace for every witness.
    fn witness(&self, model: &str, quint: &QuintModel) -> Result<()> {
        let samples = quint.samples.to_string();
        let steps = quint.steps.to_string();
        let mut simulate = vec![
            "run",
            model,
            "--backend",
            "typescript",
            "--main",
            &quint.main,
            "--seed",
            &quint.seed,
            "--max-samples",
            &samples,
            "--max-steps",
            &steps,
            "--invariant",
            &quint.invariant,
            "--witnesses",
        ];
        simulate.extend(quint.witnesses.iter().map(String::as_str));
        let report = self.passing(&format!("the simulation of {}", quint.main), &simulate)?;
        for name in &quint.witnesses {
            let count = witnessed(&report, name);
            ensure!(
                count.is_some_and(|count| count > 0),
                "no trace of {} at seed {} witnessed {name}: {count:?}",
                quint.main,
                quint.seed
            );
        }
        Ok(())
    }

    /// Writes the traces of the model and the violation of its probe again at the seed, and requires the committed states.
    fn traces(&self, model: &str, quint: &QuintModel) -> Result<()> {
        let generated = self.output.join("traces");
        fs::create_dir_all(&generated)?;
        let steps = quint.steps.to_string();
        let count = quint
            .traces
            .iter()
            .filter(|trace| of_model(trace, &quint.main))
            .count()
            .to_string();
        let pattern = format!("{OUTPUT}/traces/{}.{{seq}}.itf.json", quint.main);
        let probe = format!("{OUTPUT}/traces/{}.itf.json", quint.probe);
        let base = [
            "run",
            model,
            "--backend",
            "typescript",
            "--seed",
            &quint.seed,
        ];
        let mut traced = base.to_vec();
        traced.extend([
            "--main",
            &quint.main,
            "--max-samples",
            &count,
            "--n-traces",
            &count,
            "--max-steps",
            &steps,
            "--mbt",
            "--out-itf",
            &pattern,
            "--verbosity",
            "0",
        ]);
        self.passing(&format!("the traces of {}", quint.main), &traced)?;
        let mut violated = base.to_vec();
        violated.extend([
            "--main",
            &quint.probe,
            "--max-samples",
            PROBE_SAMPLES,
            "--max-steps",
            &steps,
            "--invariant",
            &quint.invariant,
            "--mbt",
            "--out-itf",
            &probe,
            "--verbosity",
            "0",
        ]);
        // The probe's run fails on the violation it records.
        self.quint(&violated)?;
        for trace in &quint.traces {
            let committed = self.root.join(trace);
            let name = committed.file_name().context("a trace path names a file")?;
            let fresh = fs::read_to_string(generated.join(name))
                .with_context(|| format!("Quint wrote no trace for {trace}"))?;
            ensure!(
                states(&fs::read_to_string(&committed).context(trace.clone())?)? == states(&fresh)?,
                "{trace} differs from the trace Quint writes at seed {}; regenerate it and review the change",
                quint.seed
            );
        }
        Ok(())
    }
}

/// Discharges every Quint obligation of the manifest in the pinned image, offline.
///
/// # Errors
///
/// Returns an error when Docker can't build or run the image, or any check of an obligation fails.
pub fn verify(root: &Path, manifest: &obligations::Manifest) -> Result<()> {
    let root = crate::canonical(root)?;
    let output = tempfile::tempdir()?;
    let image = image(&root)?;
    build(&root, &image)?;
    fs::write(output.path().join("tlc.json"), TLC_CONFIG)?;
    let runner = Runner {
        root: &root,
        output: output.path(),
        image,
    };
    let mut verified = 0;
    for obligation in &manifest.obligations {
        if let Obligation::Quint(quint) = obligation {
            let model = format!("{SOURCE}/{}", quint.spec);
            runner.check(&model, quint)?;
            runner.witness(&model, quint)?;
            runner.traces(&model, quint)?;
            verified += 1;
        }
    }
    let graphs = crate::state_graph::check(&root, output.path(), &runner.image, manifest)?;
    println!(
        "Verified all {verified} Quint models offline: each typechecks, passes its tests, holds its invariant under TLC with its recorded state count, witnesses every outcome, and reproduces its traces, each probe breaks the invariant, and TLC dumps the committed state graph of {graphs}"
    );
    Ok(())
}
