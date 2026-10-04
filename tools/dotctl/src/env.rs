use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

pub fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PATH", augmented_path());
    cmd.env("MISE_AUTO_INSTALL", "false");
    cmd
}

pub fn augmented_path() -> OsString {
    let mut dirs: Vec<PathBuf> = vec![scoop_root().join("shims")];
    if let Some(local) = local_app_data() {
        dirs.push(local.join("mise").join("shims"));
        dirs.push(local.join("Microsoft").join("WinGet").join("Links"));
    }
    if let Some(home) = home_dir() {
        dirs.push(home.join(".cargo").join("bin"));
        dirs.push(home.join(".local").join("bin"));
    }

    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut joined = std::env::join_paths(dirs).unwrap_or_default();
    if !existing.is_empty() {
        joined.push(if cfg!(windows) { ";" } else { ":" });
        joined.push(&existing);
    }
    joined
}

pub fn scoop_root() -> PathBuf {
    std::env::var_os("SCOOP")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join("scoop")))
        .unwrap_or_else(|| PathBuf::from("scoop"))
}

pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local").join("share")))
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
