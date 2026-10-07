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

    #[must_use]
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
#[must_use]
pub const fn verdict(status: Status, successors: usize) -> Verdict {
    match (status, successors > 0) {
        (Status::Amended | Status::Superseded, false) => Verdict::MissingSuccessor,
        (Status::Proposed | Status::Accepted | Status::Deprecated, true) => {
            Verdict::HiddenSuccessor
        }
        (Status::Amended | Status::Superseded, true)
        | (Status::Proposed | Status::Accepted | Status::Deprecated, false) => Verdict::Consistent,
    }
}

/// The highest number a record's name carries.
/// Every later record takes a dated name from its own decision, which two branches never pick by accident.
pub const LAST_NUMBERED: u16 = 23;

/// How a record's name and front matter identify it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Identity {
    /// A record from before dated names, which keeps its number.
    Numbered,
    /// A record named by its decision and placed by its `date`.
    Dated,
    Refused(Unidentified),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unidentified {
    /// A new record took the next number, which a concurrent branch can take too.
    LateNumber,
    /// A numbered record also names a date, so two orders could place it.
    NumberAndDate,
    /// A record without a number names no date to place it by.
    Undated,
}

impl Unidentified {
    /// The corrective next action for the record.
    #[must_use]
    pub const fn next_action(self) -> &'static str {
        match self {
            Self::LateNumber => {
                "Drop the number from the file name, add `date: YYYY-MM-DD` to the front matter, then run `just gen`."
            }
            Self::NumberAndDate => {
                "Remove `date` from the front matter of the numbered record, then run `just gen`."
            }
            Self::Undated => "Add `date: YYYY-MM-DD` to the front matter, then run `just gen`.",
        }
    }
}

/// The identity of a record whose name starts with `number`, if any, and whose front matter names a date when `dated`.
#[must_use]
pub const fn identity(number: Option<u16>, dated: bool) -> Identity {
    match (number, dated) {
        (Some(number), false) if number <= LAST_NUMBERED => Identity::Numbered,
        (Some(_), false) => Identity::Refused(Unidentified::LateNumber),
        (Some(_), true) => Identity::Refused(Unidentified::NumberAndDate),
        (None, true) => Identity::Dated,
        (None, false) => Identity::Refused(Unidentified::Undated),
    }
}

#[cfg(kani)]
#[kani::proof]
fn only_an_early_number_or_a_date_identifies_a_record() {
    let number: Option<u16> = kani::any();
    let dated: bool = kani::any();
    let result = identity(number, dated);
    assert_eq!(
        result == Identity::Numbered,
        number.is_some_and(|number| number <= LAST_NUMBERED) && !dated
    );
    assert_eq!(result == Identity::Dated, number.is_none() && dated);
    kani::cover!(result == Identity::Numbered);
    kani::cover!(result == Identity::Refused(Unidentified::LateNumber));
}

#[cfg(kani)]
#[kani::proof]
fn every_refused_identity_names_its_fix() {
    let number: Option<u16> = kani::any();
    let dated: bool = kani::any();
    if let Identity::Refused(reason) = identity(number, dated) {
        assert!(!reason.next_action().is_empty());
    }
    kani::cover!(matches!(identity(number, dated), Identity::Refused(_)));
    kani::cover!(identity(number, dated) == Identity::Dated);
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
    fn a_record_takes_an_early_number_or_a_date_and_never_both() {
        assert_eq!(identity(Some(1), false), Identity::Numbered);
        assert_eq!(identity(Some(LAST_NUMBERED), false), Identity::Numbered);
        assert_eq!(
            identity(Some(LAST_NUMBERED + 1), false),
            Identity::Refused(Unidentified::LateNumber)
        );
        assert_eq!(
            identity(Some(1), true),
            Identity::Refused(Unidentified::NumberAndDate)
        );
        assert_eq!(identity(None, true), Identity::Dated);
        assert_eq!(
            identity(None, false),
            Identity::Refused(Unidentified::Undated)
        );
    }

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
