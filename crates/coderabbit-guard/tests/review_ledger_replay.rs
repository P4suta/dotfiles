//! Replays the traces of `specs/review_ledger.qnt` through the step function of `review_rules` and compares the full state after every step.

use anyhow::{Context, Result, bail, ensure};
use coderabbit_guard::review_rules::{
    Event, GUARDS, Guard, HORIZON, Ledger, Stage, ledger, ledger_safe, record,
};
use serde_json::Value;
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

fn boolean(value: &Value) -> Result<bool> {
    value.as_bool().context("an ITF Boolean")
}

fn stage(value: &Value) -> Result<Stage> {
    Ok(match value["tag"].as_str().context("an ITF variant")? {
        "Idle" => Stage::Idle,
        "Locked" => Stage::Locked,
        "Read" => Stage::Read,
        "Reserved" => Stage::Reserved,
        "Done" => Stage::Done,
        other => bail!("unknown phase {other}"),
    })
}

/// The values of an Informal Trace Format (ITF) map over `0..length`, in key order.
fn map<T>(value: &Value, length: usize, decode: impl Fn(&Value) -> Result<T>) -> Result<Vec<T>> {
    let pairs = value["#map"].as_array().context("an ITF map")?;
    ensure!(pairs.len() == length, "a map over every key");
    let mut found = Vec::new();
    for (index, pair) in pairs.iter().enumerate() {
        ensure!(integer(&pair[0])? == i64::try_from(index)?, "keys in order");
        found.push(decode(&pair[1])?);
    }
    Ok(found)
}

/// The model state that one state of a trace records, field for field.
fn decode(value: &Value) -> Result<Ledger> {
    let count = |value: &Value| -> Result<u8> { Ok(u8::try_from(integer(value)?)?) };
    let holder = integer(variable(value, "holder")?)?;
    Ok(Ledger {
        cap: count(variable(value, "cap")?)?,
        now: count(variable(value, "now")?)?,
        paused: boolean(variable(value, "paused")?)?,
        stage: map(variable(value, "phase")?, GUARDS, stage)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("a phase per guard"))?,
        saw: map(variable(value, "saw")?, GUARDS, boolean)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("a read per guard"))?,
        paused_since_lock: map(variable(value, "pausedSinceLock")?, GUARDS, boolean)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("a lock pause per guard"))?,
        holder: (holder >= 0).then(|| usize::try_from(holder)).transpose()?,
        records: map(variable(value, "ledger")?, usize::from(HORIZON) + 1, count)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("a record count per tick"))?,
        ignored_pause: boolean(variable(value, "ignoredPause")?)?,
    })
}

fn pick<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    let picked = &value["mbt::nondetPicks"][name];
    ensure!(picked["tag"] == "Some", "the action records its {name}");
    Ok(&picked["value"])
}

fn guard(value: &Value) -> Result<usize> {
    Ok(usize::try_from(integer(pick(value, "guard")?)?)?)
}

/// The action that produced a state of a trace, with the choices the model made.
fn event(value: &Value) -> Result<Option<Event>> {
    let taken = value["mbt::actionTaken"]
        .as_str()
        .context("the trace records each action")?;
    Ok(Some(match taken {
        "init" => return Ok(None),
        "tick" => Event::Tick,
        "pause" => Event::Pause,
        "resume" => Event::Resume,
        "readEarly" => Event::ReadEarly(guard(value)?),
        "lock" => Event::Lock(guard(value)?),
        "read" => Event::Read(guard(value)?),
        "reserve" => Event::Reserve(guard(value)?, u8::try_from(integer(pick(value, "skew")?)?)?),
        "launch" => Event::Launch(guard(value)?),
        "crash" => Event::Crash(guard(value)?),
        other => bail!("unknown action {other}"),
    }))
}

/// Replays one trace and returns its last state and the actions it took.
fn replay(model: Guard, text: &str) -> Result<(Ledger, BTreeSet<String>)> {
    let document: Value = serde_json::from_str(text)?;
    let states = document["states"]
        .as_array()
        .context("a trace lists states")?;
    let (first, rest) = states.split_first().context("a trace starts somewhere")?;
    let mut state = decode(first)?;
    ensure!(
        state == ledger(state.cap),
        "the trace starts elsewhere than the model"
    );
    let mut taken = BTreeSet::new();
    for (index, recorded) in rest.iter().enumerate() {
        let event = event(recorded)?.context("only the first state is initial")?;
        taken.insert(
            format!("{event:?}")
                .split(['(', ' '])
                .next()
                .unwrap_or_default()
                .to_owned(),
        );
        state = record(model, state, event)
            .with_context(|| format!("step {index}: {event:?} can't happen in {state:?}"))?;
        ensure!(
            state == decode(recorded)?,
            "step {index}: {event:?} left {state:?}, and the model recorded {recorded}"
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
        if name.starts_with("review_ledger") {
            found.push((name, std::fs::read_to_string(&path)?));
        }
    }
    found.sort();
    Ok(found)
}

#[test]
fn every_trace_of_the_fixed_guards_replays_state_for_state_and_covers_every_action() -> Result<()> {
    let mut covered = BTreeSet::new();
    let mut replayed = 0;
    for (name, text) in traces()? {
        if name.starts_with("review_ledger.") {
            let (last, taken) = replay(Guard::Fixed, &text).context(name)?;
            assert!(ledger_safe(&last));
            covered.extend(taken);
            replayed += 1;
        }
    }
    assert_eq!(replayed, FIXED_TRACES);
    // The fixed guards never read the pause before the lock, so the model disables that action.
    let every: BTreeSet<String> = [
        "Tick", "Pause", "Resume", "Lock", "Read", "Reserve", "Launch", "Crash",
    ]
    .map(str::to_owned)
    .into();
    assert_eq!(covered, every);
    Ok(())
}

#[test]
fn the_early_guard_replays_into_a_reservation_under_the_pause() -> Result<()> {
    let (_, text) = traces()?
        .into_iter()
        .find(|(name, _)| name == "review_ledger_early.itf.json")
        .context("the regression trace")?;
    let (last, taken) = replay(Guard::Early, &text)?;
    assert!(!ledger_safe(&last));
    assert!(taken.contains("ReadEarly") && taken.contains("Reserve"));
    Ok(())
}
