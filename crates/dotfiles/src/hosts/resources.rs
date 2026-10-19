//! Capacity for a native campaign's declared extra growth.

use super::Family;
use anyhow::{Context, Result, ensure};
use rules::resource_rules::{Allocation, COMPONENTS, Inodes};

const GIB: u64 = 1024 * 1024 * 1024;
const BUILD_GROWTH: u64 = 8 * GIB;
const PROOF_GROWTH: u64 = 8 * GIB;
const TEMPORARY_GROWTH: u64 = 4 * GIB;
const SAFETY_BYTES: u64 = 4 * GIB;
const BACKGROUND_GROWTH: u64 = 8 * GIB;
/// The retained competing snapshot measured 34,109,222,792 bytes, so this bound exceeds that measurement.
const LINUX_BACKGROUND_GROWTH: u64 = 40 * GIB;
const WORKSPACE_FILES: u64 = 100_000;
const BUILD_FILES: u64 = 200_000;
const PROOF_FILES: u64 = 200_000;
const TEMPORARY_FILES: u64 = 100_000;
const SAFETY_FILES: u64 = 100_000;
const BACKGROUND_FILES: u64 = 1_000_000;
const REQUIRED_INODE_ROWS: usize = 2;

pub(super) fn plan(family: Family) -> [Allocation; COMPONENTS] {
    [
        Allocation {
            bytes: crate::hosts_rules::WORKSPACE_BYTES,
            inodes: WORKSPACE_FILES,
        },
        Allocation {
            bytes: BUILD_GROWTH,
            inodes: BUILD_FILES,
        },
        Allocation {
            bytes: if family == Family::Windows {
                0
            } else {
                PROOF_GROWTH
            },
            inodes: if family == Family::Windows {
                0
            } else {
                PROOF_FILES
            },
        },
        Allocation {
            bytes: TEMPORARY_GROWTH,
            inodes: TEMPORARY_FILES,
        },
        Allocation {
            bytes: SAFETY_BYTES,
            inodes: SAFETY_FILES,
        },
        Allocation {
            bytes: if family == Family::Linux {
                LINUX_BACKGROUND_GROWTH
            } else {
                BACKGROUND_GROWTH
            },
            inodes: BACKGROUND_FILES,
        },
    ]
}

pub(super) fn available_inodes(family: Family, output: &str) -> Result<Inodes> {
    if family == Family::Windows {
        return Ok(Inodes::NotApplicable);
    }
    let mut available: Option<u64> = None;
    let mut column = None;
    let mut rows = 0usize;
    for line in output.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if let Some(index) = words
            .iter()
            .position(|word| matches!(*word, "IFree" | "ifree"))
        {
            column = Some(index);
            continue;
        }
        if !words.first().is_some_and(|word| word.starts_with('/')) {
            continue;
        }
        let value = column
            .and_then(|column| words.get(column))
            .context("inode measurement has no IFree column")?
            .parse::<u64>()
            .context("inode measurement is not an unsigned count")?;
        available = Some(available.map_or(value, |previous| previous.min(value)));
        rows += 1;
    }
    ensure!(
        rows >= REQUIRED_INODE_ROWS,
        "inode measurements must include home and temporary volumes"
    );
    Ok(Inodes::Available(
        available.context("host returned no inode measurement")?,
    ))
}

pub(super) fn require(family: Family, bytes: u64, inodes: Inodes, host: &str) -> Result<()> {
    let plan = plan(family);
    let required =
        rules::resource_rules::required(plan).context("declared resource plan overflows")?;
    rules::resource_rules::admission(plan, Some(bytes), inodes).map_err(|reason| {
        anyhow::anyhow!(
            "{host}: resource admission refused ({reason:?}); available bytes={bytes}, inodes={inodes:?}; declared additional peak requires {} bytes and {} inodes for workspace, build, proofs, temporary files, safety, and bounded competing consumers; retire only eligible ended artifacts or revise the measured plan before submitting new work",
            required.bytes, required.inodes
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_inode_columns_follow_the_actual_header_and_least_free_volume() -> Result<()> {
        let linux = "Filesystem Inodes IUsed IFree IUse% Mounted on\n/dev/home 100 10 90 10% /home\n/dev/tmp 50 30 20 60% /tmp";
        let mac = "Filesystem 512-blocks Used Available Capacity iused ifree %iused Mounted on\n/dev/home 100 10 90 10% 2 80 2% /home\n/dev/tmp 50 30 20 60% 3 40 7% /tmp";
        assert_eq!(
            available_inodes(Family::Linux, linux)?,
            Inodes::Available(20)
        );
        assert_eq!(available_inodes(Family::Mac, mac)?, Inodes::Available(40));
        assert_eq!(
            available_inodes(Family::Windows, "")?,
            Inodes::NotApplicable
        );
        for text in [
            "",
            "Filesystem Inodes IUsed IFree IUse% Mounted on\n/dev/home 100 10 90 10% /home",
            "Filesystem Inodes IUsed IFree IUse% Mounted on\n/dev/home 100 10 unknown 10% /home\n/dev/tmp 50 30 20 60% /tmp",
        ] {
            assert!(available_inodes(Family::Linux, text).is_err(), "{text}");
        }
        Ok(())
    }

    #[test]
    fn campaign_admission_counts_build_temporary_and_competing_growth() -> Result<()> {
        assert!(
            require(
                Family::Linux,
                crate::hosts_rules::WORKSPACE_BYTES,
                Inodes::Available(u64::MAX),
                "linux"
            )
            .is_err()
        );
        let required = rules::resource_rules::required(plan(Family::Linux)).unwrap();
        assert!(
            require(
                Family::Linux,
                required.bytes,
                Inodes::Available(required.inodes - 1),
                "linux"
            )
            .is_err()
        );
        assert!(
            require(
                Family::Linux,
                required.bytes - 1,
                Inodes::Available(required.inodes),
                "linux"
            )
            .is_err()
        );
        require(
            Family::Linux,
            required.bytes,
            Inodes::Available(required.inodes),
            "linux",
        )?;
        Ok(())
    }
}
