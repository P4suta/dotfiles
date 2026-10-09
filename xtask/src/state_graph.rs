//! The reachable state graph of a finite Quint model, which `TLC` dumps with every transition labelled by its action.
//! A model's committed graph lets a test replay every reachable edge through the implementation's step function, where a trace replay visits only the sampled ones.

use crate::obligations::{Manifest, Obligation, QuintModel};
use crate::quint;
use crate::tool::Tool;
use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// One state as ``TLC`` prints it: each variable's name and its value with the line breaks of its layout folded to single spaces.
pub type State = BTreeMap<String, String>;

/// The states in sorted order and every transition between them as `(from, action, to)`, with the counts `TLC` reported.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Graph {
    /// The states `TLC` generated, the initial ones included, which counts every transition once.
    pub generated: u64,
    pub initial: u64,
    pub states: Vec<State>,
    pub edges: Vec<(usize, String, usize)>,
}

/// The `TLC` configuration of a compiled model, whose initial and step actions Quint names.
const CONFIGURATION: &str = "INIT q_init\nNEXT q_step\n";

/// The thread stack `TLC` needs for the deep expressions Quint compiles.
const STACK: &str = "-Xss64m";

/// Where `TLC` keeps its fingerprints, inside the container.
const STATES: &str = "/tmp/states";

/// The transitions and initial states that the ``TLC`` report counts.
#[must_use]
pub fn transitions(report: &str) -> Option<(u64, u64)> {
    let count = |prefix: &str, suffix: &str| {
        report.lines().find_map(|line| {
            let (number, _) = line.strip_prefix(prefix)?.split_once(suffix)?;
            number.trim().parse::<u64>().ok()
        })
    };
    let generated = count("", " states generated, ")?;
    let initial = count("Finished computing initial states: ", " distinct state")?;
    Some((generated, initial))
}

/// The text of a `DOT` string whose opening quote precedes `text`, and what follows its closing quote.
fn quoted(text: &str) -> Option<(String, &str)> {
    let mut value = String::new();
    let mut characters = text.char_indices();
    while let Some((index, character)) = characters.next() {
        match character {
            '"' => return Some((value, &text[index + 1..])),
            '\\' => match characters.next()?.1 {
                'n' => value.push('\n'),
                other => value.push(other),
            },
            other => value.push(other),
        }
    }
    None
}

fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The variables of a state label, which `TLC` writes as a conjunction with one `/\ name = value` per variable.
fn variables(label: &str) -> Result<State> {
    let body = label
        .strip_prefix("/\\ ")
        .with_context(|| format!("a state label is a conjunction: {label}"))?;
    body.split("\n/\\ ")
        .map(|conjunct| {
            let (name, value) = conjunct
                .split_once(" = ")
                .with_context(|| format!("a conjunct assigns a variable: {conjunct}"))?;
            Ok((name.to_owned(), folded(value)))
        })
        .collect()
}

/// The graph of a `TLC` dump in `dot,actionlabels` form, with the report of the same run.
/// The node names, fingerprints that differ between runs, drop out of the sorted states, which go by value and index.
///
/// # Errors
///
/// Returns an error when the report lacks its counts, a label holds no state, or an edge names an unknown node.
/// It also errors when the dump holds a different number of edges than the transitions `TLC` generated.
pub fn canonical(dot: &str, report: &str) -> Result<Graph> {
    let (generated, initial) =
        transitions(report).context("the TLC report counts no generated states")?;
    let mut nodes: BTreeMap<i64, State> = BTreeMap::new();
    let mut arrows: Vec<(i64, String, i64)> = Vec::new();
    for line in dot.lines() {
        let Some((head, rest)) = line.split_once(" [label=\"") else {
            continue;
        };
        let (label, _) = quoted(rest).with_context(|| format!("an unterminated label: {line}"))?;
        if let Some((from, to)) = head.split_once(" -> ") {
            arrows.push((from.parse()?, folded(&label), to.parse()?));
        } else {
            nodes.insert(head.parse()?, variables(&label)?);
        }
    }
    let sorted: BTreeSet<&State> = nodes.values().collect();
    let index: BTreeMap<&State, usize> = sorted
        .iter()
        .enumerate()
        .map(|(position, state)| (*state, position))
        .collect();
    let mut edges = arrows
        .into_iter()
        .map(|(from, action, to)| {
            let position = |node: i64| {
                nodes
                    .get(&node)
                    .and_then(|state| index.get(state).copied())
                    .with_context(|| format!("an edge names the unknown node {node}"))
            };
            Ok((position(from)?, action, position(to)?))
        })
        .collect::<Result<Vec<_>>>()?;
    edges.sort();
    let expected = generated.saturating_sub(initial);
    ensure!(
        u64::try_from(edges.len())? == expected,
        "the dump holds {} edges and TLC generated {expected} transitions, so the graph misses or repeats one",
        edges.len()
    );
    Ok(Graph {
        generated,
        initial,
        states: sorted.into_iter().cloned().collect(),
        edges,
    })
}

/// The committed form of a graph: JSON with one state or edge per line, so a model change shows as a reviewable diff.
#[must_use]
pub fn render(graph: &Graph) -> String {
    let line = |value: String, last: bool, closing: &str| {
        if last {
            format!("{value}{closing}\n")
        } else {
            format!("{value},\n")
        }
    };
    let mut text = format!(
        "{{\"generated\":{},\"initial\":{},\n\"states\":[\n",
        graph.generated, graph.initial
    );
    let count = graph.states.len();
    for (position, state) in graph.states.iter().enumerate() {
        text.push_str(&line(
            serde_json::to_string(state).unwrap_or_default(),
            position + 1 == count,
            "],",
        ));
    }
    text.push_str("\"edges\":[\n");
    let count = graph.edges.len();
    for (position, edge) in graph.edges.iter().enumerate() {
        text.push_str(&line(
            serde_json::to_string(edge).unwrap_or_default(),
            position + 1 == count,
            "]}",
        ));
    }
    text
}

/// Runs `command` in the pinned Quint image and requires success, returning its standard output.
fn run(root: &Path, output: &Path, image: &str, command: &[&str]) -> Result<String> {
    let result = Tool::Docker
        .command()
        .args(quint::container_arguments(root, output, image, command))
        .output()
        .context("start the Quint container")?;
    let stdout = String::from_utf8(result.stdout)?;
    ensure!(
        result.status.success(),
        "`{}` failed in the Quint image: {}\n{stdout}{}",
        command.join(" "),
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(stdout)
}

/// Compiles the model to `TLA+` and dumps its reachable graph with `TLC` on one worker, offline in the pinned image.
///
/// # Errors
///
/// Returns an error when the compilation or `TLC` fails, or the dump doesn't hold the graph `TLC` counted.
pub fn dump(root: &Path, output: &Path, image: &str, model: &QuintModel) -> Result<Graph> {
    let main = &model.main;
    let spec = format!("{}/{}", quint::SOURCE, model.spec);
    let compiled = run(
        root,
        output,
        image,
        &[
            "quint", "compile", "--target", "tlaplus", "--main", main, &spec,
        ],
    )?;
    fs::write(output.join(format!("{main}.tla")), compiled)?;
    fs::write(output.join(format!("{main}.cfg")), CONFIGURATION)?;
    let jar = format!(
        "/root/.quint/apalache-dist-{}/apalache/lib/apalache.jar",
        quint::APALACHE_VERSION
    );
    let base = format!("{}/{main}", quint::OUTPUT);
    let module = format!("{base}.tla");
    let report = run(
        root,
        output,
        image,
        &[
            "java",
            STACK,
            "-cp",
            &jar,
            "tlc2.TLC",
            "-deadlock",
            "-workers",
            "1",
            "-metadir",
            STATES,
            "-dump",
            "dot,actionlabels",
            &base,
            &module,
        ],
    )?;
    let dot = fs::read_to_string(output.join(format!("{main}.dot")))
        .with_context(|| format!("TLC wrote no graph of {main}"))?;
    canonical(&dot, &report)
}

fn graphs(manifest: &Manifest) -> impl Iterator<Item = (&QuintModel, &str)> {
    manifest
        .obligations
        .iter()
        .filter_map(|obligation| match obligation {
            Obligation::Quint(model) => model.graph.as_deref().map(|graph| (model.as_ref(), graph)),
            _ => None,
        })
}

/// Requires the committed graph of every Quint model that records one to match the graph `TLC` dumps now.
///
/// # Errors
///
/// Returns an error when a dump fails or differs from its committed graph.
pub fn check(root: &Path, output: &Path, image: &str, manifest: &Manifest) -> Result<usize> {
    let mut checked = 0;
    for (model, graph) in graphs(manifest) {
        let fresh = render(&dump(root, output, image, model)?);
        let committed = fs::read_to_string(root.join(graph)).context(graph.to_owned())?;
        if committed != fresh {
            bail!(
                "{graph} differs from the graph TLC dumps of {}; run `just graphs` and review the change",
                model.main
            );
        }
        checked += 1;
    }
    Ok(checked)
}

/// Writes the graph of every Quint model that records one.
///
/// # Errors
///
/// Returns an error when Docker can't build or run the image, or a dump fails.
pub fn write(root: &Path) -> Result<()> {
    let root = crate::canonical(root)?;
    let manifest = crate::obligations::manifest(&root)?;
    let output = tempfile::tempdir()?;
    let image = quint::image(&root)?;
    quint::build(&root, &image)?;
    for (model, graph) in graphs(&manifest) {
        let dumped = dump(&root, output.path(), &image, model)?;
        fs::write(root.join(graph), render(&dumped))?;
        println!(
            "Wrote {graph}: {} states and {} transitions of {}",
            dumped.states.len(),
            dumped.edges.len(),
            model.main
        );
    }
    Ok(())
}
