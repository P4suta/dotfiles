//! Which code shapes a repository must own, and what each enforcement point decides about the gaps a change leaves.
//!
//! A shape names code that needs verification beyond example tests, like a parser.
//! Data names the signals that find each shape, and each site they find has an owner, an exemption beside the code, or a gap.

/// The closed reasons an exemption may give.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    /// The property depends on an external system or operating system behavior that no verifier models.
    SemanticBoundary,
    /// The site passes values through with no branch and no arithmetic.
    NoDecision,
    /// The cost of verifying the site outweighs the risk.
    Disproportionate,
}

/// One detection signal: a line of a file whose path ends with `suffix` and that starts with `prefix` after its leading blanks.
/// An empty prefix finds the file as a whole, on its first line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signal<'a> {
    pub suffix: &'a [u8],
    pub prefix: &'a [u8],
}

/// The rest of `text` after `prefix`, or `None` when `text` doesn't start with it.
fn after<'a>(mut text: &'a [u8], mut prefix: &[u8]) -> Option<&'a [u8]> {
    while let [expected, rest @ ..] = prefix {
        let [actual, tail @ ..] = text else {
            return None;
        };
        if actual != expected {
            return None;
        }
        (text, prefix) = (tail, rest);
    }
    Some(text)
}

fn ends_with(mut text: &[u8], mut suffix: &[u8]) -> bool {
    while let [rest @ .., expected] = suffix {
        let [head @ .., actual] = text else {
            return false;
        };
        if actual != expected {
            return false;
        }
        (text, suffix) = (head, rest);
    }
    true
}

/// `text` without its leading spaces and tabs.
fn trimmed(mut text: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = text {
        text = rest;
    }
    text
}

/// Whether line `line` of the file at `path` holds a site of `signal`.
/// `first` holds when `line` opens the file.
#[must_use]
pub fn found(signal: Signal<'_>, path: &[u8], first: bool, line: &[u8]) -> bool {
    ends_with(path, signal.suffix)
        && if signal.prefix.is_empty() {
            first
        } else {
            after(trimmed(line), signal.prefix).is_some()
        }
}

/// Whether one `/`-separated segment of `path` equals `segment`, as a test, fixture, or fuzz directory does.
#[must_use]
pub fn within(path: &[u8], segment: &[u8]) -> bool {
    let mut separated = segment;
    while let [byte, rest @ ..] = separated {
        if *byte == b'/' {
            return false;
        }
        separated = rest;
    }
    let mut rest = path;
    loop {
        if let Some([] | [b'/', ..]) = after(rest, segment) {
            return true;
        }
        loop {
            match rest {
                [b'/', tail @ ..] => {
                    rest = tail;
                    break;
                }
                [_, tail @ ..] => rest = tail,
                [] => return false,
            }
        }
    }
}

/// Whether `line` belongs to the comment and attribute block before a site, which ends at a blank or code line.
#[must_use]
pub fn annotation(line: &[u8]) -> bool {
    matches!(
        trimmed(line),
        [b'/', b'/' | b'*', ..] | [b'#' | b'*', ..] | [b'-', b'-', ..]
    )
}

/// What an `Exempt as <reason>: <justification>` comment declares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exemption {
    Exempt(Reason),
    /// The comment names no closed reason or gives no justification.
    Malformed,
}

const MARKER: &[u8] = b"Exempt as ";

/// The key of each closed reason, as an exemption names it.
pub const REASONS: [(&[u8], Reason); 3] = [
    (b"semantic-boundary", Reason::SemanticBoundary),
    (b"no-decision", Reason::NoDecision),
    (b"disproportionate", Reason::Disproportionate),
];

/// The exemption one comment line declares, or `None` when it declares none.
/// It names exactly one key of [`REASONS`], a colon, and a justification with a visible character.
#[must_use]
pub fn exemption(line: &[u8]) -> Option<Exemption> {
    let mut rest = line;
    let declared = loop {
        if let Some(declared) = after(rest, MARKER) {
            break declared;
        }
        let [_, tail @ ..] = rest else {
            return None;
        };
        rest = tail;
    };
    let mut keys: &[(&[u8], Reason)] = &REASONS;
    while let [(key, reason), others @ ..] = keys {
        if let Some([b':', justification @ ..]) = after(declared, key) {
            return Some(match trimmed(justification) {
                [] => Exemption::Malformed,
                _ => Exemption::Exempt(*reason),
            });
        }
        keys = others;
    }
    Some(Exemption::Malformed)
}

/// What owns the sites of one file, as the repository's owners file declares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Owner {
    /// No entry names the file.
    Undeclared,
    /// The entry's owning file carries the shape's marker, and its corpus when the shape needs one.
    Present,
    /// The entry names an owning file that the tree lacks, or that lacks the marker or its corpus.
    Broken,
}

/// The facts about one site that decide its standing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Site {
    /// The site's own file carries the shape's marker.
    pub marker: bool,
    /// The shape accepts the marker in the site's own file, as a Kani harness beside a decision.
    pub in_file: bool,
    pub owner: Owner,
    pub exemption: Option<Exemption>,
}

/// Why a site counts as a gap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cause {
    Unowned,
    /// The owners file names an owner that doesn't hold.
    BrokenOwner,
    MalformedExemption,
    /// The site has an owner and an exemption, so the exemption no longer says anything.
    StaleExemption,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Standing {
    Owned,
    Exempt(Reason),
    Gap(Cause),
}

#[must_use]
pub const fn standing(site: Site) -> Standing {
    let owned = (site.marker && site.in_file) || matches!(site.owner, Owner::Present);
    match (site.exemption, owned, site.owner) {
        (Some(Exemption::Malformed), _, _) => Standing::Gap(Cause::MalformedExemption),
        (Some(Exemption::Exempt(_)), true, _) => Standing::Gap(Cause::StaleExemption),
        (Some(Exemption::Exempt(reason)), false, _) => Standing::Exempt(reason),
        (None, true, _) => Standing::Owned,
        (None, false, Owner::Broken) => Standing::Gap(Cause::BrokenOwner),
        (None, false, _) => Standing::Gap(Cause::Unowned),
    }
}

/// How many entries of `later` `before` lacks, as the gaps a change adds or, swapped, closes.
#[must_use]
pub fn fresh(before: &[&[u8]], later: &[&[u8]]) -> usize {
    let mut count = 0;
    let mut rest = later;
    while let [entry, tail @ ..] = rest {
        let mut known = false;
        let mut scan = before;
        while let [candidate, others @ ..] = scan {
            if let Some([]) = after(candidate, entry) {
                known = true;
                break;
            }
            scan = others;
        }
        if !known {
            count += 1;
        }
        rest = tail;
    }
    count
}

/// Which way a budget may never move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bound {
    /// A value such as surviving mutants or an instruction count, which may never rise.
    Ceiling,
    /// A value such as a mutation score, which may never fall.
    Floor,
}

/// Whether a budget became weaker: its value moved the wrong way, or the change removed it.
#[must_use]
pub const fn lowered(bound: Bound, before: u64, after: Option<u64>) -> bool {
    match (bound, after) {
        (_, None) => true,
        (Bound::Ceiling, Some(after)) => after > before,
        (Bound::Floor, Some(after)) => after < before,
    }
}

/// The machine-level enforcement points.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Point {
    /// A session start, which reports the gaps with their next actions and never refuses.
    Start,
    Commit,
    Push,
    /// An agent's end of turn.
    Stop,
    /// `pr-workflow ready`.
    Ready,
}

/// What a change does to the gaps and budgets of one repository.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Change {
    /// Gaps after the change that it didn't find before.
    pub added: usize,
    /// Gaps before the change that it removed.
    pub closed: usize,
    pub before: usize,
    pub after: usize,
    /// Gaps after the change in files the change touched.
    pub touched: usize,
    /// Budgets the change weakened or removed.
    pub lowered: usize,
    /// The change only reverts earlier commits, so whatever it restores already stood in the history.
    pub reverted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Admit,
    /// The repository has gaps, which the session start lists with their next actions.
    Report,
    /// The change adds a gap.
    Added,
    /// A file the agent touched still holds a gap.
    Touched,
    /// The change weakens a budget.
    Lowered,
    /// The repository has gaps and the PR closes none.
    Stalled,
}

impl Verdict {
    #[must_use]
    pub const fn refuses(self) -> bool {
        !matches!(self, Self::Admit | Self::Report)
    }
}

/// The decision of `point` about `change`.
/// Commits and pushes refuse only a gap the change adds, never one that already existed.
/// A commit, push, or PR made only of reverts passes, because it restores code that already stood.
#[must_use]
pub const fn verdict(point: Point, change: Change) -> Verdict {
    match point {
        Point::Start if change.after > 0 => Verdict::Report,
        Point::Commit | Point::Push | Point::Ready if change.reverted => Verdict::Admit,
        Point::Commit | Point::Push | Point::Stop | Point::Ready if change.added > 0 => {
            Verdict::Added
        }
        Point::Stop if change.touched > 0 => Verdict::Touched,
        Point::Ready if change.lowered > 0 => Verdict::Lowered,
        Point::Ready if change.before > 0 && change.closed == 0 => Verdict::Stalled,
        _ => Verdict::Admit,
    }
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub const POINTS: [Point; 5] = [
        Point::Start,
        Point::Commit,
        Point::Push,
        Point::Stop,
        Point::Ready,
    ];

    pub const OWNERS: [Owner; 3] = [Owner::Undeclared, Owner::Present, Owner::Broken];

    pub const EXEMPTIONS: [Option<Exemption>; 5] = [
        None,
        Some(Exemption::Malformed),
        Some(Exemption::Exempt(Reason::SemanticBoundary)),
        Some(Exemption::Exempt(Reason::NoDecision)),
        Some(Exemption::Exempt(Reason::Disproportionate)),
    ];

    /// Only the gaps a change adds refuse a commit or a push, a session start never refuses, and a PR in a repository with gaps must close one.
    /// A revert passes every point but the end of a turn.
    pub fn spec_verdict(point: Point, change: Change) {
        let result = verdict(point, change);
        let waived = change.reverted && matches!(point, Point::Commit | Point::Push | Point::Ready);
        assert!(!waived || result == Verdict::Admit);
        assert_eq!(
            result == Verdict::Report,
            point == Point::Start && change.after > 0
        );
        assert_eq!(
            result == Verdict::Added,
            point != Point::Start && !waived && change.added > 0
        );
        if matches!(point, Point::Commit | Point::Push) {
            assert_eq!(result.refuses(), !waived && change.added > 0);
        }
        assert_eq!(
            result == Verdict::Touched,
            point == Point::Stop && change.added == 0 && change.touched > 0
        );
        assert_eq!(
            result == Verdict::Lowered,
            point == Point::Ready && !waived && change.added == 0 && change.lowered > 0
        );
        assert_eq!(
            result == Verdict::Stalled,
            point == Point::Ready
                && !waived
                && change.added == 0
                && change.lowered == 0
                && change.before > 0
                && change.closed == 0
        );
        assert_eq!(
            result.refuses(),
            !matches!(result, Verdict::Admit | Verdict::Report)
        );
    }

    /// A site has exactly one owner that holds, and every other combination makes a gap with its cause.
    pub fn spec_standing(site: Site) {
        let owned = (site.marker && site.in_file) || site.owner == Owner::Present;
        let result = standing(site);
        assert_eq!(result == Standing::Owned, owned && site.exemption.is_none());
        assert_eq!(
            matches!(result, Standing::Exempt(_)),
            !owned && matches!(site.exemption, Some(Exemption::Exempt(_)))
        );
        if let (Standing::Exempt(reason), Some(Exemption::Exempt(declared))) =
            (result, site.exemption)
        {
            assert_eq!(reason, declared);
        }
        assert_eq!(
            result == Standing::Gap(Cause::MalformedExemption),
            site.exemption == Some(Exemption::Malformed)
        );
        assert_eq!(
            result == Standing::Gap(Cause::Unowned),
            !owned && site.exemption.is_none() && site.owner != Owner::Broken
        );
        assert_eq!(
            result == Standing::Gap(Cause::BrokenOwner),
            !owned && site.exemption.is_none() && site.owner == Owner::Broken
        );
    }

    pub fn spec_lowered(ceiling: bool, before: u64, after: Option<u64>) {
        let bound = if ceiling {
            Bound::Ceiling
        } else {
            Bound::Floor
        };
        let expected = match after {
            None => true,
            Some(after) if ceiling => after > before,
            Some(after) => after < before,
        };
        assert_eq!(lowered(bound, before, after), expected);
        if after == Some(before) {
            assert!(!lowered(bound, before, after));
        }
    }

    /// The rest of `text` after `prefix`, compared byte by byte from the front.
    fn skip<'a>(text: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
        let (mut text, mut prefix) = (text, prefix);
        loop {
            match (text, prefix) {
                (_, []) => return Some(text),
                ([], _) => return None,
                ([a, text_rest @ ..], [b, prefix_rest @ ..]) => {
                    if a != b {
                        return None;
                    }
                    (text, prefix) = (text_rest, prefix_rest);
                }
            }
        }
    }

    fn begins(text: &[u8], prefix: &[u8]) -> bool {
        skip(text, prefix).is_some()
    }

    /// Whether `text` finishes with `suffix`, compared byte by byte from the back.
    fn finishes(text: &[u8], suffix: &[u8]) -> bool {
        let (mut text, mut suffix) = (text, suffix);
        loop {
            match (text, suffix) {
                (_, []) => return true,
                ([], _) => return false,
                ([text_rest @ .., a], [suffix_rest @ .., b]) => {
                    if a != b {
                        return false;
                    }
                    (text, suffix) = (text_rest, suffix_rest);
                }
            }
        }
    }

    /// `line` from its first byte other than a space or a tab.
    fn body(line: &[u8]) -> &[u8] {
        match line {
            [b' ' | b'\t', rest @ ..] => body(rest),
            _ => line,
        }
    }

    /// An exemption counts only after the marker, with one closed reason, a colon, and a justification with a visible character.
    #[expect(
        clippy::manual_assert_eq,
        reason = "assert_eq formats its operands, which leaves Kani an unreachable formatting check that fails the proof"
    )]
    pub fn spec_exemption(line: &[u8]) {
        let mut marked = None;
        let mut rest = line;
        loop {
            if begins(rest, MARKER) {
                marked = Some(rest);
                break;
            }
            match rest {
                [_, tail @ ..] => rest = tail,
                [] => break,
            }
        }
        let result = exemption(line);
        assert!(result.is_some() == marked.is_some());
        let justified = |key: &[u8]| {
            marked
                .and_then(|start| skip(start, MARKER))
                .and_then(|rest| skip(rest, key))
                .and_then(|rest| skip(rest, b":"))
                .is_some_and(|text| !body(text).is_empty())
        };
        let names = |key: &[u8], reason| {
            assert!(justified(key) == (result == Some(Exemption::Exempt(reason))));
        };
        names(b"semantic-boundary", Reason::SemanticBoundary);
        names(b"no-decision", Reason::NoDecision);
        names(b"disproportionate", Reason::Disproportionate);
    }

    /// `fresh` counts exactly the entries of `after` that `before` lacks.
    #[expect(
        clippy::manual_assert_eq,
        reason = "assert_eq formats its operands, which leaves Kani an unreachable formatting check that fails the proof"
    )]
    pub fn spec_fresh(before: &[&[u8]], after: &[&[u8]]) {
        let mut expected = 0;
        let mut rest = after;
        while let [entry, tail @ ..] = rest {
            let mut scan = before;
            let mut known = false;
            while let [candidate, others @ ..] = scan {
                known |= begins(candidate, entry) && begins(entry, candidate);
                scan = others;
            }
            expected += usize::from(!known);
            rest = tail;
        }
        assert!(fresh(before, after) == expected);
        assert!(fresh(after, after) == 0);
    }

    pub fn spec_found(signal: Signal<'_>, path: &[u8], first: bool, line: &[u8]) {
        let expected = finishes(path, signal.suffix)
            && if signal.prefix.is_empty() {
                first
            } else {
                begins(body(line), signal.prefix)
            };
        assert_eq!(found(signal, path, first, line), expected);
    }

    /// Some part of `path` between separators equals `segment` byte for byte.
    pub fn spec_within(path: &[u8], segment: &[u8]) {
        let mut expected = false;
        let mut rest = path;
        loop {
            let (mut part, mut wanted) = (rest, segment);
            let equal = loop {
                match (part, wanted) {
                    ([] | [b'/', ..], []) => break true,
                    ([] | [b'/', ..], [_, ..]) | ([_, ..], []) => break false,
                    ([a, part_rest @ ..], [b, wanted_rest @ ..]) => {
                        if a != b {
                            break false;
                        }
                        (part, wanted) = (part_rest, wanted_rest);
                    }
                }
            };
            expected |= equal;
            let mut scan = rest;
            let mut next = None;
            while let [byte, tail @ ..] = scan {
                if *byte == b'/' {
                    next = Some(tail);
                    break;
                }
                scan = tail;
            }
            match next {
                Some(tail) => rest = tail,
                None => break,
            }
        }
        assert_eq!(within(path, segment), expected);
    }

    pub fn spec_annotation(line: &[u8]) {
        let start = body(line);
        let expected = begins(start, b"//")
            || begins(start, b"/*")
            || begins(start, b"#")
            || begins(start, b"*")
            || begins(start, b"--");
        assert_eq!(annotation(line), expected);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_commit_or_push_refuses_only_an_added_gap_and_ready_requires_progress() {
        let point = POINTS[usize::from(kani::any::<u8>() % 5)];
        let change = Change {
            added: kani::any(),
            closed: kani::any(),
            before: kani::any(),
            after: kani::any(),
            touched: kani::any(),
            lowered: kani::any(),
            reverted: kani::any(),
        };
        spec_verdict(point, change);
        let result = verdict(point, change);
        kani::cover!(result == Verdict::Stalled);
        kani::cover!(result == Verdict::Touched);
        kani::cover!(point == Point::Commit && change.before > 0 && result == Verdict::Admit);
        kani::cover!(point == Point::Push && change.added > 0 && result == Verdict::Admit);
        kani::cover!(change.reverted && result == Verdict::Added);
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_site_has_one_owner_that_holds_or_is_a_gap_with_its_cause() {
        let site = Site {
            marker: kani::any(),
            in_file: kani::any(),
            owner: OWNERS[usize::from(kani::any::<u8>() % 3)],
            exemption: EXEMPTIONS[usize::from(kani::any::<u8>() % 5)],
        };
        spec_standing(site);
        kani::cover!(standing(site) == Standing::Gap(Cause::StaleExemption));
        kani::cover!(standing(site) == Standing::Gap(Cause::BrokenOwner));
    }

    #[cfg(kani)]
    #[kani::proof]
    fn a_budget_weakens_only_by_moving_the_wrong_way_or_disappearing() {
        let ceiling: bool = kani::any();
        let before: u64 = kani::any();
        let after: u64 = kani::any();
        let present: bool = kani::any();
        let after = present.then_some(after);
        spec_lowered(ceiling, before, after);
        let bound = if ceiling {
            Bound::Ceiling
        } else {
            Bound::Floor
        };
        kani::cover!(lowered(bound, before, after) && present);
        kani::cover!(!lowered(bound, before, after));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(28)]
    fn an_exemption_needs_a_closed_reason_and_a_justification() {
        let mut line: [u8; 26] = *b"# Exempt as no-decision:xx";
        if let [.., first, second] = &mut line {
            (*first, *second) = (kani::any(), kani::any());
        }
        spec_exemption(&line);
        let result = exemption(&line);
        let exempt = matches!(result, Some(Exemption::Exempt(_)));
        let malformed = matches!(result, Some(Exemption::Malformed));
        kani::cover!(exempt);
        kani::cover!(malformed);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn fresh_counts_exactly_the_entries_before_lacks() {
        let (a, b, c, d): ([u8; 2], [u8; 2], [u8; 2], [u8; 2]) =
            (kani::any(), kani::any(), kani::any(), kani::any());
        let before: [&[u8]; 2] = [&a, &b];
        let after: [&[u8]; 2] = [&c, &d];
        spec_fresh(&before, &after);
        kani::cover!(fresh(&before, &after) == 1);
        kani::cover!(fresh(&before, &after) == 0);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(8)]
    fn a_signal_finds_its_prefix_after_blanks_or_the_first_line_of_its_files() {
        let path: [u8; 3] = kani::any();
        let line: [u8; 4] = kani::any();
        let suffix: [u8; 1] = kani::any();
        let prefix: [u8; 2] = kani::any();
        let short: bool = kani::any();
        let prefix = if short { &prefix[..0] } else { &prefix[..] };
        let first: bool = kani::any();
        let signal = Signal {
            suffix: &suffix,
            prefix,
        };
        spec_found(signal, &path, first, &line);
        kani::cover!(found(signal, &path, first, &line) && !short);
        kani::cover!(!found(signal, &path, first, &line));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn a_segment_matches_only_a_whole_path_segment() {
        let path: [u8; 4] = kani::any();
        let segment: [u8; 2] = kani::any();
        spec_within(&path, &segment);
        kani::cover!(within(&path, &segment));
        kani::cover!(!within(&path, &segment));
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(6)]
    fn an_annotation_line_starts_with_a_comment_or_attribute_marker() {
        let line: [u8; 4] = kani::any();
        spec_annotation(&line);
        kani::cover!(annotation(&line));
        kani::cover!(!annotation(&line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri takes seconds over every change, site, path, and line"
    )]
    fn every_input_meets_the_specifications() {
        for point in proofs::POINTS {
            for added in 0..2 {
                for closed in 0..2 {
                    for before in 0..2 {
                        for touched in 0..2 {
                            for lowered in 0..2 {
                                for after in 0..2 {
                                    for reverted in [false, true] {
                                        proofs::spec_verdict(
                                            point,
                                            Change {
                                                added,
                                                closed,
                                                before,
                                                after,
                                                touched,
                                                lowered,
                                                reverted,
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        for owner in proofs::OWNERS {
            for exemption in proofs::EXEMPTIONS {
                for [marker, in_file] in crate::domain::flags() {
                    proofs::spec_standing(Site {
                        marker,
                        in_file,
                        owner,
                        exemption,
                    });
                }
            }
        }
        for ceiling in [false, true] {
            for before in [0, 1, 2, u64::MAX] {
                for after in [None, Some(0), Some(1), Some(2), Some(u64::MAX)] {
                    proofs::spec_lowered(ceiling, before, after);
                }
            }
        }
        for path in crate::domain::words(b"a/", 3) {
            for segment in crate::domain::words(b"a", 1) {
                proofs::spec_within(&path, &segment);
            }
        }
        for line in crate::domain::words(b" /#*-a", 2) {
            proofs::spec_annotation(&line);
        }
        let entries = crate::domain::words(b"ab", 1);
        let slices: alloc::vec::Vec<&[u8]> =
            entries.iter().map(alloc::vec::Vec::as_slice).collect();
        for before in crate::domain::words(&[0, 1], 2) {
            for after in crate::domain::words(&[1, 2], 2) {
                let pick = |indices: &[u8]| -> alloc::vec::Vec<&[u8]> {
                    indices
                        .iter()
                        .filter_map(|index| slices.get(usize::from(*index)).copied())
                        .collect()
                };
                proofs::spec_fresh(&pick(&before), &pick(&after));
            }
        }
        for path in crate::domain::words(b"ab", 1) {
            for line in crate::domain::words(b" ab", 2) {
                for suffix in crate::domain::words(b"b", 1) {
                    for prefix in crate::domain::words(b"ab", 1) {
                        for first in [false, true] {
                            proofs::spec_found(
                                Signal {
                                    suffix: &suffix,
                                    prefix: &prefix,
                                },
                                &path,
                                first,
                                &line,
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri takes seconds over every short line")]
    fn exemption_lines_meet_the_specification() {
        for line in [
            &b"// Exempt as semantic-boundary: the OS decides"[..],
            b"/// Exempt as no-decision: passes through",
            b"# Exempt as disproportionate: x",
            b"Exempt as disproportionate:",
            b"Exempt as disproportionate: \t",
            b"Exempt as other: why",
            b"Exempt as no-decision",
            b"Exempt as ",
            b"Exempt a",
            b"",
            b"plain comment",
            b"xExempt as no-decision: y",
        ] {
            proofs::spec_exemption(line);
        }
        for line in crate::domain::words(b"Ex :", 3) {
            proofs::spec_exemption(&line);
        }
    }

    /// Every example below in one test, since Miri pays for each test thread it starts.
    #[test]
    fn every_example_meets_its_rule() {
        an_exemption_names_one_closed_reason_and_a_justification();
        signals_find_their_prefix_after_blanks_and_whole_files_on_the_first_line();
        segments_match_whole_directories_only();
        annotations_are_comments_and_attributes();
        a_site_is_owned_by_an_in_file_marker_or_a_present_owner();
        an_exemption_stands_alone_and_a_malformed_one_is_a_gap();
        fresh_counts_the_entries_the_other_side_lacks();
        a_budget_weakens_when_it_moves_the_wrong_way_or_disappears();
        old_gaps_never_refuse_a_commit_or_push_but_ready_requires_closing_one();
        reasons_are_the_closed_set_and_a_key_must_end_at_the_colon();
    }

    fn an_exemption_names_one_closed_reason_and_a_justification() {
        assert_eq!(
            exemption(b"    // Exempt as semantic-boundary: Git decides the merge base"),
            Some(Exemption::Exempt(Reason::SemanticBoundary))
        );
        assert_eq!(
            exemption(b"# Exempt as no-decision: x"),
            Some(Exemption::Exempt(Reason::NoDecision))
        );
        assert_eq!(
            exemption(b"// Exempt as disproportionate: one fixture"),
            Some(Exemption::Exempt(Reason::Disproportionate))
        );
        assert_eq!(
            exemption(b"// Exempt as convenience: later"),
            Some(Exemption::Malformed)
        );
        assert_eq!(
            exemption(b"// Exempt as disproportionate:   "),
            Some(Exemption::Malformed)
        );
        assert_eq!(
            exemption(b"// Exempt as disproportionate"),
            Some(Exemption::Malformed)
        );
        assert_eq!(exemption(b"// an ordinary comment"), None);
    }

    fn signals_find_their_prefix_after_blanks_and_whole_files_on_the_first_line() {
        let parser = Signal {
            suffix: b".rs",
            prefix: b"pub fn parse(",
        };
        assert!(found(
            parser,
            b"src/a.rs",
            false,
            b"    pub fn parse(text: &str)"
        ));
        assert!(found(parser, b"src/a.rs", false, b"\tpub fn parse("));
        assert!(!found(parser, b"src/a.ts", false, b"pub fn parse("));
        assert!(!found(parser, b"src/a.rs", false, b"pub fn parsed("));
        assert!(!found(parser, b"src/a.rs", false, b"// pub fn parse("));
        let module = Signal {
            suffix: b"_rules.rs",
            prefix: b"",
        };
        assert!(found(module, b"src/gate_rules.rs", true, b"//! doc"));
        assert!(!found(module, b"src/gate_rules.rs", false, b"//! doc"));
        assert!(!found(module, b"src/gate.rs", true, b"//! doc"));
    }

    fn segments_match_whole_directories_only() {
        assert!(within(b"crates/x/tests/a.rs", b"tests"));
        assert!(within(b"tests/a.rs", b"tests"));
        assert!(within(b"fuzz", b"fuzz"));
        assert!(!within(b"crates/x/src/tests.rs", b"tests"));
        assert!(!within(b"contests/a.rs", b"tests"));
        assert!(!within(b"a/testsuite/b.rs", b"tests"));
        assert!(within(b"a//b", b""));
        assert!(!within(b"a/b", b""));
    }

    fn annotations_are_comments_and_attributes() {
        for line in [
            &b"/// doc"[..],
            b"  // note",
            b"#[must_use]",
            b"# shell comment",
            b" * inside a block",
            b"/* block",
            b"-- lua",
        ] {
            assert!(annotation(line), "{line:?}");
        }
        for line in [&b""[..], b"   ", b"pub fn parse(", b"/ x", b"- item"] {
            assert!(!annotation(line), "{line:?}");
        }
    }

    const BARE: Site = Site {
        marker: false,
        in_file: false,
        owner: Owner::Undeclared,
        exemption: None,
    };

    fn a_site_is_owned_by_an_in_file_marker_or_a_present_owner() {
        assert_eq!(standing(BARE), Standing::Gap(Cause::Unowned));
        assert_eq!(
            standing(Site {
                marker: true,
                ..BARE
            }),
            Standing::Gap(Cause::Unowned)
        );
        assert_eq!(
            standing(Site {
                marker: true,
                in_file: true,
                ..BARE
            }),
            Standing::Owned
        );
        assert_eq!(
            standing(Site {
                owner: Owner::Present,
                ..BARE
            }),
            Standing::Owned
        );
        assert_eq!(
            standing(Site {
                owner: Owner::Broken,
                ..BARE
            }),
            Standing::Gap(Cause::BrokenOwner)
        );
    }

    fn an_exemption_stands_alone_and_a_malformed_one_is_a_gap() {
        let exempt = Site {
            exemption: Some(Exemption::Exempt(Reason::Disproportionate)),
            ..BARE
        };
        assert_eq!(standing(exempt), Standing::Exempt(Reason::Disproportionate));
        assert_eq!(
            standing(Site {
                owner: Owner::Present,
                ..exempt
            }),
            Standing::Gap(Cause::StaleExemption)
        );
        assert_eq!(
            standing(Site {
                owner: Owner::Present,
                exemption: Some(Exemption::Malformed),
                ..BARE
            }),
            Standing::Gap(Cause::MalformedExemption)
        );
    }

    fn fresh_counts_the_entries_the_other_side_lacks() {
        let before: [&[u8]; 2] = [b"a", b"b"];
        let after: [&[u8]; 3] = [b"b", b"c", b"d"];
        assert_eq!(fresh(&before, &after), 2);
        assert_eq!(fresh(&after, &before), 1);
        assert_eq!(fresh(&[], &after), 3);
        assert_eq!(fresh(&before, &[]), 0);
    }

    fn a_budget_weakens_when_it_moves_the_wrong_way_or_disappears() {
        assert!(lowered(Bound::Ceiling, 3, Some(4)));
        assert!(!lowered(Bound::Ceiling, 3, Some(3)));
        assert!(!lowered(Bound::Ceiling, 3, Some(2)));
        assert!(lowered(Bound::Floor, 3, Some(2)));
        assert!(!lowered(Bound::Floor, 3, Some(4)));
        assert!(lowered(Bound::Floor, 3, None));
        assert!(lowered(Bound::Ceiling, 0, None));
    }

    fn old_gaps_never_refuse_a_commit_or_push_but_ready_requires_closing_one() {
        let old = Change {
            before: 4,
            after: 4,
            touched: 1,
            ..Change::default()
        };
        assert_eq!(verdict(Point::Commit, old), Verdict::Admit);
        assert_eq!(verdict(Point::Push, old), Verdict::Admit);
        assert_eq!(verdict(Point::Start, old), Verdict::Report);
        assert_eq!(verdict(Point::Stop, old), Verdict::Touched);
        assert_eq!(verdict(Point::Ready, old), Verdict::Stalled);
        let progress = Change {
            closed: 1,
            after: 3,
            ..old
        };
        assert_eq!(verdict(Point::Ready, progress), Verdict::Admit);
        let added = Change { added: 1, ..old };
        for point in [Point::Commit, Point::Push, Point::Stop, Point::Ready] {
            assert_eq!(verdict(point, added), Verdict::Added);
        }
        assert_eq!(verdict(Point::Start, Change::default()), Verdict::Admit);
        assert_eq!(
            verdict(
                Point::Ready,
                Change {
                    lowered: 1,
                    ..Change::default()
                }
            ),
            Verdict::Lowered
        );
        assert_eq!(verdict(Point::Ready, Change::default()), Verdict::Admit);
        let revert = Change {
            reverted: true,
            lowered: 1,
            ..added
        };
        for point in [Point::Commit, Point::Push, Point::Ready] {
            assert_eq!(verdict(point, revert), Verdict::Admit);
        }
        assert_eq!(verdict(Point::Stop, revert), Verdict::Added);
        assert!(Verdict::Stalled.refuses());
        assert!(!Verdict::Report.refuses());
    }

    fn reasons_are_the_closed_set_and_a_key_must_end_at_the_colon() {
        assert_eq!(
            exemption(b"Exempt as semantic-boundary: x"),
            Some(Exemption::Exempt(Reason::SemanticBoundary))
        );
        assert_eq!(
            exemption(b"Exempt as no-decisions: x"),
            Some(Exemption::Malformed)
        );
        assert_eq!(exemption(b"Exempt as : x"), Some(Exemption::Malformed));
        assert_eq!(
            exemption(b"Exempt as no-decision:"),
            Some(Exemption::Malformed)
        );
        assert_eq!(REASONS.len(), 3);
    }
}
