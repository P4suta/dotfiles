/// A shell command that measures disk usage by walking a tree, which storage-scout does with its own index and exclusions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse,
}

/// The rule refuses a command only when it both walks a tree and reports sizes.
#[must_use]
pub const fn verdict(recursive: bool, sizes: bool) -> Verdict {
    if recursive && sizes {
        Verdict::Refuse
    } else {
        Verdict::Admit
    }
}

pub const NEXT_ACTION: &str = "Measure disk usage with `storage-scout scan <path>` and reclaim space with `storage-scout clean`; they skip what a recursive walk should not touch and report what is safe to remove.";

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_verdict(recursive: bool, sizes: bool) {
        assert_eq!(
            verdict(recursive, sizes) == Verdict::Refuse,
            recursive && sizes
        );
        assert_eq!(verdict(recursive, !recursive), Verdict::Admit);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn only_a_recursive_size_scan_is_refused() {
        let recursive: bool = kani::any();
        let sizes: bool = kani::any();
        spec_verdict(recursive, sizes);
        let result = verdict(recursive, sizes);
        kani::cover!(result == Verdict::Refuse);
        kani::cover!(result == Verdict::Admit && recursive);
        kani::cover!(result == Verdict::Admit && sizes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        for [recursive, sizes] in crate::domain::flags() {
            proofs::spec_verdict(recursive, sizes);
        }
    }

    #[test]
    fn a_tree_walk_that_reports_sizes_is_refused() {
        assert_eq!(verdict(true, true), Verdict::Refuse);
        assert_eq!(verdict(true, false), Verdict::Admit);
        assert_eq!(verdict(false, true), Verdict::Admit);
        assert!(NEXT_ACTION.contains("storage-scout scan"));
        assert_eq!(NEXT_ACTION.get(25..38), Some("storage-scout"));
    }
}
