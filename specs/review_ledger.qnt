// How two CodeRabbit guards share the review ledger while the clock skews, a guard crashes, and the owner pauses.
// `crates/rules/src/review_rules.rs` implements the same step function, and `crates/coderabbit-guard/tests/review_ledger_replay.rs` runs the traces of this model through it.
// `check_budget` decides one reservation, and this model owns the rolling limits over every sequence of them.
module review_ledger_model {
  // Whether the guards run as they did before the fix, reading the pause before they take the ledger lock.
  const EARLY_PAUSE: bool
  const N: int

  // Apalache initializes the symbolic hourly capacity once for the entire behavior.
  pure val ConstInit = N.in(Int) and N > 0

  pure val GUARDS = 0.to(1)

  // The ticks of a rolling hour and of a rolling day, and the last tick the model reaches.
  pure val HOUR = 1
  pure val DAY = 2
  pure val HORIZON = 3

  // The largest hourly limit the model chooses; the daily limit holds as many hours as a day holds.
  pure val MAX_CAP = N

  // Where one guard stands: before the lock, holding it, after reading the pause, with a reservation, or finished.
  type Phase = Idle | Locked | Read | Reserved | Done

  // The hourly limit, chosen once and never changed, so every check holds for each limit up to MAX_CAP.
  var cap: int
  var now: int
  var paused: bool
  var phase: int -> Phase
  // The pause each guard read, or false before it reads.
  var saw: int -> bool
  // Whether the pause stayed on from the moment the guard took the lock.
  var pausedSinceLock: int -> bool
  // The guard holding the ledger lock, or -1.
  var holder: int
  // The reservations recorded at each tick.
  var ledger: int -> int
  // Whether a guard reserved while the pause it should have seen stayed on.
  var ignoredPause: bool

  pure val TICKS = 0.to(HORIZON)

  // The records of `ledger` from `start` up to and excluding `end`.
  pure def window(records: int -> int, start: int, end: int): int =
    TICKS.filter(t => start <= t and t < end).fold(0, (sum, t) => sum + records.get(t))

  action init = {
    nondet chosen = 1.to(MAX_CAP).oneOf()
    all {
      ConstInit,
      cap' = chosen,
      now' = 0,
      paused' = false,
      phase' = GUARDS.mapBy(_ => Idle),
      saw' = GUARDS.mapBy(_ => false),
      pausedSinceLock' = GUARDS.mapBy(_ => false),
      holder' = -1,
      ledger' = TICKS.mapBy(_ => 0),
      ignoredPause' = false,
    }
  }

  action keepGuards = all {
    phase' = phase,
    saw' = saw,
    holder' = holder,
    ledger' = ledger,
    ignoredPause' = ignoredPause,
  }

  // Time passes.
  action tick = all {
    now < HORIZON,
    now' = now + 1,
    cap' = cap,
    paused' = paused,
    pausedSinceLock' = pausedSinceLock,
    keepGuards,
  }

  // The owner pauses reviews.
  action pause = all {
    not(paused),
    paused' = true,
    now' = now,
    cap' = cap,
    pausedSinceLock' = pausedSinceLock,
    keepGuards,
  }

  // The owner resumes reviews, which ends every pause a guard saw at its lock.
  action resume = all {
    paused,
    paused' = false,
    pausedSinceLock' = GUARDS.mapBy(_ => false),
    now' = now,
    cap' = cap,
    keepGuards,
  }

  // The old guard reads the pause before it takes the lock.
  action readEarly = {
    nondet guard = GUARDS.filter(g => EARLY_PAUSE and phase.get(g) == Idle).oneOf()
    all {
      saw' = saw.set(guard, paused),
      phase' = phase,
      holder' = holder,
      ledger' = ledger,
      ignoredPause' = ignoredPause,
      now' = now,
      cap' = cap,
      paused' = paused,
      pausedSinceLock' = pausedSinceLock,
    }
  }

  // A guard takes the free ledger lock.
  action lock = {
    nondet guard = GUARDS.filter(g => phase.get(g) == Idle and holder == -1).oneOf()
    all {
      phase' = phase.set(guard, Locked),
      holder' = guard,
      pausedSinceLock' = pausedSinceLock.set(guard, paused),
      saw' = saw,
      ledger' = ledger,
      ignoredPause' = ignoredPause,
      now' = now,
      cap' = cap,
      paused' = paused,
    }
  }

  // The fixed guard reads the pause while it holds the lock, and the old one goes on with the pause it read before.
  action read = {
    nondet guard = GUARDS.filter(g => phase.get(g) == Locked).oneOf()
    all {
      saw' = if (EARLY_PAUSE) saw else saw.set(guard, paused),
      phase' = phase.set(guard, Read),
      holder' = holder,
      ledger' = ledger,
      ignoredPause' = ignoredPause,
      now' = now,
      cap' = cap,
      paused' = paused,
      pausedSinceLock' = pausedSinceLock,
    }
  }

  // A guard checks the budget at its clock, which may lag by a tick, and records a reservation, or refuses and releases the lock.
  action reserve = {
    nondet guard = GUARDS.filter(g => phase.get(g) == Read).oneOf()
    nondet skew = 0.to(1).oneOf()
    val clock = if (now >= skew) now - skew else 0
    val admitted = not(saw.get(guard))
      and TICKS.forall(t => t > clock implies ledger.get(t) == 0)
      and window(ledger, clock - HOUR + 1, clock + 1) < cap
      and window(ledger, clock - DAY + 1, clock + 1) < cap * (DAY / HOUR)
    all {
      if (admitted) all {
        phase' = phase.set(guard, Reserved),
        holder' = holder,
        ledger' = ledger.set(clock, ledger.get(clock) + 1),
        ignoredPause' = (ignoredPause or pausedSinceLock.get(guard)),
      } else all {
        phase' = phase.set(guard, Done),
        holder' = -1,
        ledger' = ledger,
        ignoredPause' = ignoredPause,
      },
      saw' = saw,
      now' = now,
      cap' = cap,
      paused' = paused,
      pausedSinceLock' = pausedSinceLock,
    }
  }

  // A guard launches its reserved review and releases the lock.
  action launch = {
    nondet guard = GUARDS.filter(g => phase.get(g) == Reserved).oneOf()
    all {
      phase' = phase.set(guard, Done),
      holder' = -1,
      saw' = saw,
      ledger' = ledger,
      ignoredPause' = ignoredPause,
      now' = now,
      cap' = cap,
      paused' = paused,
      pausedSinceLock' = pausedSinceLock,
    }
  }

  // A guard dies wherever it stands, the system releases its lock, and its recorded reservation stays.
  action crash = {
    nondet guard = GUARDS.filter(g => phase.get(g) != Idle and phase.get(g) != Done).oneOf()
    all {
      phase' = phase.set(guard, Done),
      holder' = if (holder == guard) -1 else holder,
      saw' = saw,
      ledger' = ledger,
      ignoredPause' = ignoredPause,
      now' = now,
      cap' = cap,
      paused' = paused,
      pausedSinceLock' = pausedSinceLock,
    }
  }

  action step = any { tick, pause, resume, readEarly, lock, read, reserve, launch, crash }

  // No rolling hour holds more than `cap` reservations and no rolling day more than a day's worth.
  val withinBudget = TICKS.forall(t => window(ledger, t, t + HOUR) <= cap and window(ledger, t, t + DAY) <= cap * (DAY / HOUR))

  // No guard reserves while the pause it took the lock under stays on.
  val heedsPause = not(ignoredPause)

  val safe = withinBudget and heedsPause

  // The facts every reachable state of the fixed guards keeps, which imply `safe` and survive every step.
  // Apalache reads the leading memberships as the domain of each variable.
  val inductive = all {
    ConstInit,
    cap.in(Int),
    cap > 0 and cap <= N,
    now.in(0.to(HORIZON)),
    paused.in(Bool),
    phase.in(GUARDS.setOfMaps(Set(Idle, Locked, Read, Reserved, Done))),
    saw.in(GUARDS.setOfMaps(Bool)),
    pausedSinceLock.in(GUARDS.setOfMaps(Bool)),
    holder.in((-1).to(1)),
    ledger.in(TICKS.setOfMaps(Int)),
    TICKS.forall(t => ledger.get(t) >= 0 and ledger.get(t) <= N),
    ignoredPause.in(Bool),
    TICKS.forall(t => t > now implies ledger.get(t) == 0),
    GUARDS.forall(g => (holder == g) iff (phase.get(g) == Locked or phase.get(g) == Read or phase.get(g) == Reserved)),
    GUARDS.forall(g => (phase.get(g) == Read and pausedSinceLock.get(g)) implies saw.get(g)),
    GUARDS.forall(g => pausedSinceLock.get(g) implies paused),
    safe,
  }

  // A guard launches a review.
  val launched = GUARDS.exists(g => phase.get(g) == Done) and TICKS.exists(t => ledger.get(t) > 0)
  // The budget refuses a reservation at the limit.
  val full = TICKS.exists(t => window(ledger, t, t + HOUR) == cap)
  // A guard reads the pause and stops.
  val stopped = GUARDS.exists(g => phase.get(g) == Done and saw.get(g))
}

module review_ledger {
  import review_ledger_model(EARLY_PAUSE = false, N = 2).*

  run pauseUnderLockStopsTheGuardTest =
    init.then(lock).then(pause).then(read).then(reserve)
      .expect(heedsPause and ledger.get(0) == 0)

  run aReservationLaunchesTest =
    init.then(lock).then(read).then(reserve).then(launch)
      .expect(safe and ledger.get(0) == 1)
}

module review_ledger_early {
  import review_ledger_model(EARLY_PAUSE = true, N = 2).*

  // The regression: the guard reads the pause, the owner pauses, and the guard reserves under the pause anyway.
  run earlyReadIgnoresThePauseTest =
    init.then(readEarly).then(pause).then(lock).then(read).then(reserve)
      .expect(not(heedsPause))
}

module review_ledger_symbolic {
  const CAPACITY: int
  import review_ledger_model(EARLY_PAUSE = false, N = CAPACITY).*
}
