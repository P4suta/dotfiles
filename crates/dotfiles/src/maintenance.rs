//! The scheduled maintenance of every host, one entry per job with the profiles that run it.
//! Launchd on the Mac, systemd user units on Linux and WSL, and Task Scheduler on Windows register the jobs from this list.
//! A job reaches every host from this one place.
//! A profile missing from a job's list states its reason beside the entry.

use crate::profile_rules::Profile;
use std::path::Path;

/// One scheduled job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    /// The job's name, which names its launchd agent `dev.dotfiles.<name>` and its Windows task `dotfiles-<name>`.
    pub name: &'static str,
    /// The systemd user unit that starts the job: a timer, or a service for a watcher.
    pub systemd: &'static str,
    /// The profiles that run the job.
    pub profiles: &'static [Profile],
}

impl Job {
    /// Whether `profile` runs this job.
    #[must_use]
    pub fn runs_on(&self, profile: Profile) -> bool {
        self.profiles.contains(&profile)
    }

    /// The launchd label of the job's agent on the Mac.
    #[must_use]
    pub fn launchd(&self) -> String {
        format!("dev.dotfiles.{}", self.name)
    }

    /// The name of the job's Windows scheduled task.
    #[must_use]
    pub fn task(&self) -> String {
        format!("dotfiles-{}", self.name)
    }
}

const EVERY: &[Profile] = &[Profile::Mac, Profile::Linux, Profile::Wsl, Profile::Windows];
const UNIX: &[Profile] = &[Profile::Mac, Profile::Linux, Profile::Wsl];
const MAC_AND_LINUX: &[Profile] = &[Profile::Mac, Profile::Linux];

/// The jobs, in registration order.
pub const JOBS: [Job; 7] = [
    // Fast-forwards and applies the merged main, converges tools to their reviewed pins, and cleans finished domyjob jobs.
    Job {
        name: "tools-upgrade",
        systemd: "tools-upgrade.timer",
        profiles: EVERY,
    },
    // Windows registers its watcher in the storage-scout setup step, which owns the policy it reads.
    // WSL lacks a storage-scout installation and a policy for the build directories inside its own disk, which the Windows watcher can't see.
    Job {
        name: "storage-scout",
        systemd: "storage-scout.service",
        profiles: &[Profile::Mac, Profile::Linux, Profile::Windows],
    },
    // The reaper reads Unix process groups and job-control states, which Windows lacks.
    // Windows dev containers run inside the WSL engine, whose reaper covers them.
    Job {
        name: "session-reaper",
        systemd: "session-reaper.timer",
        profiles: UNIX,
    },
    Job {
        name: "dev-reaper",
        systemd: "dev-reaper.timer",
        profiles: UNIX,
    },
    // Runs the reaper's bounded container and image prune, which replaced the separate Docker prune timer.
    Job {
        name: "dev-cleanup",
        systemd: "dev-cleanup.timer",
        profiles: UNIX,
    },
    // Refreshes the desktop applications that the Mac and Linux desktops install outside a package manager.
    // Windows updates its applications through Scoop and winget in the upgrade job, and WSL has no desktop of its own.
    Job {
        name: "desktop-refresh",
        systemd: "dotfiles-desktop-refresh.timer",
        profiles: MAC_AND_LINUX,
    },
    // WSL and Windows use Windows OpenSSH and the password manager bridge.
    Job {
        name: "herdr-agent",
        systemd: "herdr-agent.service",
        profiles: MAC_AND_LINUX,
    },
];

/// The systemd user units that earlier versions enabled and this list replaced.
pub const RETIRED_UNITS: [&str; 2] = ["mise-upgrade.timer", "docker-prune.timer"];

/// Hours an unattended upgrade may run before Task Scheduler stops it, as systemd's `TimeoutStartSec=6h` does.
const UPGRADE_LIMIT_HOURS: u8 = 6;
/// Minutes after logon before the upgrade starts, as systemd's `OnStartupSec=5min` does.
const UPGRADE_DELAY_MINUTES: u8 = 5;
/// The weekly start, as the Mac agent's `StartCalendarInterval` sets it.
const UPGRADE_WEEKLY: &str = "Monday";
const UPGRADE_TIME: &str = "11:30";
/// Task Scheduler's below-normal priority, as `Nice=10` asks on Linux.
const BELOW_NORMAL_PRIORITY: u8 = 7;

/// The PowerShell that registers the Windows upgrade task.
/// The task starts five minutes after logon and every Monday at 11:30, catches up a missed start, and logs to `log`.
/// `conhost --headless` keeps a console window from opening.
#[must_use]
pub fn windows_upgrade_task(dotfiles: &Path, source: &Path, config: &Path, log: &Path) -> String {
    let command = format!(
        "\"{}\" --root \"{}\" setup upgrade --config \"{}\" --live >> \"{}\" 2>&1",
        dotfiles.display(),
        source.display(),
        config.display(),
        log.display()
    );
    let argument = format!("--headless cmd.exe /d /c \"{command}\"");
    let task = JOBS[0].task();
    [
        "$ErrorActionPreference = 'Stop'".to_owned(),
        format!(
            "$action = New-ScheduledTaskAction -Execute 'conhost.exe' -Argument {}",
            literal(&argument)
        ),
        format!(
            "$logon = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME; $logon.Delay = 'PT{UPGRADE_DELAY_MINUTES}M'"
        ),
        format!(
            "$weekly = New-ScheduledTaskTrigger -Weekly -DaysOfWeek {UPGRADE_WEEKLY} -At {}",
            literal(UPGRADE_TIME)
        ),
        format!(
            "$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -Priority {BELOW_NORMAL_PRIORITY} -ExecutionTimeLimit (New-TimeSpan -Hours {UPGRADE_LIMIT_HOURS})"
        ),
        format!(
            "Register-ScheduledTask -TaskName {} -Description 'dotfiles upgrade: converge tools to their pins and clean finished domyjob jobs' -Action $action -Trigger @($logon, $weekly) -Settings $settings -Force | Out-Null",
            literal(&task)
        ),
    ]
    .join("\n")
}

/// A PowerShell single-quoted literal, as `host_setup_rules::powershell_literal` writes it.
fn literal(text: &str) -> String {
    crate::host_setup_rules::powershell_literal(text)
        .as_str()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn data(name: &str) -> toml::Table {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../home/.chezmoidata")
            .join(name);
        toml::from_str(&std::fs::read_to_string(path).expect("chezmoi data")).expect("TOML")
    }

    /// The profiles that receive `target`, by the sets of targets.toml and profiles.toml.
    fn receivers(target: &str) -> BTreeSet<String> {
        let sets: BTreeMap<String, Vec<String>> = data("profiles.toml")["profiles"]
            .as_table()
            .expect("profile sets")
            .iter()
            .map(|(set, members)| {
                let members = members
                    .as_array()
                    .expect("set members")
                    .iter()
                    .map(|member| member.as_str().expect("profile").to_owned())
                    .collect();
                (set.clone(), members)
            })
            .collect();
        let targets = data("targets.toml");
        targets["targets"]
            .get(target)
            .and_then(toml::Value::as_str)
            .map_or_else(
                || Profile::ALL.iter().map(|p| p.name().to_owned()).collect(),
                |set| sets[set].iter().cloned().collect(),
            )
    }

    fn names(profiles: impl IntoIterator<Item = Profile>) -> BTreeSet<String> {
        profiles.into_iter().map(|p| p.name().to_owned()).collect()
    }

    #[test]
    fn every_job_has_its_definition_on_exactly_the_hosts_the_list_names() {
        for job in JOBS {
            let mac = names(job.runs_on(Profile::Mac).then_some(Profile::Mac));
            let plist = format!("Library/LaunchAgents/{}.plist", job.launchd());
            assert_eq!(receivers(&plist), mac, "{plist}");
            let systemd = names(
                [Profile::Linux, Profile::Wsl]
                    .into_iter()
                    .filter(|p| job.runs_on(*p)),
            );
            let unit = format!(".config/systemd/user/{}", job.systemd);
            assert_eq!(receivers(&unit), systemd, "{unit}");
            if let Some(timer) = job.systemd.strip_suffix(".timer") {
                let service = format!(".config/systemd/user/{timer}.service");
                assert_eq!(receivers(&service), systemd, "{service}");
            }
        }
    }

    #[test]
    fn the_upgrade_runs_on_every_host() {
        for profile in Profile::ALL {
            assert!(JOBS[0].runs_on(profile), "{profile:?}");
        }
        assert_eq!(JOBS[0].name, "tools-upgrade");
    }

    #[test]
    fn retired_units_reach_no_host() {
        let retired = data("retired.toml");
        let listed = retired["retired"].as_table().expect("retired targets");
        for unit in RETIRED_UNITS {
            let target = format!(".config/systemd/user/{unit}");
            assert!(listed.contains_key(&target), "{target}");
            assert!(
                !data("targets.toml")["targets"]
                    .as_table()
                    .expect("targets")
                    .contains_key(&target),
                "{target}"
            );
        }
    }

    #[test]
    fn the_windows_task_runs_the_upgrade_after_logon_and_weekly() {
        let script = windows_upgrade_task(
            Path::new("C:/Users/o/.local/bin/dotfiles.exe"),
            Path::new("C:/Users/o/.local/share/chezmoi"),
            Path::new("C:/Users/o/.config/chezmoi/chezmoi.toml"),
            Path::new("C:/Users/o/.local/state/dotfiles/log/tools-upgrade.log"),
        );
        assert!(script.contains("setup upgrade --config"));
        assert!(script.contains("--live >>"));
        assert!(script.contains("-AtLogOn"));
        assert!(script.contains("$logon.Delay = 'PT5M'"));
        assert!(script.contains("-Weekly -DaysOfWeek Monday -At '11:30'"));
        assert!(script.contains("-StartWhenAvailable"));
        assert!(script.contains("-TaskName 'dotfiles-tools-upgrade'"));
        assert!(script.contains("-Trigger @($logon, $weekly)"));
    }
}
