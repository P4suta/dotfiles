#![cfg(target_os = "macos")]
#![allow(
    clippy::disallowed_methods,
    reason = "integration tests spawn the binaries and real tools they verify"
)]

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

/// Boots the probe out even when an assertion fails, so a failed run leaves no agent behind.
struct Loaded(String);

impl Drop for Loaded {
    fn drop(&mut self) {
        let _ = Command::new("launchctl")
            .args(["bootout", &self.0])
            .status();
    }
}

fn describe(target: &str) -> String {
    let printed = Command::new("launchctl")
        .args(["print", target])
        .output()
        .unwrap();
    String::from_utf8_lossy(&printed.stdout).into_owned()
}

/// The probe keeps running and takes seconds to exit after SIGTERM, as a busy agent such as storage-scout does.
fn write_probe(plist: &Path, label: &str, generation: &str) {
    std::fs::write(
        plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{label}</string>
<key>ProgramArguments</key><array><string>/bin/sh</string><string>-c</string><string>trap 'sleep 2; exit 0' TERM; while :; do sleep 1; done</string><string>{generation}</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><true/>
</dict></plist>
"#
        ),
    )
    .unwrap();
}

/// Runs against the real launchd, because `bootout` returning before a running job exits is launchd's behavior rather than this repository's.
#[test]
fn a_running_agent_is_replaced_by_its_new_definition() {
    let uid = Command::new("id").arg("-u").output().unwrap().stdout;
    let domain = format!("gui/{}", String::from_utf8(uid).unwrap().trim());
    let label = format!("dev.dotfiles.contract-{}", std::process::id());
    let target = format!("{domain}/{label}");
    let scope = tempfile::tempdir().unwrap();
    let plist = scope.path().join(format!("{label}.plist"));
    let home = std::env::var_os("HOME").unwrap();
    let mut runner = dotfiles_xtask::runtime::Native::new(Path::new(&home)).unwrap();

    write_probe(&plist, &label, "first-generation");
    let _loaded = Loaded(target.clone());
    dotfiles_xtask::setup::reload_agent(&mut runner, &domain, &label, &plist).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !describe(&target).contains("state = running") {
        assert!(Instant::now() < deadline, "the probe agent never started");
        std::thread::sleep(Duration::from_millis(100));
    }

    write_probe(&plist, &label, "second-generation");
    dotfiles_xtask::setup::reload_agent(&mut runner, &domain, &label, &plist).unwrap();
    let loaded = describe(&target);
    assert!(loaded.contains("second-generation"), "{loaded}");
}
