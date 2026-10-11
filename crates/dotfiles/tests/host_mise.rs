use anyhow::{Context, Result, ensure};
use dotfiles::hosts::remote_gate;
use dotfiles::hosts_rules::Family;
use std::path::PathBuf;

#[test]
fn remote_bootstrap_retains_native_mise_locations_before_relocating_home() -> Result<()> {
    let scope = tempfile::tempdir()?;
    let names = [
        "MISE_DATA_DIR",
        "MISE_STATE_DIR",
        "MISE_CACHE_DIR",
        "MISE_CONFIG_DIR",
        "MISE_GLOBAL_CONFIG_FILE",
        "MISE_SYSTEM_CONFIG_FILE",
        "MISE_INSTALLS_DIR",
        "MISE_DOWNLOADS_DIR",
        "MISE_PLUGINS_DIR",
        "MISE_SHIMS_DIR",
        "RUSTUP_HOME",
    ];
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
        let capture = script.split_once(boundary).context("native capture")?.0;
        let values = names.map(|name| scope.path().join(name));
        let expressions = names.map(|name| {
            if cfg!(windows) {
                format!("$env:DOTFILES_HOST_{name}")
            } else {
                format!("\"$DOTFILES_HOST_{name}\"")
            }
        });
        let mut command =
            platform::process::command(if cfg!(windows) { "pwsh" } else { "/bin/sh" });
        if cfg!(windows) {
            command
                .args(["-NoLogo", "-NoProfile", "-Command"])
                .arg(format!(
                    "{capture}Write-Output ({})",
                    expressions.join(", ")
                ));
        } else {
            command.args([
                "-c",
                &format!("{capture}printf '%s\\n' {}", expressions.join(" ")),
            ]);
        }
        for (name, value) in names.into_iter().zip(&values) {
            command.env(name, value);
        }
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let captured = std::str::from_utf8(&output.stdout)?
            .lines()
            .map(|line| PathBuf::from(line.trim()))
            .collect::<Vec<_>>();
        assert_eq!(captured, values, "{family:?}");
    }
    Ok(())
}
