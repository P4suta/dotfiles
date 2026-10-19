// How `pr-workflow ready` moves a draft PR to review while other hosts push to its branch and CI reports on each tip.
// `crates/rules/src/pr_ready_rules.rs` implements the same step function, and `crates/pr-workflow/tests/pr_ready_replay.rs` runs the traces of this model through it.
module pr_ready_model {
  // Whether the workflow runs as it did before the fix: it returns right after marking the PR ready, and any passed check counts as complete.
  const BUGGY: bool

  // The number of pushes the environment makes, which bounds the tips.
  pure val PUSHES = 2

  // What the required checks of one tip have reported.
  type Report = Pending | Partial | Complete | Failed

  // Where the command stands: before reading the PR, after the read, after marking it ready, refused, or finished.
  type Phase = Idle | Viewed | Marked | Refused | Returned

  // The head commit of the PR, numbered in push order.
  var tip: int
  // What CI reported on each tip.
  var reports: int -> Report
  // Whether GitHub shows the PR as ready for review.
  var ready: bool
  var phase: Phase
  // The tip the command read together with its checks.
  var viewed: int
  // Whether the read showed every required check of the viewed tip as passed.
  var checked: bool
  // The tip when the command returned, which only the invariant reads.
  var returned_tip: int

  pure def complete(report: Report, buggy: bool): bool =
    if (buggy) report == Partial or report == Complete else report == Complete

  action init = all {
    tip' = 0,
    reports' = 0.to(PUSHES).mapBy(_ => Pending),
    ready' = false,
    phase' = Idle,
    viewed' = -1,
    checked' = false,
    returned_tip' = -1,
  }

  // Another host pushes a new tip, which GitHub accepts whether the PR is ready or not.
  action push = all {
    tip < PUSHES,
    tip' = tip + 1,
    reports' = reports,
    ready' = ready,
    phase' = phase,
    viewed' = viewed,
    checked' = checked,
    returned_tip' = returned_tip,
  }

  // CI reports on the current tip: some required checks, all of them, or a failure.
  action reportAs(outcome: Report): bool = all {
    reports.get(tip) != Complete,
    reports.get(tip) != Failed,
    reports' = reports.set(tip, outcome),
    tip' = tip,
    ready' = ready,
    phase' = phase,
    viewed' = viewed,
    checked' = checked,
    returned_tip' = returned_tip,
  }

  action report = {
    nondet outcome = Set(Partial, Complete, Failed).oneOf()
    reportAs(outcome)
  }

  // The command reads the tip and its checks in one response.
  action view = all {
    phase == Idle,
    phase' = Viewed,
    viewed' = tip,
    checked' = complete(reports.get(tip), BUGGY),
    tip' = tip,
    reports' = reports,
    ready' = ready,
    returned_tip' = returned_tip,
  }

  // The command marks the PR ready only when the read showed every required check passed, and the old command then returned.
  action mark = all {
    phase == Viewed,
    if (checked) all {
      ready' = true,
      phase' = if (BUGGY) Returned else Marked,
      returned_tip' = if (BUGGY) tip else returned_tip,
    } else all {
      ready' = ready,
      phase' = Refused,
      returned_tip' = tip,
    },
    tip' = tip,
    reports' = reports,
    viewed' = viewed,
    checked' = checked,
  }

  // The command reads the tip again and returns the PR to draft when it moved.
  action settle = all {
    phase == Marked,
    ready' = (ready and tip == viewed),
    phase' = Returned,
    returned_tip' = tip,
    tip' = tip,
    reports' = reports,
    viewed' = viewed,
    checked' = checked,
  }

  action step = any { push, report, view, mark, settle }

  // When the command returns with the PR ready, the tip it left ready has every required check passed.
  val safe = (phase == Returned and ready) implies (returned_tip == viewed and reports.get(viewed) == Complete)

  // A run leaves the PR ready.
  val keptReady = phase == Returned and ready
  // A run marks the PR ready and then returns it to draft.
  val returnedToDraft = phase == Returned and not(ready) and checked
  // A run refuses a tip whose checks haven't all passed.
  val refused = phase == Refused
}

module pr_ready {
  import pr_ready_model(BUGGY = false).*

  // Another host pushes between the read and the mark, and the command returns the PR to draft.
  run pushBetweenViewAndReadyTest =
    init.then(reportAs(Complete)).then(view).then(push).then(mark).then(settle)
      .expect(phase == Returned and not(ready) and safe)

  run completeChecksKeepReadyTest =
    init.then(reportAs(Complete)).then(view).then(mark).then(settle)
      .expect(phase == Returned and ready and safe)

  run partialChecksRefuseTest =
    init.then(reportAs(Partial)).then(view).then(mark)
      .expect(phase == Refused and not(ready))
}

module pr_ready_buggy {
  import pr_ready_model(BUGGY = true).*

  // The regression: the old command leaves an unchecked tip ready.
  run pushBetweenViewAndReadyTest =
    init.then(reportAs(Complete)).then(view).then(push).then(mark)
      .expect(not(safe))

  // The old command also took a partial report for a complete one.
  run partialChecksMarkReadyTest =
    init.then(reportAs(Partial)).then(view).then(mark)
      .expect(not(safe))
}
