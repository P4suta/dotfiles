// How the skill maintenance gate binds a review to the latest analysis while observations arrive, two triages run, and the Stop hook asks for maintenance.
// `crates/rules/src/skill_gate_rules.rs` implements the same step function, and `crates/skill-ops/tests/skill_gate_replay.rs` runs the traces of this model through it.
module skill_gate_model {
  // Whether the store runs as it did before the fix: triage checks the analysis and records the review in two steps, and a recorded review stands in for any later draft.
  const OLD: bool

  pure val MAX_EVENTS = 3
  pure val THRESHOLD = 2
  pure val PROCESSES = 0.to(1)

  type Answer = None | Allow | Request | End

  // The observations recorded so far, which only grow.
  var events: int
  // The draft report and the reviewed report, each the observation count it analyzed, or -1.
  var draft: int
  var reviewed: int
  // The report each old triage checked, or -1.
  var checked: int -> int
  var continued: bool
  var requests: int
  var answer: Answer

  pure def since(events: int, report: int): bool = events - report >= THRESHOLD

  // The report of n observations lists n findings, and its review decides each, so a review covers a draft no later than its report.
  def applies: bool = reviewed != -1 and (OLD or reviewed >= draft)

  // As `skill-ops` decides it: a draft that needs nothing ends the question, a review that applies is judged by the observations since its report, and otherwise the draft or the observations decide.
  def pending: bool =
    val draftPending = draft > 0 or since(events, draft)
    if (draft != -1 and not(draftPending)) false
    else if (applies) since(events, reviewed)
    else if (draft != -1) draftPending
    else events >= THRESHOLD

  action init = all {
    events' = 0,
    draft' = -1,
    reviewed' = -1,
    checked' = PROCESSES.mapBy(_ => -1),
    continued' = false,
    requests' = 0,
    answer' = None,
  }

  action keepSession = all {
    continued' = continued,
    requests' = requests,
    answer' = None,
  }

  // A client records an observation.
  action observe = all {
    events < MAX_EVENTS,
    events' = events + 1,
    draft' = draft,
    reviewed' = reviewed,
    checked' = checked,
    keepSession,
  }

  // skill-ops analyze writes the draft report of the current observations.
  action analyze = all {
    draft' = events,
    events' = events,
    reviewed' = reviewed,
    checked' = checked,
    keepSession,
  }

  // The fixed triage checks that the draft is current and records its review under the store lock.
  action triage = all {
    not(OLD),
    draft == events,
    reviewed' = draft,
    events' = events,
    draft' = draft,
    checked' = checked,
    keepSession,
  }

  // The old triage checks that the draft is current.
  action check = {
    nondet process = PROCESSES.filter(_ => OLD and draft == events).oneOf()
    all {
      checked' = checked.set(process, draft),
      events' = events,
      draft' = draft,
      reviewed' = reviewed,
      keepSession,
    }
  }

  // The old triage records the review it checked, whatever came since.
  action record = {
    nondet process = PROCESSES.filter(p => checked.get(p) != -1).oneOf()
    all {
      reviewed' = checked.get(process),
      checked' = checked.set(process, -1),
      events' = events,
      draft' = draft,
      keepSession,
    }
  }

  // The Stop hook allows a stop when nothing is pending, asks for maintenance once per session, and ends the session after that.
  action stop = all {
    if (not(pending)) all {
      answer' = Allow,
      continued' = continued,
      requests' = requests,
    } else if (continued) all {
      answer' = End,
      continued' = continued,
      requests' = requests,
    } else all {
      answer' = Request,
      continued' = true,
      requests' = requests + 1,
    },
    events' = events,
    draft' = draft,
    reviewed' = reviewed,
    checked' = checked,
  }

  action step = any { observe, analyze, triage, check, record, stop }

  // The gate allows a stop only with every finding of the latest draft decided, and asks for maintenance once per session.
  val safe = (answer != Allow or draft <= 0 or reviewed >= draft) and requests <= 1

  // A session stops with its maintenance done.
  val allowed = answer == Allow and draft > 0
  // The gate asks for maintenance.
  val requested = answer == Request
  // A session ends incomplete after its one continuation.
  val ended = answer == End
}

module skill_gate {
  import skill_gate_model(OLD = false).*

  run aTriagedDraftAllowsTheStopTest =
    init.then(observe).then(analyze).then(triage).then(stop)
      .expect(answer == Allow and safe)

  run aNewerDraftAsksForMaintenanceOnceTest =
    init.then(observe).then(analyze).then(triage).then(observe).then(analyze).then(stop).then(stop)
      .expect(answer == End and requests == 1 and safe)
}

module skill_gate_old {
  import skill_gate_model(OLD = true).*

  // The regression: a triage between two analyses records the older review, which stands in for the newer draft.
  run triageBetweenTwoAnalysesTest =
    init.then(observe).then(analyze).then(check).then(observe).then(analyze).then(record).then(stop)
      .expect(not(safe))
}
