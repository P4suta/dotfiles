// How three reaper runs take the single-instance lock and kill a stopped orphan while the process table changes under them.
// `crates/rules/src/reaper_rules.rs` implements the same step function, and `crates/dotguard/tests/reaper_lock_replay.rs` runs the traces of this model through it.
module reaper_lock_model {
  // Whether the reapers run as they did before the fix.
  // The old lock is a directory that a run steals once it looks stale, and that a holder removes on exit even when another run took it since.
  // The old kill reads the identity of the process again, but not whether it is still stopped and orphaned.
  const OLD: bool

  // The runs that start, one after another or at once.
  pure val RUNS = 0.to(2)

  // How often the operating system reuses the process ID for another process.
  pure val REUSES = 1

  // Where one run stands: before the lock, about to remove a lock it judged stale, about to create the lock again, holding it, or finished.
  type Phase = Idle | Stealing | Creating | Holding | Done

  var phase: int -> Phase
  // The identity each run read for the target while it held the lock, or -1.
  var observed: int -> int
  // Whether the lock exists: the old directory, or the advisory lock the operating system holds for an open file.
  var locked: bool
  // The process behind the target ID: its identity, and whether it is stopped and orphaned.
  var target: { identity: int, stopped: bool, orphan: bool }
  // What the last kill hit, with identity -1 before any kill.
  var kill: { identity: int, observed: int, stopped: bool, orphan: bool }
  // Whether a run refused a kill because the process changed after its read.
  var refused: bool

  pure val NO_KILL = { identity: -1, observed: -1, stopped: false, orphan: false }

  action init = all {
    phase' = RUNS.mapBy(_ => Idle),
    observed' = RUNS.mapBy(_ => -1),
    locked' = false,
    target' = { identity: 0, stopped: true, orphan: true },
    kill' = NO_KILL,
    refused' = false,
  }

  action keepWorld = all {
    target' = target,
    kill' = kill,
    refused' = refused,
  }

  // A run takes a free lock.
  action take = {
    nondet who = RUNS.filter(r => phase.get(r) == Idle and not(locked)).oneOf()
    all {
      phase' = phase.set(who, Holding),
      locked' = true,
      observed' = observed,
      keepWorld,
    }
  }

  // A run finds the lock held and leaves the work to its holder.
  action skip = {
    nondet who = RUNS.filter(r => phase.get(r) == Idle and locked).oneOf()
    all {
      phase' = phase.set(who, Done),
      locked' = locked,
      observed' = observed,
      keepWorld,
    }
  }

  // The old run judges a held lock stale, which an hour of age or a crashed holder makes it look.
  action steal = {
    nondet who = RUNS.filter(r => OLD and phase.get(r) == Idle and locked).oneOf()
    all {
      phase' = phase.set(who, Stealing),
      locked' = locked,
      observed' = observed,
      keepWorld,
    }
  }

  // The stealing run removes the lock directory, whoever holds it now.
  action remove = {
    nondet who = RUNS.filter(r => phase.get(r) == Stealing).oneOf()
    all {
      phase' = phase.set(who, Creating),
      locked' = false,
      observed' = observed,
      keepWorld,
    }
  }

  // The stealing run creates the lock again, or gives up when another run created it first.
  action create = {
    nondet who = RUNS.filter(r => phase.get(r) == Creating).oneOf()
    all {
      phase' = phase.set(who, if (locked) Done else Holding),
      locked' = true,
      observed' = observed,
      keepWorld,
    }
  }

  // A holder lists the processes and reads the identity of a stopped orphan.
  action observe = {
    nondet who = RUNS.filter(r => phase.get(r) == Holding and observed.get(r) == -1 and target.stopped and target.orphan).oneOf()
    all {
      observed' = observed.set(who, target.identity),
      phase' = phase,
      locked' = locked,
      keepWorld,
    }
  }

  // Whether the process the holder reads again right before the kill is the one it observed, still stopped and orphaned.
  pure def confirmed(old: bool, matches: bool, stopped: bool, orphan: bool): bool =
    if (old) matches else matches and stopped and orphan

  // A holder kills the process it observed when it reads it again unchanged, and refuses otherwise.
  action killTarget = {
    nondet who = RUNS.filter(r => phase.get(r) == Holding and observed.get(r) != -1).oneOf()
    val seen = observed.get(who)
    all {
      if (confirmed(OLD, target.identity == seen, target.stopped, target.orphan)) all {
        kill' = { identity: target.identity, observed: seen, stopped: target.stopped, orphan: target.orphan },
        refused' = refused,
      } else all {
        kill' = kill,
        refused' = true,
      },
      observed' = observed.set(who, -1),
      phase' = phase,
      locked' = locked,
      target' = target,
    }
  }

  // A holder finishes and releases the lock, and the old one removes the directory whoever holds it now.
  action release = {
    nondet who = RUNS.filter(r => phase.get(r) == Holding).oneOf()
    all {
      phase' = phase.set(who, Done),
      locked' = false,
      observed' = observed.set(who, -1),
      keepWorld,
    }
  }

  // A holder dies: the operating system releases the advisory lock, and the old directory stays behind.
  action crash = {
    nondet who = RUNS.filter(r => phase.get(r) == Holding).oneOf()
    all {
      phase' = phase.set(who, Done),
      locked' = (OLD and locked),
      observed' = observed.set(who, -1),
      keepWorld,
    }
  }

  action keepRuns = all {
    phase' = phase,
    observed' = observed,
    locked' = locked,
  }

  // The shell that stopped the target resumes it.
  action resume = all {
    target.stopped,
    target' = { ...target, stopped: false },
    kill' = kill,
    refused' = refused,
    keepRuns,
  }

  // The target stops again.
  action halt = all {
    not(target.stopped),
    target' = { ...target, stopped: true },
    kill' = kill,
    refused' = refused,
    keepRuns,
  }

  // The target exits and another process takes its ID.
  action reuse = {
    nondet stopped = Bool.oneOf()
    nondet orphan = Bool.oneOf()
    all {
      target.identity < REUSES,
      target' = { identity: target.identity + 1, stopped: stopped, orphan: orphan },
      kill' = kill,
      refused' = refused,
      keepRuns,
    }
  }

  action step = any { take, skip, steal, remove, create, observe, killTarget, release, crash, resume, halt, reuse }

  // At most one run holds the lock.
  val exclusive = RUNS.filter(r => phase.get(r) == Holding).size() <= 1

  // Every kill hits the process the run observed, while it is still stopped and orphaned.
  val identityStable = kill.identity == -1 or (kill.identity == kill.observed and kill.stopped and kill.orphan)

  val safe = exclusive and identityStable

  // A run kills the stopped orphan.
  val killed = kill.identity != -1
  // A run refuses a kill because the process changed after its read.
  val spared = refused
  // A run leaves the work to the holder of the lock.
  val skipped = RUNS.exists(r => phase.get(r) == Done) and RUNS.exists(r => phase.get(r) == Holding)
}

module reaper_lock {
  import reaper_lock_model(OLD = false).*

  // A run that finds the lock held skips, and a crashed holder leaves the lock free.
  run crashFreesTheLockTest =
    init.then(take).then(skip).then(crash).then(take)
      .expect(exclusive and RUNS.filter(r => phase.get(r) == Holding).size() == 1)

  // A resumed process survives the kill.
  run resumedTargetIsSparedTest =
    init.then(take).then(observe).then(resume).then(killTarget)
      .expect(refused and kill == NO_KILL and safe)

  run stoppedOrphanIsKilledTest =
    init.then(take).then(observe).then(killTarget)
      .expect(killed and safe)
}

module reaper_lock_old {
  import reaper_lock_model(OLD = true).*

  // The regression: two runs judge a crashed holder's lock stale, and the second removes the lock the first just created.
  run doubleStealTest =
    init.then(take).then(crash)
      .then(steal).then(steal)
      .then(remove).then(create)
      .then(remove).then(create)
      .expect(not(exclusive))
}
