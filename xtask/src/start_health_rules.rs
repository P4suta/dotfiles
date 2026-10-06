/// How long a trivial process took from creation to exit, or that it had not exited when the probe stopped waiting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Start {
    Completed { millis: u64 },
    TimedOut,
}

/// The Defender and service control events from the last hour that mark a failing scan engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Events {
    /// Defender event 5008: the engine terminated.
    pub engine_terminated: u32,
    /// Defender event 3002: real-time protection failed.
    pub protection_failed: u32,
    /// Service Control Manager event 7011: a service missed its response deadline.
    pub service_timeout: u32,
}

impl Events {
    pub const NONE: Self = Self {
        engine_terminated: 0,
        protection_failed: 0,
        service_timeout: 0,
    };

    pub const fn any(self) -> bool {
        self.engine_terminated > 0 || self.protection_failed > 0 || self.service_timeout > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Healthy,
    Stalled,
}

/// A healthy host starts and finishes a trivial process well below this.
pub const SLOW_MILLIS: u64 = 1_500;
/// The probe stops waiting here, which bounds how long a gate can hang on process creation.
pub const STALL_MILLIS: u64 = 5_000;

/// Whether the event query earns its own process start.
pub const fn needs_events(start: Start) -> bool {
    match start {
        Start::Completed { millis } => millis >= SLOW_MILLIS,
        Start::TimedOut => true,
    }
}

/// A start that never finished or took the full bound counts as a stall on its own.
/// A slow start counts as a stall when the engine reported a failure, or when the event query, `None`, also failed to finish.
pub const fn verdict(start: Start, events: Option<Events>) -> Verdict {
    match start {
        Start::TimedOut => Verdict::Stalled,
        Start::Completed { millis } if millis >= STALL_MILLIS => Verdict::Stalled,
        Start::Completed { millis } if millis >= SLOW_MILLIS => match events {
            Some(events) if !events.any() => Verdict::Healthy,
            _ => Verdict::Stalled,
        },
        Start::Completed { .. } => Verdict::Healthy,
    }
}

/// Defender protection stays as configured, so the refusal offers only a restart of the host.
pub const NEXT_ACTION: &str = "Restart-Computer";

#[cfg(kani)]
fn any_start() -> Start {
    if kani::any() {
        Start::TimedOut
    } else {
        Start::Completed {
            millis: kani::any(),
        }
    }
}

#[cfg(kani)]
fn any_events() -> Option<Events> {
    if kani::any() {
        None
    } else {
        Some(Events {
            engine_terminated: kani::any(),
            protection_failed: kani::any(),
            service_timeout: kani::any(),
        })
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_start_that_reaches_the_bound_is_always_a_stall() {
    let start = any_start();
    let events = any_events();
    let result = verdict(start, events);
    let bounded = match start {
        Start::TimedOut => true,
        Start::Completed { millis } => millis >= STALL_MILLIS,
    };
    if bounded {
        assert!(result == Verdict::Stalled);
    }
    assert!(result == Verdict::Healthy || needs_events(start));
    kani::cover!(result == Verdict::Healthy);
    kani::cover!(result == Verdict::Stalled && !bounded);
}

#[cfg(kani)]
#[kani::proof]
fn a_fast_start_is_healthy_whatever_the_events() {
    let millis: u64 = kani::any();
    let events = any_events();
    let start = Start::Completed { millis };
    let result = verdict(start, events);
    if millis < SLOW_MILLIS {
        assert!(result == Verdict::Healthy && !needs_events(start));
    }
    kani::cover!(millis < SLOW_MILLIS && events.is_some_and(Events::any));
    kani::cover!(result == Verdict::Stalled);
    assert!(!NEXT_ACTION.is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAILED_ENGINE: Events = Events {
        engine_terminated: 1,
        ..Events::NONE
    };

    #[test]
    fn a_start_that_never_finishes_is_a_stall() {
        assert_eq!(verdict(Start::TimedOut, None), Verdict::Stalled);
        assert_eq!(
            verdict(
                Start::Completed {
                    millis: STALL_MILLIS
                },
                Some(Events::NONE)
            ),
            Verdict::Stalled
        );
    }

    #[test]
    fn a_slow_start_is_a_stall_only_with_engine_evidence() {
        let slow = Start::Completed {
            millis: SLOW_MILLIS,
        };
        assert_eq!(verdict(slow, Some(Events::NONE)), Verdict::Healthy);
        assert_eq!(verdict(slow, Some(FAILED_ENGINE)), Verdict::Stalled);
        assert_eq!(verdict(slow, None), Verdict::Stalled);
    }

    #[test]
    fn a_fast_start_ignores_past_events() {
        let fast = Start::Completed { millis: 40 };
        assert!(!needs_events(fast));
        assert_eq!(verdict(fast, Some(FAILED_ENGINE)), Verdict::Healthy);
    }
}
