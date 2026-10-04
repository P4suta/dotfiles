fn main() {
    let user_directory = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let result = user_directory
        .ok_or_else(|| anyhow::anyhow!("user directory is unavailable"))
        .and_then(|directory| {
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
            eprintln!("CodeRabbit guard: {error:#}");
            std::process::exit(75);
        }
    }
}
