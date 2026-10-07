/// The reason the checker refuses a document bound for GitHub.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// A line claims authorship for an agent: a byline trailer, a generated-with footer, or a session URL.
    Attribution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse(Reason),
}

/// The checker never sends a PR, issue, or comment body that carries an attribution line.
#[must_use]
pub fn verdict(attributed: bool) -> Verdict {
    if attributed {
        Verdict::Refuse(Reason::Attribution)
    } else {
        Verdict::Admit
    }
}

#[must_use]
pub const fn next_action(reason: Reason) -> &'static str {
    match reason {
        Reason::Attribution => {
            "Delete that line from the document and send it again; the author's own Co-authored-by lines and prose that mentions Claude are not affected."
        }
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_verdict(attributed: bool) {
        let result = verdict(attributed);
        assert_eq!(result == Verdict::Admit, !attributed);
        if let Verdict::Refuse(reason) = result {
            assert!(next_action(reason).starts_with("Delete that line"));
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    fn attributed_bodies_are_never_admitted_and_each_refusal_names_the_deletion() {
        let attributed: bool = kani::any();
        spec_verdict(attributed);
        kani::cover!(verdict(attributed) == Verdict::Admit);
        kani::cover!(verdict(attributed) == Verdict::Refuse(Reason::Attribution));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        for attributed in [false, true] {
            proofs::spec_verdict(attributed);
        }
    }

    #[test]
    fn an_attributed_body_is_refused_and_a_clean_one_admitted() {
        assert_eq!(verdict(true), Verdict::Refuse(Reason::Attribution));
        assert_eq!(verdict(false), Verdict::Admit);
    }

    #[test]
    fn the_refusal_names_deleting_the_line() {
        assert!(next_action(Reason::Attribution).starts_with("Delete that line"));
    }
}
