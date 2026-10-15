//! The fixed spoken events and the conditions under which a desktop may queue them.

pub const MAX_PENDING: u8 = 8;
pub const MAX_AGE_MILLIS: u64 = 30_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Event {
    CommitStarted,
    CommitFailed,
    CommitCompleted,
    PushStarted,
    PushFailed,
    PushCompleted,
    TurnCompleted,
    InputWaiting,
}

pub const EVENTS: [Event; 8] = [
    Event::CommitStarted,
    Event::CommitFailed,
    Event::CommitCompleted,
    Event::PushStarted,
    Event::PushFailed,
    Event::PushCompleted,
    Event::TurnCompleted,
    Event::InputWaiting,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Frequency {
    Off,
    Attention,
    Results,
    All,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Desktop {
    Present,
    Headless,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Origin {
    Owner,
    Agent,
    Internal,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Receipt {
    New,
    Duplicate,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    Unknown,
    Disabled,
    Frequency,
    Quiet,
    Clock,
    Stale,
    Duplicate,
    Cancelled,
    Full,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    Queue(Event),
    Quiet(Reason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request {
    pub event: Option<Event>,
    pub frequency: Frequency,
    pub desktop: Desktop,
    pub origin: Origin,
    pub receipt: Receipt,
    pub pending: u8,
    pub issued_millis: u64,
    pub now_millis: u64,
}

#[must_use]
pub fn event(code: u8) -> Option<Event> {
    EVENTS.get(usize::from(code)).copied()
}

#[must_use]
pub fn decide(request: Request) -> Decision {
    let Some(event) = request.event else {
        return Decision::Quiet(Reason::Unknown);
    };
    if request.frequency == Frequency::Off {
        return Decision::Quiet(Reason::Disabled);
    }
    if request.desktop != Desktop::Present || request.origin == Origin::Internal {
        return Decision::Quiet(Reason::Quiet);
    }
    let selected = match request.frequency {
        Frequency::Off => false,
        Frequency::Attention => matches!(
            event,
            Event::CommitFailed | Event::PushFailed | Event::InputWaiting
        ),
        Frequency::Results => !matches!(event, Event::CommitStarted | Event::PushStarted),
        Frequency::All => true,
    };
    if !selected {
        return Decision::Quiet(Reason::Frequency);
    }
    let Some(age) = request.now_millis.checked_sub(request.issued_millis) else {
        return Decision::Quiet(Reason::Clock);
    };
    if age > MAX_AGE_MILLIS {
        return Decision::Quiet(Reason::Stale);
    }
    match request.receipt {
        Receipt::Duplicate => return Decision::Quiet(Reason::Duplicate),
        Receipt::Cancelled => return Decision::Quiet(Reason::Cancelled),
        Receipt::New => {}
    }
    if request.pending >= MAX_PENDING {
        return Decision::Quiet(Reason::Full);
    }
    Decision::Queue(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fresh(event: Option<Event>) -> Request {
        Request {
            event,
            frequency: Frequency::All,
            desktop: Desktop::Present,
            origin: Origin::Owner,
            receipt: Receipt::New,
            pending: 0,
            issued_millis: 1,
            now_millis: 1,
        }
    }
    #[test]
    fn every_fixed_event_has_a_distinct_accepted_code_and_unknown_codes_are_quiet() {
        for code in 0..=u8::MAX {
            let actual = event(code);
            if usize::from(code) < EVENTS.len() {
                assert!(actual.is_some());
                assert!(matches!(decide(fresh(actual)), Decision::Queue(_)));
            } else {
                assert_eq!(decide(fresh(actual)), Decision::Quiet(Reason::Unknown));
            }
        }
        for (index, first) in EVENTS.iter().enumerate() {
            for second in EVENTS.iter().skip(index + 1) {
                assert_ne!(first, second);
            }
        }
    }
    #[test]
    fn each_frequency_selects_exactly_its_documented_events() {
        for current in EVENTS {
            for frequency in [
                Frequency::Off,
                Frequency::Attention,
                Frequency::Results,
                Frequency::All,
            ] {
                let expected = match frequency {
                    Frequency::Off => false,
                    Frequency::Attention => matches!(
                        current,
                        Event::CommitFailed | Event::PushFailed | Event::InputWaiting
                    ),
                    Frequency::Results => {
                        !matches!(current, Event::CommitStarted | Event::PushStarted)
                    }
                    Frequency::All => true,
                };
                assert_eq!(
                    matches!(
                        decide(Request {
                            frequency,
                            ..fresh(Some(current))
                        }),
                        Decision::Queue(_)
                    ),
                    expected
                );
            }
        }
    }
    #[test]
    fn stale_future_duplicate_cancelled_internal_headless_and_full_requests_never_queue() {
        for event in EVENTS {
            let input = fresh(Some(event));
            for request in [
                Request {
                    now_millis: MAX_AGE_MILLIS + 2,
                    ..input
                },
                Request {
                    issued_millis: 2,
                    ..input
                },
                Request {
                    receipt: Receipt::Duplicate,
                    ..input
                },
                Request {
                    receipt: Receipt::Cancelled,
                    ..input
                },
                Request {
                    origin: Origin::Internal,
                    ..input
                },
                Request {
                    desktop: Desktop::Headless,
                    ..input
                },
                Request {
                    desktop: Desktop::Unavailable,
                    ..input
                },
                Request {
                    pending: MAX_PENDING,
                    ..input
                },
                Request {
                    frequency: Frequency::Off,
                    ..input
                },
            ] {
                assert!(!matches!(decide(request), Decision::Queue(_)));
            }
        }
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;
    #[kani::proof]
    fn every_admitted_event_has_fresh_unique_desktop_capacity() {
        let code: u8 = kani::any();
        let frequency: u8 = kani::any();
        let desktop: u8 = kani::any();
        let origin: u8 = kani::any();
        let receipt: u8 = kani::any();
        let input = Request {
            event: event(code),
            frequency: match frequency {
                0 => Frequency::Off,
                1 => Frequency::Attention,
                2 => Frequency::Results,
                _ => Frequency::All,
            },
            desktop: match desktop {
                0 => Desktop::Present,
                1 => Desktop::Headless,
                _ => Desktop::Unavailable,
            },
            origin: match origin {
                0 => Origin::Owner,
                1 => Origin::Agent,
                _ => Origin::Internal,
            },
            receipt: match receipt {
                0 => Receipt::New,
                1 => Receipt::Duplicate,
                _ => Receipt::Cancelled,
            },
            pending: kani::any(),
            issued_millis: kani::any(),
            now_millis: kani::any(),
        };
        if let Decision::Queue(accepted) = decide(input) {
            assert_eq!(Some(accepted), input.event);
            assert_eq!(input.desktop, Desktop::Present);
            assert_ne!(input.origin, Origin::Internal);
            assert_eq!(input.receipt, Receipt::New);
            assert!(input.pending < MAX_PENDING);
            assert!(input.now_millis >= input.issued_millis);
            assert!(input.now_millis - input.issued_millis <= MAX_AGE_MILLIS);
            assert_ne!(input.frequency, Frequency::Off);
            if input.frequency == Frequency::Attention {
                assert!(matches!(
                    accepted,
                    Event::CommitFailed | Event::PushFailed | Event::InputWaiting
                ));
            }
            if input.frequency == Frequency::Results {
                assert!(!matches!(
                    accepted,
                    Event::CommitStarted | Event::PushStarted
                ));
            }
        }
    }
    #[kani::proof]
    fn event_codes_name_only_the_fixed_vocabulary() {
        let code: u8 = kani::any();
        assert_eq!(event(code).is_some(), usize::from(code) < EVENTS.len());
    }
}
