use anyhow::{Result, bail};

use crate::{env, proc};

pub fn run(apps: &[String]) -> Result<i32> {
    for id in apps {
        if installed(id) {
            continue;
        }
        println!(">>> winget install {id}");
        let mut cmd = winget("install");
        cmd.args(["--id", id, "--exact", "--silent"])
            .args(["--accept-source-agreements", "--accept-package-agreements"]);
        let code = proc::status(&mut cmd)?.code();
        if code != 0 {
            bail!("winget install {id} exited with {code}");
        }
    }
    Ok(0)
}

pub fn installed(id: &str) -> bool {
    proc::capture(
        winget("list")
            .args(["--id", id, "--exact"])
            .arg("--accept-source-agreements"),
    )
    .is_ok_and(|out| out.ok())
}

pub fn uninstall(id: &str) -> Result<()> {
    let mut cmd = winget("uninstall");
    cmd.args(["--id", id, "--exact", "--silent"]);
    let code = proc::status(&mut cmd)?.code();
    if code != 0 {
        bail!("winget uninstall {id} exited with {code}");
    }
    Ok(())
}

fn winget(subcommand: &str) -> std::process::Command {
    let mut cmd = env::command("winget");
    cmd.args([subcommand, "--disable-interactivity"]);
    cmd
}
