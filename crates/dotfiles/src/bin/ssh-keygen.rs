//! Git's Unix SSH signing entry point.

fn main() {
    #[cfg(unix)]
    let result = dotfiles::signing::exec(&std::env::args_os().skip(1).collect::<Vec<_>>());
    #[cfg(not(unix))]
    let result: anyhow::Result<()> = Err(anyhow::anyhow!(
        "SSH signing uses Windows OpenSSH on Windows"
    ));
    if let Err(error) = result {
        dotfiles::report(&error);
        std::process::exit(1);
    }
}
