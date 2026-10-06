fn main() {
    let result = platform::home::home()
        .ok_or_else(|| anyhow::anyhow!("user directory is unavailable"))
        .and_then(|directory| {
            coderabbit_guard::freshness::require_current(
                coderabbit_guard::freshness::Built::Coderabbit,
            )?;
            let args = std::env::args_os()
                .skip(1)
                .map(|arg| {
                    arg.into_string()
                        .map_err(|_| anyhow::anyhow!("non-UTF-8 CodeRabbit argument"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            coderabbit_guard::run(args, &directory)
        });
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if error
                .downcast_ref::<coderabbit_guard::refusal::Refusal>()
                .is_some()
            {
                coderabbit_guard::report(&error);
            } else {
                eprintln!("CodeRabbit guard: {error:#}");
            }
            std::process::exit(75);
        }
    }
}
