//! Replays the traces of `specs/skill_gate.qnt` through the step function of `skill_gate_rules` and compares the full state after every step.

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use skill_ops::skill_gate_rules::{Answer, Gate, Move, PROCESSES, Store, gate, safe, step};
use std::collections::BTreeSet;
use std::path::Path;

/// The directory of the committed traces, which `just proofs` regenerates and compares.
const TRACES: &str = "../../specs/traces";

/// The traces the fixed model must replay, which the manifest also lists.
const FIXED_TRACES: usize = 100;

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

/// The model's report count, mapping negative one to `None`.
fn report(value: &Value) -> Result<Option<u8>> {
    let number = integer(value)?;
    Ok(if number < 0 {
        None
    } else {
        Some(u8::try_from(number)?)
    })
}

fn answer(value: &Value) -> Result<Answer> {
    Ok(match value["tag"].as_str().context("an ITF variant")? {
        "None" => Answer::None,
        "Allow" => Answer::Allow,
        "Request" => Answer::Request,
        "End" => Answer::End,
        other => bail!("unknown answer {other}"),
    })
}

/// The model state that one state of a trace records, field for field.
fn decode(value: &Value) -> Result<Gate> {
    let pairs = variable(value, "checked")?["#map"]
        .as_array()
        .context("the checked reports are a map")?;
    ensure!(pairs.len() == PROCESSES, "a checked report per process");
    let mut checked = [None; PROCESSES];
    for (index, pair) in pairs.iter().enumerate() {
        ensure!(
            integer(&pair[0])? == i64::try_from(index)?,
            "processes in order"
        );
        if let Some(slot) = checked.get_mut(index) {
            *slot = report(&pair[1])?;
        }
    }
    Ok(Gate {
        events: u8::try_from(integer(variable(value, "events")?)?)?,
        draft: report(variable(value, "draft")?)?,
        reviewed: report(variable(value, "reviewed")?)?,
        checked,
        continued: variable(value, "continued")?
            .as_bool()
            .context("continued is a Boolean")?,
        requests: u8::try_from(integer(variable(value, "requests")?)?)?,
        answer: answer(variable(value, "answer")?)?,
    })
}

fn process(value: &Value) -> Result<usize> {
    let picked = &value["mbt::nondetPicks"]["process"];
    ensure!(picked["tag"] == "Some", "the action records its process");
    Ok(usize::try_from(integer(&picked["value"])?)?)
}

/// The action that produced a state of a trace, with the process the model chose.
fn action(value: &Value) -> Result<Option<Move>> {
    let taken = value["mbt::actionTaken"]
        .as_str()
        .context("the trace records each action")?;
    Ok(Some(match taken {
        "init" => return Ok(None),
        "observe" => Move::Observe,
        "analyze" => Move::Analyze,
        "triage" => Move::Triage,
        "check" => Move::Check(process(value)?),
        "record" => Move::Record(process(value)?),
        "stop" => Move::Stop,
        other => bail!("unknown action {other}"),
    }))
}

/// Replays one trace and returns its last state and the actions it took.
fn replay(store: Store, text: &str) -> Result<(Gate, BTreeSet<String>)> {
    let document: Value = serde_json::from_str(text)?;
    let states = document["states"]
        .as_array()
        .context("a trace lists states")?;
    let (first, rest) = states.split_first().context("a trace starts somewhere")?;
    let mut state = decode(first)?;
    ensure!(state == gate(), "the trace starts elsewhere than the model");
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
        state = step(store, state, action)
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
        if name.starts_with("skill_gate") {
            found.push((name, std::fs::read_to_string(&path)?));
        }
    }
    found.sort();
    Ok(found)
}

#[test]
fn every_trace_of_the_fixed_store_replays_state_for_state_and_covers_every_action() -> Result<()> {
    let mut covered = BTreeSet::new();
    let mut replayed = 0;
    for (name, text) in traces()? {
        if name.starts_with("skill_gate.") {
            let (last, taken) = replay(Store::Fixed, &text).context(name)?;
            assert!(safe(&last));
            covered.extend(taken);
            replayed += 1;
        }
    }
    assert_eq!(replayed, FIXED_TRACES);
    // The fixed store triages in one step under its lock, so the model disables the old check and record.
    let every: BTreeSet<String> = ["Observe", "Analyze", "Triage", "Stop"]
        .map(str::to_owned)
        .into();
    assert_eq!(covered, every);
    Ok(())
}

#[test]
fn the_old_store_replays_a_triage_between_two_analyses_into_an_allowed_stop() -> Result<()> {
    let (_, text) = traces()?
        .into_iter()
        .find(|(name, _)| name == "skill_gate_old.itf.json")
        .context("the regression trace")?;
    let (last, taken) = replay(Store::Old, &text)?;
    assert!(!safe(&last));
    assert!(taken.contains("Check") && taken.contains("Record"));
    Ok(())
}
