//! Which `cfg` predicates can compile an item into production code or into a Kani proof.
//!
//! An item's `cfg` attributes arrive as one postfix sequence of steps, and the sequence holds when every predicate it leaves holds, as the [Rust Reference](https://doc.rust-lang.org/reference/conditional-compilation.html) defines.
//! The gates know the values of `test` and `kani` only, so every other option, such as `unix` or `feature = "tools"`, stays unknown.

/// The most predicate values a sequence may hold at once before it counts as unreadable.
const DEPTH: u32 = 16;

/// One step of an item's `cfg` predicates in postfix order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Step {
    /// The `test` option.
    Test,
    /// The `kani` option.
    Kani,
    /// A `true` or `false` literal predicate.
    Literal(bool),
    /// Another option, or syntax the reader can't parse, whose value stays unknown.
    Unknown,
    /// `not` of the previous predicate.
    Not,
    /// `all` of the previous `n` predicates.
    All(u8),
    /// `any` of the previous `n` predicates.
    Any(u8),
}

/// The value of a predicate when some options stay unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Truth {
    True,
    False,
    Unknown,
}

/// The values of the options the gates know.
#[derive(Clone, Copy)]
struct Context {
    test: bool,
    kani: bool,
}

/// An ordinary build, which compiles production code.
const PRODUCTION: Context = Context {
    test: false,
    kani: false,
};

/// A Kani build, which compiles the proofs.
const PROOF: Context = Context {
    test: false,
    kani: true,
};

const fn known(value: bool) -> Truth {
    if value { Truth::True } else { Truth::False }
}

const fn negate(value: Truth) -> Truth {
    match value {
        Truth::True => Truth::False,
        Truth::False => Truth::True,
        Truth::Unknown => Truth::Unknown,
    }
}

/// The bits from `start` up to `end`, which mark those positions of a stack.
const fn span(start: u32, end: u32) -> u32 {
    ((1 << end) - 1) & !((1 << start) - 1)
}

/// A stack of predicate values, one bit per position: `known` marks a known value, and `holds` marks a true one.
#[derive(Clone, Copy)]
struct Stack {
    known: u32,
    holds: u32,
}

impl Stack {
    /// `all` of the values at the positions in `mask`, which is true for no values.
    const fn all(self, mask: u32) -> Truth {
        if self.known & !self.holds & mask != 0 {
            Truth::False
        } else if !self.known & mask != 0 {
            Truth::Unknown
        } else {
            Truth::True
        }
    }

    /// `any` of the values at the positions in `mask`, which is false for no values.
    const fn any(self, mask: u32) -> Truth {
        if self.known & self.holds & mask != 0 {
            Truth::True
        } else if !self.known & mask != 0 {
            Truth::Unknown
        } else {
            Truth::False
        }
    }

    /// The stack with `value` at `position`.
    const fn with(self, position: u32, value: Truth) -> Self {
        let bit = 1 << position;
        Self {
            known: if matches!(value, Truth::Unknown) {
                self.known & !bit
            } else {
                self.known | bit
            },
            holds: if matches!(value, Truth::True) {
                self.holds | bit
            } else {
                self.holds & !bit
            },
        }
    }
}

/// The value of `steps` in `context`, or unknown for a malformed or overlong sequence.
fn evaluate(steps: &[Step], context: Context) -> Truth {
    let mut stack = Stack { known: 0, holds: 0 };
    let mut depth: u32 = 0;
    for &step in steps {
        let value = match step {
            Step::Test => known(context.test),
            Step::Kani => known(context.kani),
            Step::Literal(value) => known(value),
            Step::Unknown => Truth::Unknown,
            Step::Not => {
                let Some(top) = depth.checked_sub(1) else {
                    return Truth::Unknown;
                };
                depth = top;
                negate(stack.all(1 << top))
            }
            Step::All(count) | Step::Any(count) => {
                let Some(start) = depth.checked_sub(u32::from(count)) else {
                    return Truth::Unknown;
                };
                let operands = span(start, depth);
                depth = start;
                if matches!(step, Step::All(_)) {
                    stack.all(operands)
                } else {
                    stack.any(operands)
                }
            }
        };
        if depth >= DEPTH {
            return Truth::Unknown;
        }
        stack = stack.with(depth, value);
        depth += 1;
    }
    stack.all(span(0, depth))
}

/// Whether the gates scan an item with these `cfg` steps as production code.
/// Only a predicate false in every ordinary build excludes the item, so an unknown option or unreadable syntax keeps it in the scan.
#[must_use]
pub fn in_production(steps: &[Step]) -> bool {
    evaluate(steps, PRODUCTION) != Truth::False
}

/// Whether an item with these `cfg` steps compiles in a Kani build, so its harnesses and mentions count as proof evidence.
/// Only a predicate true in every Kani build qualifies, so an unknown option or unreadable syntax grants no evidence.
#[must_use]
pub fn in_proof(steps: &[Step]) -> bool {
    evaluate(steps, PROOF) == Truth::True
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    /// The longest sequence the proofs walk.
    pub const LONGEST: usize = 5;

    /// The value of `steps` when each unknown step at index `i` takes `options[i]`, or `None` for a malformed or overlong sequence.
    /// Bit `i` of `values` holds the `i`th value on the stack.
    pub fn concrete(steps: &[Step], context: Context, options: [bool; LONGEST]) -> Option<bool> {
        let mut values: u32 = 0;
        let mut depth: u32 = 0;
        for (index, &step) in steps.iter().enumerate() {
            let value = match step {
                Step::Test => context.test,
                Step::Kani => context.kani,
                Step::Literal(value) => value,
                Step::Unknown => *options.get(index)?,
                Step::Not => {
                    depth = depth.checked_sub(1)?;
                    (values >> depth) & 1 == 0
                }
                Step::All(count) | Step::Any(count) => {
                    let start = depth.checked_sub(u32::from(count))?;
                    let operands = ((1u32 << depth) - 1) ^ ((1u32 << start) - 1);
                    depth = start;
                    if matches!(step, Step::All(_)) {
                        values & operands == operands
                    } else {
                        values & operands != 0
                    }
                }
            };
            if depth >= DEPTH {
                return None;
            }
            values = (values & !(1 << depth)) | (u32::from(value) << depth);
            depth += 1;
        }
        let left = (1u32 << depth) - 1;
        Some(values & left == left)
    }

    /// A known value holds for every value of the unknown options, and a readable sequence without unknown options has a known value.
    pub fn spec_sound(steps: &[Step], context: Context, options: [bool; LONGEST]) -> Truth {
        let concrete = concrete(steps, context, options);
        let value = evaluate(steps, context);
        match value {
            Truth::True => assert_eq!(concrete, Some(true)),
            Truth::False => assert_eq!(concrete, Some(false)),
            Truth::Unknown => {
                assert!(concrete.is_none() || steps.contains(&Step::Unknown));
            }
        }
        value
    }

    /// An excluded item can't compile in an ordinary build, and an item that grants evidence compiles in every Kani build.
    pub fn spec_decisions(steps: &[Step], options: [bool; LONGEST]) -> (bool, bool) {
        let production = in_production(steps);
        let proof = in_proof(steps);
        if !production {
            assert_eq!(concrete(steps, PRODUCTION, options), Some(false));
        }
        if proof {
            assert_eq!(concrete(steps, PROOF, options), Some(true));
        }
        (production, proof)
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(7)]
    fn a_known_value_holds_for_every_unknown_option() {
        let steps: [Step; LONGEST] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= LONGEST);
        let options: [bool; LONGEST] = kani::any();
        let context = Context {
            test: kani::any(),
            kani: kani::any(),
        };
        let value = spec_sound(&steps[..length], context, options);
        kani::cover!(value == Truth::Unknown);
        kani::cover!(value == Truth::False && length == LONGEST);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(7)]
    fn only_an_impossible_predicate_leaves_production_and_only_a_certain_one_proves() {
        let steps: [Step; LONGEST] = kani::any();
        let length: usize = kani::any();
        kani::assume(length <= LONGEST);
        let options: [bool; LONGEST] = kani::any();
        let (production, proof) = spec_decisions(&steps[..length], options);
        kani::cover!(!production);
        kani::cover!(proof);
        kani::cover!(production && !proof);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const ALPHABET: [Step; 9] = [
        Step::Test,
        Step::Kani,
        Step::Literal(true),
        Step::Literal(false),
        Step::Unknown,
        Step::Not,
        Step::All(0),
        Step::All(2),
        Step::Any(2),
    ];

    /// Every sequence over `ALPHABET` up to three steps, and one of five.
    fn sequences() -> Vec<Vec<Step>> {
        let mut found = alloc::vec![Vec::new()];
        let mut start = 0;
        for _ in 0..3 {
            let end = found.len();
            for index in start..end {
                for step in ALPHABET {
                    let mut sequence = found.get(index).cloned().unwrap_or_default();
                    sequence.push(step);
                    found.push(sequence);
                }
            }
            start = end;
        }
        found.push(alloc::vec![
            Step::Test,
            Step::Kani,
            Step::Unknown,
            Step::Any(3),
            Step::Not
        ]);
        found
    }

    #[test]
    fn every_listed_sequence_meets_the_specification() {
        for steps in sequences() {
            for options in crate::domain::flags::<{ proofs::LONGEST }>() {
                for context in [
                    PRODUCTION,
                    PROOF,
                    Context {
                        test: true,
                        kani: false,
                    },
                ] {
                    proofs::spec_sound(&steps, context, options);
                }
                proofs::spec_decisions(&steps, options);
            }
        }
    }

    #[test]
    fn the_issue_forms_classify_as_the_compiler_builds_them() {
        // `cfg(any(windows, test))` compiles on Windows, so the gate keeps scanning it.
        let windows_or_test = [Step::Unknown, Step::Test, Step::Any(2)];
        assert!(in_production(&windows_or_test));
        assert!(!in_proof(&windows_or_test));
        // `cfg(not(test))` compiles in every ordinary build.
        assert!(in_production(&[Step::Test, Step::Not]));
        // `cfg(feature = "test-utils")` names a feature, not the test option.
        assert!(in_production(&[Step::Unknown]));
        // `cfg(all(test, not(kani)))` compiles in neither an ordinary nor a Kani build.
        let test_without_kani = [Step::Test, Step::Kani, Step::Not, Step::All(2)];
        assert!(!in_production(&test_without_kani));
        assert!(!in_proof(&test_without_kani));
        // `cfg(any(test, kani))` and `cfg(kani)` hold only verification code.
        for steps in [&[Step::Test, Step::Kani, Step::Any(2)][..], &[Step::Kani]] {
            assert!(!in_production(steps));
            assert!(in_proof(steps));
        }
        // `cfg(test)` compiles for tests only.
        assert!(!in_production(&[Step::Test]));
        assert!(!in_proof(&[Step::Test]));
        // Two attributes must both hold, so `cfg(unix)` with `cfg(test)` stays out of production.
        assert!(!in_production(&[Step::Unknown, Step::Test]));
        // Empty lists and literals follow the reference.
        assert!(in_production(&[]) && in_proof(&[]));
        assert!(!in_production(&[Step::Any(0)]));
        assert!(in_proof(&[Step::All(0)]));
        assert!(!in_production(&[Step::Literal(false)]));
        // An unreadable sequence stays in production and grants no evidence.
        for steps in [&[Step::Not][..], &[Step::Kani, Step::All(2)]] {
            assert!(in_production(steps));
            assert!(!in_proof(steps));
        }
    }
}
