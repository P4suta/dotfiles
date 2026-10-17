//! Replays the traces of `specs/reaper_lock.qnt` through the step function of `reaper_rules` and compares the full state after every step.

use anyhow::{Context, Result, bail, ensure};
use dotguard::reaper_rules::{
    Action, Kill, Phase, RUNS, Reaper, State, Target, initial, safe, step,
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

/// A non-negative model integer, or `None` where the model marks absence.
fn optional(value: &Value) -> Result<Option<u8>> {
    let number = integer(value)?;
    Ok(if number < 0 {
        None
    } else {
        Some(u8::try_from(number)?)
    })
}

fn boolean(value: &Value, field: &str) -> Result<bool> {
    value[field]
        .as_bool()
        .with_context(|| format!("{field} is a Boolean"))
}

fn phase(value: &Value) -> Result<Phase> {
    Ok(match value["tag"].as_str().context("an ITF variant")? {
        "Idle" => Phase::Idle,
        "Stealing" => Phase::Stealing,
        "Creating" => Phase::Creating,
        "Holding" => Phase::Holding,
        "Done" => Phase::Done,
        other => bail!("unknown phase {other}"),
    })
}

/// The entries of an Incremental Trace Format map from run to value, each run once.
fn runs<T: Copy>(value: &Value, decode: impl Fn(&Value) -> Result<T>) -> Result<[T; RUNS]> {
    let pairs = value["#map"].as_array().context("a map over the runs")?;
    ensure!(pairs.len() == RUNS, "a map over every run");
    let mut found = Vec::new();
    for (index, pair) in pairs.iter().enumerate() {
        ensure!(integer(&pair[0])? == i64::try_from(index)?, "runs in order");
        found.push(decode(&pair[1])?);
    }
    found
        .try_into()
        .map_err(|_| anyhow::anyhow!("a map over every run"))
}

/// The model state that one state of a trace records, field for field.
fn decode(value: &Value) -> Result<State> {
    let target = variable(value, "target")?;
    let kill = variable(value, "kill")?;
    let killed = optional(&kill["identity"])?
        .map(|identity| -> Result<Kill> {
            Ok(Kill {
                identity,
                observed: optional(&kill["observed"])?.context("a kill names what it observed")?,
                stopped: boolean(kill, "stopped")?,
                orphan: boolean(kill, "orphan")?,
            })
        })
        .transpose()?;
    if killed.is_none() {
        ensure!(
            optional(&kill["observed"])?.is_none()
                && !boolean(kill, "stopped")?
                && !boolean(kill, "orphan")?,
            "no kill leaves every field at its default"
        );
    }
    Ok(State {
        phase: runs(variable(value, "phase")?, phase)?,
        observed: runs(variable(value, "observed")?, optional)?,
        locked: variable(value, "locked")?
            .as_bool()
            .context("locked is a Boolean")?,
        target: Target {
            identity: optional(&target["identity"])?.context("the target has an identity")?,
            stopped: boolean(target, "stopped")?,
            orphan: boolean(target, "orphan")?,
        },
        kill: killed,
        refused: variable(value, "refused")?
            .as_bool()
            .context("refused is a Boolean")?,
    })
}

fn pick<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    let picked = &value["mbt::nondetPicks"][name];
    ensure!(picked["tag"] == "Some", "the action records its {name}");
    Ok(&picked["value"])
}

fn run(value: &Value) -> Result<usize> {
    Ok(usize::try_from(integer(pick(value, "who")?)?)?)
}

/// The action that produced a state of a trace, with the choices the model made.
fn action(value: &Value) -> Result<Option<Action>> {
    let taken = value["mbt::actionTaken"]
        .as_str()
        .context("the trace records each action")?;
    Ok(Some(match taken {
        "init" => return Ok(None),
        "take" => Action::Take(run(value)?),
        "skip" => Action::Skip(run(value)?),
        "steal" => Action::Steal(run(value)?),
        "remove" => Action::Remove(run(value)?),
        "create" => Action::Create(run(value)?),
        "observe" => Action::Observe(run(value)?),
        "killTarget" => Action::Kill(run(value)?),
        "release" => Action::Release(run(value)?),
        "crash" => Action::Crash(run(value)?),
        "resume" => Action::Resume,
        "halt" => Action::Halt,
        "reuse" => Action::Reuse {
            stopped: pick(value, "stopped")?
                .as_bool()
                .context("a Boolean pick")?,
            orphan: pick(value, "orphan")?.as_bool().context("a Boolean pick")?,
        },
        other => bail!("unknown action {other}"),
    }))
}

/// Replays one trace and returns its last state and the actions it took.
fn replay(reaper: Reaper, text: &str) -> Result<(State, BTreeSet<String>)> {
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
                .split(['(', ' '])
                .next()
                .unwrap_or_default()
                .to_owned(),
        );
        state = step(reaper, state, action)
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
        if name.starts_with("reaper_lock") {
            found.push((name, std::fs::read_to_string(&path)?));
        }
    }
    found.sort();
    Ok(found)
}

#[test]
fn every_trace_of_the_fixed_model_replays_state_for_state_and_covers_every_action() -> Result<()> {
    let mut covered = BTreeSet::new();
    let mut replayed = 0;
    for (name, text) in traces()? {
        if name.starts_with("reaper_lock.") {
            let (last, taken) = replay(Reaper::Fixed, &text).context(name)?;
            assert!(safe(&last));
            covered.extend(taken);
            replayed += 1;
        }
    }
    assert_eq!(replayed, FIXED_TRACES);
    // The fixed reaper never steals, so the model disables the three steps of a steal.
    let every: BTreeSet<String> = [
        "Take", "Skip", "Observe", "Kill", "Release", "Crash", "Resume", "Halt", "Reuse",
    ]
    .map(str::to_owned)
    .into();
    assert_eq!(covered, every);
    Ok(())
}

#[test]
fn the_old_reaper_replays_into_two_runs_holding_one_lock() -> Result<()> {
    let (_, text) = traces()?
        .into_iter()
        .find(|(name, _)| name == "reaper_lock_old.itf.json")
        .context("the regression trace")?;
    let (last, taken) = replay(Reaper::Old, &text)?;
    assert!(!safe(&last));
    assert!(
        ["Steal", "Remove", "Create"]
            .iter()
            .all(|action| taken.contains(*action))
    );
    Ok(())
}
