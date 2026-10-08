use anyhow::{Result, ensure};
use dotfiles::hook_rules::{
    Configured, Gate, HOOKS, Hook, MOST_GATES, notification, plan, takes_input,
};

const CONFIGURATIONS: usize = 1 << 6;
const INPUT: &[u8] = b"refs/heads/fixture 0123 refs/heads/fixture abcd\n";

fn configured(mask: usize) -> Configured {
    Configured {
        global: mask & 1 != 0,
        repository: mask & 2 != 0,
        url: mask & 4 != 0,
        committed: mask & 8 != 0,
        storage: mask & 16 != 0,
        working_copy: mask & 32 != 0,
    }
}

fn touches_index(gate: Gate) -> bool {
    matches!(
        gate,
        Gate::Dotguard | Gate::Verification | Gate::GlobalLefthook | Gate::RepositoryLefthook
    )
}

fn ordered(hook: Hook, gates: &[Gate]) -> Result<()> {
    let mut read = false;
    let mut captured = false;
    let mut checked = false;
    let mut notified = false;
    for (index, gate) in gates.iter().enumerate() {
        ensure!(
            (*gate == Gate::PushHold) == (hook == Hook::PrePush && index == 0),
            "the push hold precedes every other gate"
        );
        if takes_input(*gate) && matches!(hook, Hook::PrePush | Hook::ReferenceTransaction) {
            ensure!(read, "input is available before a gate receives it");
        }
        if hook == Hook::PreCommit && touches_index(*gate) {
            ensure!(
                captured && !checked,
                "the index brackets every mutating gate"
            );
        }
        if notification(*gate) {
            notified = true;
        } else {
            ensure!(!notified, "notifications follow verdict gates");
        }
        match gate {
            Gate::ReadInput => {
                ensure!(!read, "input is read once");
                read = true;
            }
            Gate::StagedTree => {
                ensure!(!captured, "the tree is captured once");
                captured = true;
            }
            Gate::StagedTreeCheck => {
                checked = true;
                ensure!(index + 1 == gates.len(), "the index check is last");
            }
            _ => {}
        }
    }
    ensure!(
        read == matches!(hook, Hook::PrePush | Hook::ReferenceTransaction),
        "only hooks with stdin read it"
    );
    ensure!(
        captured == (hook == Hook::PreCommit) && checked == captured,
        "commits preserve the index"
    );
    Ok(())
}

fn execute(hook: Hook, gates: &[Gate], failed: u8, edits: u8) {
    let mut input = &[][..];
    let mut seen = Vec::new();
    let mut changed = false;
    let mut refused = false;
    let mut ran = Vec::new();
    for (index, gate) in gates.iter().enumerate() {
        if refused {
            break;
        }
        ran.push(*gate);
        if *gate == Gate::ReadInput {
            input = INPUT;
        }
        if takes_input(*gate) {
            seen.push(input);
        }
        changed |= edits & (1 << index) != 0;
        refused = (failed & (1 << index) != 0 && !notification(*gate))
            || (*gate == Gate::StagedTreeCheck && changed);
    }
    let first_failure = gates.iter().enumerate().position(|(index, gate)| {
        (failed & (1 << index) != 0 && !notification(*gate))
            || (*gate == Gate::StagedTreeCheck && edits != 0)
    });
    assert_eq!(refused, first_failure.is_some());
    assert_eq!(
        ran,
        gates[..first_failure.map_or(gates.len(), |index| index + 1)]
    );
    let expected = if matches!(hook, Hook::PrePush | Hook::ReferenceTransaction) {
        INPUT
    } else {
        &[]
    };
    assert!(seen.iter().all(|bytes| *bytes == expected));
    if !refused && hook == Hook::PreCommit {
        assert!(!changed);
    }
    let notification_bits = gates.iter().enumerate().fold(0, |mask, (index, gate)| {
        if notification(*gate) {
            mask | (1 << index)
        } else {
            mask
        }
    });
    assert_eq!(
        first_failure,
        gates
            .iter()
            .enumerate()
            .position(
                |(index, gate)| ((failed & !notification_bits) & (1 << index) != 0
                    && !notification(*gate))
                    || (*gate == Gate::StagedTreeCheck && edits != 0)
            )
    );
}

#[test]
fn every_hook_and_configuration_preserves_the_plan_for_every_step_outcome() -> Result<()> {
    for hook in HOOKS {
        for mask in 0..CONFIGURATIONS {
            let planned = plan(hook, configured(mask));
            let gates = planned.gates();
            ordered(hook, gates)?;
            let changeable = gates.iter().enumerate().fold(0u8, |bits, (index, gate)| {
                if hook == Hook::PreCommit && touches_index(*gate) {
                    bits | (1 << index)
                } else {
                    bits
                }
            });
            for edits in 0..=u8::MAX {
                if edits & !changeable != 0 {
                    continue;
                }
                for failed in 0..(1u16 << gates.len()) {
                    execute(hook, gates, u8::try_from(failed)?, edits);
                }
            }
        }
    }
    let planned = plan(Hook::PrePush, configured(CONFIGURATIONS - 1));
    let mut reordered = planned.gates().to_vec();
    assert_eq!(reordered.len(), MOST_GATES);
    reordered.swap(0, 1);
    assert!(ordered(Hook::PrePush, &reordered).is_err());
    Ok(())
}
