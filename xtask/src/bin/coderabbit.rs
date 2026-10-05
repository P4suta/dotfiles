fn main() {
    let user_directory = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let result = user_directory
        .ok_or_else(|| anyhow::anyhow!("user directory is unavailable"))
        .and_then(|directory| {
            dotfiles_xtask::freshness::require_current(
                dotfiles_xtask::freshness::Built::Coderabbit,
            )?;
            let args = std::env::args_os()
                .skip(1)
                .map(|arg| {
                    arg.into_string()
                        .map_err(|_| anyhow::anyhow!("non-UTF-8 CodeRabbit argument"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            dotfiles_xtask::review_guard::run(args, std::path::Path::new(&directory))
        });
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if error
                .downcast_ref::<dotfiles_xtask::refusal::Refusal>()
                .is_some()
            {
                dotfiles_xtask::report(&error);
            } else {
                eprintln!("CodeRabbit guard: {error:#}");
            }
            std::process::exit(75);
        }
    }
}
