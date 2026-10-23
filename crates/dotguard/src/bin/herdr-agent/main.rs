//! The supervised Unix SSH agent and per-command routing selector.

#[cfg(unix)]
mod unix;

fn main() -> std::process::ExitCode {
    #[cfg(unix)]
    {
        unix::main()
    }
    #[cfg(not(unix))]
    {
        eprintln!("herdr-agent requires Unix sockets; Windows uses Windows OpenSSH");
        std::process::ExitCode::FAILURE
    }
}
