use anyhow::{Context, Result, ensure};
use dotfiles::hosts::remote_gate;
use dotfiles::hosts_rules::Family;
use std::path::{Path, PathBuf};

#[test]
fn remote_bootstrap_captures_cli_configuration_before_relocating_home() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let host = scope.path().join("host");
    for family in Family::ALL {
        if (family == Family::Windows) != cfg!(windows) {
            continue;
        }
        let script = remote_gate(family, "just check");
        let boundary = if cfg!(windows) {
            "$nativeHome ="
        } else {
            "nativeHome="
        };
        let capture = script
            .split_once(boundary)
            .context("native home capture")?
            .0;
        for (gh, xdg, appdata, expected) in [
            (
                Some("custom-gh"),
                Some("custom-xdg"),
                Some("custom-appdata"),
                "custom-gh",
            ),
            (
                None,
                Some("custom-xdg"),
                Some("custom-appdata"),
                "custom-xdg/gh",
            ),
            (
                None,
                None,
                Some("custom-appdata"),
                if cfg!(windows) {
                    "custom-appdata/GitHub CLI"
                } else {
                    "host/.config/gh"
                },
            ),
            (None, None, None, "host/.config/gh"),
        ] {
            let mut command =
                platform::process::command(if cfg!(windows) { "pwsh" } else { "/bin/sh" });
            if cfg!(windows) {
                command
                    .args(["-NoLogo", "-NoProfile", "-Command"])
                    .arg(format!("{capture}Write-Output $env:GH_CONFIG_DIR"));
            } else {
                command.args(["-c", &format!("{capture}printf '%s' \"$GH_CONFIG_DIR\"")]);
            }
            command.env("HOME", &host).env("USERPROFILE", &host);
            for (name, value) in [
                ("GH_CONFIG_DIR", gh),
                ("XDG_CONFIG_HOME", xdg),
                ("APPDATA", appdata),
            ] {
                if let Some(value) = value {
                    command.env(name, scope.path().join(value));
                } else {
                    command.env_remove(name);
                }
            }
            let output = command.output()?;
            ensure!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                PathBuf::from(std::str::from_utf8(&output.stdout)?.trim()),
                scope.path().join(Path::new(expected)),
                "{family:?} {gh:?} {xdg:?} {appdata:?}"
            );
        }
    }
    Ok(())
}
