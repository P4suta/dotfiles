//! The session-start summary of agent hook runs that failed rather than refused.

extern crate alloc;
use self::alloc::vec::Vec;

/// One hook's failures since the last report: the index of its first and of its most recent record, and how many records name it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Summary {
    pub first: usize,
    pub latest: usize,
    pub count: usize,
}

/// Groups failure records by hook, in the order each hook first failed.
#[must_use]
pub fn summarize<K: Eq>(hooks: &[K]) -> Vec<Summary> {
    let mut summaries: Vec<Summary> = Vec::new();
    for (index, hook) in hooks.iter().enumerate() {
        let known = summaries
            .iter_mut()
            .find(|summary| hooks.get(summary.first) == Some(hook));
        match known {
            Some(summary) => {
                summary.latest = index;
                summary.count += 1;
            }
            None => summaries.push(Summary {
                first: index,
                latest: index,
                count: 1,
            }),
        }
    }
    summaries
}

/// The properties Kani proves for every input and the tests check over every input they list.
#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_summarize(hooks: &[u8]) {
        let summaries = summarize(hooks);
        let mut total = 0;
        for (position, summary) in summaries.iter().enumerate() {
            let hook = hooks.get(summary.first);
            assert!(hook.is_some());
            assert_eq!(hooks.get(summary.latest), hook);
            let mut count = 0;
            for (index, other) in hooks.iter().enumerate() {
                if Some(other) == hook {
                    count += 1;
                    assert!(summary.first <= index && index <= summary.latest);
                }
            }
            assert_eq!(summary.count, count);
            total += count;
            if let Some(next) = summaries.get(position + 1) {
                assert!(summary.first < next.first);
                assert_ne!(hooks.get(next.first), hook);
            }
        }
        assert_eq!(total, hooks.len());
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn each_failed_hook_gets_one_summary_with_its_count_and_latest_record() {
        let hooks: [u8; 3] = kani::any();
        let length: usize = kani::any_where(|length| *length <= hooks.len());
        let hooks = hooks.get(..length).unwrap_or_default();
        spec_summarize(hooks);
        kani::cover!(summarize(hooks).len() == 2 && length == 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_meets_the_specification() {
        let values = [0, 1, 2];
        for first in values {
            for second in values {
                for third in values {
                    for length in 0..=3 {
                        let hooks = [first, second, third];
                        proofs::spec_summarize(hooks.get(..length).unwrap_or_default());
                    }
                }
            }
        }
    }

    #[test]
    fn a_hook_that_failed_twice_reports_both_runs_and_the_later_one() {
        assert_eq!(
            summarize(&["bash", "edit", "bash"]),
            [
                Summary {
                    first: 0,
                    latest: 2,
                    count: 2
                },
                Summary {
                    first: 1,
                    latest: 1,
                    count: 1
                }
            ]
        );
        assert_eq!(summarize::<&str>(&[]), []);
    }
}
