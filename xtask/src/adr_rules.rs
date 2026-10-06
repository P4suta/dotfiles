//! What an architecture decision record's status requires of its links to later records.

/// The closed status vocabulary of a decision record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Proposed,
    Accepted,
    /// In force, with parts that later records changed.
    Amended,
    /// Replaced by later records.
    Superseded,
    Deprecated,
}

impl Status {
    pub const ALL: [Self; 5] = [
        Self::Proposed,
        Self::Accepted,
        Self::Amended,
        Self::Superseded,
        Self::Deprecated,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Amended => "amended",
            Self::Superseded => "superseded",
            Self::Deprecated => "deprecated",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Consistent,
    /// An amended or superseded record names no record that changed it.
    MissingSuccessor,
    /// A record that later records changed claims a status that hides the change.
    HiddenSuccessor,
}

/// An amended or superseded record names the records that changed it, and no other status names any.
pub const fn verdict(status: Status, successors: usize) -> Verdict {
    match status {
        Status::Amended | Status::Superseded if successors == 0 => Verdict::MissingSuccessor,
        Status::Amended | Status::Superseded => Verdict::Consistent,
        Status::Proposed | Status::Accepted | Status::Deprecated if successors > 0 => {
            Verdict::HiddenSuccessor
        }
        Status::Proposed | Status::Accepted | Status::Deprecated => Verdict::Consistent,
    }
}

#[cfg(kani)]
fn any_status() -> Status {
    let choice: u8 = kani::any();
    kani::assume(choice < 5);
    match choice {
        0 => Status::Proposed,
        1 => Status::Accepted,
        2 => Status::Amended,
        3 => Status::Superseded,
        _ => Status::Deprecated,
    }
}

#[cfg(kani)]
#[kani::proof]
fn a_changed_record_always_names_what_changed_it() {
    let status = any_status();
    let successors: usize = kani::any();
    let result = verdict(status, successors);
    let changed = matches!(status, Status::Amended | Status::Superseded);
    assert!((result == Verdict::MissingSuccessor) == (changed && successors == 0));
    kani::cover!(result == Verdict::MissingSuccessor);
    kani::cover!(result == Verdict::Consistent && changed);
}

#[cfg(kani)]
#[kani::proof]
fn only_a_changed_record_names_successors() {
    let status = any_status();
    let successors: usize = kani::any();
    let result = verdict(status, successors);
    let changed = matches!(status, Status::Amended | Status::Superseded);
    assert!((result == Verdict::HiddenSuccessor) == (!changed && successors > 0));
    kani::cover!(result == Verdict::HiddenSuccessor);
    kani::cover!(result == Verdict::Consistent && !changed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_links_must_agree() {
        assert_eq!(verdict(Status::Accepted, 0), Verdict::Consistent);
        assert_eq!(verdict(Status::Amended, 1), Verdict::Consistent);
        assert_eq!(verdict(Status::Superseded, 0), Verdict::MissingSuccessor);
        assert_eq!(verdict(Status::Accepted, 2), Verdict::HiddenSuccessor);
        assert_eq!(
            Status::ALL.map(Status::name),
            [
                "proposed",
                "accepted",
                "amended",
                "superseded",
                "deprecated"
            ]
        );
    }
}
