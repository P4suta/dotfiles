//! `dotctl`, the Windows machine setup that the Windows launchers run.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    dotfiles::windows::cli::run()
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("dotctl: the Windows machine setup runs only on Windows");
    std::process::ExitCode::FAILURE
}
