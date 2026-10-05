/// A shell command that measures disk usage by walking a tree, which storage-scout does with its own index and exclusions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    Refuse,
}

/// The rule refuses a command only when it both walks a tree and reports sizes.
pub const fn verdict(recursive: bool, sizes: bool) -> Verdict {
    if recursive && sizes {
        Verdict::Refuse
    } else {
        Verdict::Admit
    }
}

pub const NEXT_ACTION: &str = "Measure disk usage with `storage-scout scan <path>` and reclaim space with `storage-scout clean`; they skip what a recursive walk should not touch and report what is safe to remove.";

#[cfg(kani)]
#[kani::proof]
fn only_a_recursive_size_scan_is_refused() {
    let recursive: bool = kani::any();
    let sizes: bool = kani::any();
    let result = verdict(recursive, sizes);
    assert_eq!(result == Verdict::Refuse, recursive && sizes);
    kani::cover!(result == Verdict::Refuse);
    kani::cover!(result == Verdict::Admit && recursive);
    kani::cover!(result == Verdict::Admit && sizes);
}

#[cfg(kani)]
#[kani::proof]
fn a_walk_or_a_size_report_alone_is_admitted() {
    let recursive: bool = kani::any();
    let result = verdict(recursive, !recursive);
    assert_eq!(result, Verdict::Admit);
    kani::cover!(recursive);
    kani::cover!(!recursive);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_walk_that_reports_sizes_is_refused() {
        assert_eq!(verdict(true, true), Verdict::Refuse);
        assert_eq!(verdict(true, false), Verdict::Admit);
        assert_eq!(verdict(false, true), Verdict::Admit);
        assert!(NEXT_ACTION.contains("storage-scout scan"));
        assert_eq!(&NEXT_ACTION[25..38], "storage-scout");
    }
}
