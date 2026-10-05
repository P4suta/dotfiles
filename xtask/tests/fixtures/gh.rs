use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::env::var("GH_HOST")? != "github.com" {
        return Err("incorrect GitHub host".into());
    }
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var_os("GH_FIXTURE_LOG").ok_or("log missing")?)?;
    writeln!(log, "invocation\n{}", args.join("\n"))?;
    if let Some(path) = std::env::var_os("GH_FIXTURE_REWRITE_FILE") {
        std::fs::write(path, "changed after validation")?;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--body-file") {
        writeln!(log, "body:\n{}", std::fs::read_to_string(&args[index + 1])?)?;
    }
    if std::env::var("GH_FIXTURE_FAILURE").ok().as_ref() == args.get(1) {
        eprintln!("injected gh failure");
        std::process::exit(42);
    }
    if args.first().is_some_and(|arg| arg == "api") {
        let variable = if args.get(1).is_some_and(|arg| arg == "user") {
            "GH_FIXTURE_USER"
        } else if args.get(1).is_some_and(|arg| arg.contains("/issues/")) {
            "GH_FIXTURE_ISSUE"
        } else {
            "GH_FIXTURE_REPOSITORY"
        };
        if args.iter().any(|arg| arg == "--include") {
            let headers = std::env::var(format!("{variable}_HEADERS"))
                .or_else(|_| std::env::var("GH_FIXTURE_HEADERS"))
                .unwrap_or_else(|_| "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 4990\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core".into());
            println!("{headers}\n");
        }
        println!("{}", std::env::var(variable)?);
    } else if args.get(1).is_some_and(|arg| arg == "view") {
        let view = std::env::var("GH_FIXTURE_VIEW")?;
        let checks = std::env::var("GH_FIXTURE_CHECKS")
            .unwrap_or_else(|_| r#"[{"status":"COMPLETED","conclusion":"SUCCESS"}]"#.into());
        match view.strip_suffix('}') {
            Some(open) if !view.contains("statusCheckRollup") && !open.trim().ends_with('{') => {
                println!("{open},\"statusCheckRollup\":{checks}}}");
            }
            _ => println!("{view}"),
        }
    } else {
        println!("https://github.com/owner/project/pull/17");
    }
    Ok(())
}
