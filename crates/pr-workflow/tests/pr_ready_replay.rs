//! Replays the traces of `specs/pr_ready.qnt` through the step function of `pr_ready_rules` and compares the full state after every step.

use anyhow::{Context, Result, bail, ensure};
use pr_workflow::pr_ready_rules::{Action, Phase, Report, State, Workflow, initial, safe, step};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

/// The directory of the committed traces, which `just proofs` regenerates and compares.
const TRACES: &str = "../../specs/traces";

/// The model's variables carry the module path as a prefix, which the replay drops.
fn variable<'a>(state: &'a Value, name: &str) -> Result<&'a Value> {
    state
        .as_object()
        .context("an ITF state is an object")?
        .iter()
        .find_map(|(key, value)| (key.rsplit("::").next() == Some(name)).then_some(value))
        .with_context(|| format!("the trace has no variable {name}"))
}

fn integer(value: &Value) -> Result<i64> {
    value["#bigint"]
        .as_str()
        .context("an ITF integer")?
        .parse()
        .context("an ITF integer")
}

fn head(value: &Value) -> Result<Option<u8>> {
    let number = integer(value)?;
    Ok(if number < 0 {
        None
    } else {
        Some(u8::try_from(number)?)
    })
}

fn tag(value: &Value) -> Result<&str> {
    value["tag"].as_str().context("an ITF variant")
}

fn report(value: &Value) -> Result<Report> {
    named_report(tag(value)?)
}

fn named_report(name: &str) -> Result<Report> {
    Ok(match name {
        "Pending" => Report::Pending,
        "Partial" => Report::Partial,
        "Complete" => Report::Complete,
        "Failed" => Report::Failed,
        other => bail!("unknown report {other}"),
    })
}

fn phase(value: &Value) -> Result<Phase> {
    named_phase(tag(value)?)
}

fn named_phase(name: &str) -> Result<Phase> {
    Ok(match name {
        "Idle" => Phase::Idle,
        "Viewed" => Phase::Viewed,
        "Marked" => Phase::Marked,
        "Refused" => Phase::Refused,
        "Returned" => Phase::Returned,
        other => bail!("unknown phase {other}"),
    })
}

/// The model state that one state of a trace records, field for field.
fn decode(value: &Value) -> Result<State> {
    let mut state = initial();
    state.tip = head(variable(value, "tip")?)?.context("the tip is a head")?;
    for pair in variable(value, "reports")?["#map"]
        .as_array()
        .context("the reports are a map")?
    {
        let index = usize::try_from(integer(&pair[0])?)?;
        *state
            .reports
            .get_mut(index)
            .context("a report for a head beyond the model")? = report(&pair[1])?;
    }
    state.ready = variable(value, "ready")?
        .as_bool()
        .context("ready is a Boolean")?;
    state.phase = phase(variable(value, "phase")?)?;
    state.viewed = head(variable(value, "viewed")?)?;
    state.checked = variable(value, "checked")?
        .as_bool()
        .context("checked is a Boolean")?;
    state.returned_tip = head(variable(value, "returned_tip")?)?;
    Ok(state)
}

/// The action that produced a state of a trace, with the report CI chose.
fn action(value: &Value) -> Result<Option<Action>> {
    let taken = value["mbt::actionTaken"]
        .as_str()
        .context("the trace records each action")?;
    Ok(Some(match taken {
        "init" => return Ok(None),
        "push" => Action::Push,
        "report" => {
            let picked = &value["mbt::nondetPicks"]["outcome"];
            ensure!(tag(picked)? == "Some", "a report records its outcome");
            Action::Report(report(&picked["value"])?)
        }
        "view" => Action::View,
        "mark" => Action::Mark,
        "settle" => Action::Settle,
        other => bail!("unknown action {other}"),
    }))
}

/// A step function under replay: the implementation's, or a mutant of it.
type Step = fn(Workflow, State, Action) -> Option<State>;

/// Replays one trace and returns its last state and the actions it took.
fn replay(workflow: Workflow, text: &str) -> Result<(State, BTreeSet<String>)> {
    replay_with(step, workflow, text)
}

fn replay_with(step: Step, workflow: Workflow, text: &str) -> Result<(State, BTreeSet<String>)> {
    let document: Value = serde_json::from_str(text)?;
    let states = document["states"]
        .as_array()
        .context("a trace lists states")?;
    let (first, rest) = states.split_first().context("a trace starts somewhere")?;
    let mut state = decode(first)?;
    ensure!(
        state == initial(),
        "the trace starts elsewhere than the model"
    );
    let mut taken = BTreeSet::new();
    for (index, recorded) in rest.iter().enumerate() {
        let action = action(recorded)?.context("only the first state is initial")?;
        taken.insert(
            format!("{action:?}")
                .split('(')
                .next()
                .unwrap_or_default()
                .to_owned(),
        );
        state = step(workflow, state, action)
            .with_context(|| format!("step {index}: {action:?} can't happen in {state:?}"))?;
        ensure!(
            state == decode(recorded)?,
            "step {index}: {action:?} left {state:?}, and the model recorded {recorded}"
        );
    }
    Ok((state, taken))
}

fn traces() -> Result<Vec<(String, String)>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(TRACES);
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&directory).context("specs/traces")? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("a trace name")?
            .to_owned();
        found.push((name, std::fs::read_to_string(&path)?));
    }
    found.sort();
    Ok(found)
}

#[test]
fn every_trace_of_the_fixed_model_replays_state_for_state_and_covers_every_action() -> Result<()> {
    let mut covered = BTreeSet::new();
    let mut replayed = 0;
    for (name, text) in traces()? {
        if name.starts_with("pr_ready.") {
            let (last, taken) = replay(Workflow::Fixed, &text).context(name)?;
            assert!(safe(&last));
            covered.extend(taken);
            replayed += 1;
        }
    }
    assert!(replayed > 0);
    let every: BTreeSet<String> = ["Push", "Report", "View", "Mark", "Settle"]
        .map(str::to_owned)
        .into();
    assert_eq!(covered, every);
    Ok(())
}

#[test]
fn the_old_command_replays_the_push_between_view_and_ready_into_an_unchecked_ready_head()
-> Result<()> {
    let (_, text) = traces()?
        .into_iter()
        .find(|(name, _)| name == "pr_ready_buggy.itf.json")
        .context("the regression trace")?;
    let (last, taken) = replay(Workflow::Old, &text)?;
    assert!(last.ready && !safe(&last));
    assert!(taken.contains("Push") && taken.contains("Mark"));
    Ok(())
}

/// The reachable state graph of the fixed model that `TLC` dumps, which `just graphs` writes and `just proofs` dumps again.
const GRAPH: &str = "../../specs/graphs/pr_ready.json";

/// The prefix that the `TLA+` compilation of the model gives every variable and action.
const PREFIX: &str = "pr_ready_pr_ready_model_";

/// Every action the step function takes, which the replay tries from each reachable state.
const ACTIONS: [Action; 8] = [
    Action::Push,
    Action::Report(Report::Pending),
    Action::Report(Report::Partial),
    Action::Report(Report::Complete),
    Action::Report(Report::Failed),
    Action::View,
    Action::Mark,
    Action::Settle,
];

/// A transition as the replay compares it, with the action by its name.
type Edge = (State, String, State);

/// A variable of a graph state, as `TLC` prints its value.
fn printed<'a>(state: &'a Value, name: &str) -> Result<&'a str> {
    state[format!("{PREFIX}{name}")]
        .as_str()
        .with_context(|| format!("the graph state has no variable {name}"))
}

fn printed_head(text: &str) -> Result<Option<u8>> {
    let number: i64 = text.parse()?;
    Ok(if number < 0 {
        None
    } else {
        Some(u8::try_from(number)?)
    })
}

fn printed_boolean(text: &str) -> Result<bool> {
    match text {
        "TRUE" => Ok(true),
        "FALSE" => Ok(false),
        other => bail!("{other} is no Boolean"),
    }
}

/// The name of a variant without a payload, which `TLC` prints as `[Name |-> [tag |-> "UNIT"]]`.
fn printed_tag(text: &str) -> Result<&str> {
    text.strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(r#" |-> [tag |-> "UNIT"]]"#))
        .with_context(|| format!("{text} is no variant"))
}

/// A graph state, field for field, from the values `TLC` prints.
fn printed_state(value: &Value) -> Result<State> {
    let mut state = initial();
    state.tip = printed_head(printed(value, "tip")?)?.context("the tip is a head")?;
    let reports = printed(value, "reports")?
        .strip_prefix("( ")
        .and_then(|rest| rest.strip_suffix(" )"))
        .context("the reports are a function")?;
    for pair in reports.split(" @@ ") {
        let (head, report) = pair.split_once(" :> ").context("a report of a head")?;
        *state
            .reports
            .get_mut(head.parse::<usize>()?)
            .context("a report for a head beyond the model")? = named_report(printed_tag(report)?)?;
    }
    state.ready = printed_boolean(printed(value, "ready")?)?;
    state.phase = named_phase(printed_tag(printed(value, "phase")?)?)?;
    state.viewed = printed_head(printed(value, "viewed")?)?;
    state.checked = printed_boolean(printed(value, "checked")?)?;
    state.returned_tip = printed_head(printed(value, "returned_tip")?)?;
    Ok(state)
}

/// The action an edge's label names, with the report CI chose.
fn printed_action(label: &str) -> Result<Action> {
    let name = label
        .strip_prefix(PREFIX)
        .with_context(|| format!("{label} is no action of the model"))?;
    Ok(match name {
        "push" => Action::Push,
        "view" => Action::View,
        "mark" => Action::Mark,
        "settle" => Action::Settle,
        other => {
            let outcome = other
                .strip_prefix("reportAs(")
                .and_then(|rest| rest.strip_suffix(')'))
                .with_context(|| format!("unknown action {other}"))?;
            Action::Report(named_report(printed_tag(outcome)?)?)
        }
    })
}

/// Replays every edge of the committed graph through `step` and compares the full next state.
/// It also requires an edge for every action `step` enables from a reachable state, so the implementation reaches no state the model lacks.
/// Returns the number of edges, which must equal the transitions `TLC` generated.
fn replay_graph(step: Step) -> Result<usize> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(GRAPH))
        .context("specs/graphs/pr_ready.json")?;
    let graph: Value = serde_json::from_str(&text)?;
    let states = graph["states"]
        .as_array()
        .context("the graph lists states")?
        .iter()
        .map(printed_state)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        states.contains(&initial()),
        "the graph lacks the initial state"
    );
    let mut edges: BTreeSet<Edge> = BTreeSet::new();
    let listed = graph["edges"].as_array().context("the graph lists edges")?;
    for edge in listed {
        let at = |position: usize| -> Result<State> {
            let index = usize::try_from(edge[position].as_u64().context("a state index")?)?;
            states.get(index).copied().context("an edge names no state")
        };
        let (from, to) = (at(0)?, at(2)?);
        let action = printed_action(edge[1].as_str().context("an edge names its action")?)?;
        let next = step(Workflow::Fixed, from, action).with_context(|| {
            format!("{action:?} can't happen in {from:?}, and the model takes it")
        })?;
        ensure!(
            next == to,
            "{action:?} from {from:?} left {next:?}, and the model reaches {to:?}"
        );
        ensure!(
            edges.insert((from, format!("{action:?}"), to)),
            "the graph repeats an edge"
        );
    }
    for &state in &states {
        for action in ACTIONS {
            if let Some(next) = step(Workflow::Fixed, state, action) {
                ensure!(
                    edges.contains(&(state, format!("{action:?}"), next)),
                    "{action:?} from {state:?} reaches {next:?}, which the model has no edge for"
                );
            }
        }
    }
    let count = |name: &str| graph[name].as_u64().context("the graph records its counts");
    ensure!(
        u64::try_from(edges.len())? == count("generated")? - count("initial")?,
        "the graph holds {} edges and TLC generated a different number of transitions",
        edges.len()
    );
    Ok(edges.len())
}

/// The step function, except that viewing a head when every head failed reads its checks as passed.
fn diverging(workflow: Workflow, state: State, action: Action) -> Option<State> {
    let next = step(workflow, state, action)?;
    Some(
        if state.reports == [Report::Failed; 3] && action == Action::View {
            State {
                checked: true,
                ..next
            }
        } else {
            next
        },
    )
}

#[test]
fn every_reachable_edge_of_the_model_replays_state_for_state_and_the_step_enables_no_other()
-> Result<()> {
    assert_eq!(replay_graph(step)?, 1927);
    Ok(())
}

#[test]
fn a_step_that_diverges_on_an_untraced_state_passes_every_trace_and_fails_the_graph() -> Result<()>
{
    for (name, text) in traces()? {
        if name.starts_with("pr_ready.") {
            replay_with(diverging, Workflow::Fixed, &text).context(name.clone())?;
            let document: Value = serde_json::from_str(&text)?;
            for recorded in document["states"]
                .as_array()
                .context("a trace lists states")?
            {
                assert_ne!(decode(recorded)?.reports, [Report::Failed; 3], "{name}");
            }
        }
    }
    let refusal = replay_graph(diverging).unwrap_err().to_string();
    assert!(refusal.contains("View"), "{refusal}");
    Ok(())
}
