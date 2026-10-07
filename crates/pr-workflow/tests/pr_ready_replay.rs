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
    Ok(match tag(value)? {
        "Pending" => Report::Pending,
        "Partial" => Report::Partial,
        "Complete" => Report::Complete,
        "Failed" => Report::Failed,
        other => bail!("unknown report {other}"),
    })
}

fn phase(value: &Value) -> Result<Phase> {
    Ok(match tag(value)? {
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

/// Replays one trace and returns its last state and the actions it took.
fn replay(workflow: Workflow, text: &str) -> Result<(State, BTreeSet<String>)> {
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
