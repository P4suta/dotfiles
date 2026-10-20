/// `EX_TEMPFAIL` from `sysexits.h`: the guard couldn't decide, so a caller may retry later.
const TEMPORARY_FAILURE: i32 = 75;

fn main() {
    let result = coderabbit_guard::freshness::print_build_identity().and_then(|printed| {
        if printed {
            return Ok(0);
        }
        platform::home::home()
            .ok_or_else(|| anyhow::anyhow!("user directory is unavailable"))
            .and_then(|directory| {
                if let coderabbit_guard::freshness::Start::Delegated(code) =
                    coderabbit_guard::freshness::start(
                        coderabbit_guard::freshness::Built::Coderabbit,
                    )?
                {
                    return Ok(code);
                }
                let args = std::env::args_os()
                    .skip(1)
                    .map(|arg| {
                        arg.into_string()
                            .map_err(|_| anyhow::anyhow!("non-UTF-8 CodeRabbit argument"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                coderabbit_guard::run(args, &directory)
            })
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
            std::process::exit(TEMPORARY_FAILURE);
        }
    }
}
