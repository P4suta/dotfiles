use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::env::var("GH_HOST")? != "github.com" {
        return Err("incorrect GitHub host".into());
    }
    let log_path = std::env::var_os("GH_FIXTURE_LOG").ok_or("log missing")?;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
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
        let endpoint = args.get(1).cloned().unwrap_or_default();
        let headers = |variable: &str| {
            std::env::var(format!("{variable}_HEADERS"))
                .or_else(|_| std::env::var("GH_FIXTURE_HEADERS"))
                .unwrap_or_else(|_| "HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 4990\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: core".into())
        };
        let variable = if endpoint == "user" {
            "GH_FIXTURE_USER"
        } else if endpoint.contains("/issues/") {
            "GH_FIXTURE_ISSUE"
        } else if endpoint.contains("/compare/") {
            "GH_FIXTURE_COMPARE"
        } else if endpoint.contains("/pulls?") {
            "GH_FIXTURE_PULLS"
        } else {
            "GH_FIXTURE_REPOSITORY"
        };
        if args.iter().any(|arg| arg == "--include") {
            println!("{}\n", headers(variable));
        }
        if variable == "GH_FIXTURE_COMPARE" {
            // A head listed in GH_FIXTURE_BEHIND lacks commits of its base.
            let head = endpoint.rsplit("...").next().unwrap_or_default();
            let behind = std::env::var("GH_FIXTURE_BEHIND")
                .unwrap_or_default()
                .split(',')
                .any(|listed| listed == head);
            println!("{{\"behind_by\":{}}}", u8::from(behind));
        } else if variable == "GH_FIXTURE_PULLS" {
            println!(
                "{}",
                std::env::var(variable).unwrap_or_else(|_| "[]".into())
            );
        } else {
            println!("{}", std::env::var(variable)?);
        }
    } else if args.get(1).is_some_and(|arg| arg == "list") {
        println!(
            "{}",
            std::env::var("GH_FIXTURE_LIST").unwrap_or_else(|_| "[]".into())
        );
    } else if args.get(1).is_some_and(|arg| arg == "view") {
        if let Ok(views) = std::env::var("GH_FIXTURE_VIEWS") {
            // Each PR's reads step through its listed states and stay at the last one.
            let number = args.get(4).ok_or("PR number missing")?;
            let read = std::fs::read_to_string(&log_path)?
                .matches(&format!("\nview\n--repo\n{}\n{number}\n", args[3]))
                .count();
            let views = states::parse(&views)?;
            let states = views.get(number).ok_or("no states for this PR")?;
            println!("{}", states[(read - 1).min(states.len() - 1)]);
            return Ok(());
        }
        let view = std::env::var("GH_FIXTURE_VIEW")?;
        let checks = std::env::var("GH_FIXTURE_CHECKS")
            .unwrap_or_else(|_| r#"[{"status":"COMPLETED","conclusion":"SUCCESS"}]"#.into());
        let mut view = view;
        if !view.contains("statusCheckRollup") {
            view = field(view, "statusCheckRollup", &checks);
        }
        if !view.contains("headRefOid") {
            let head = std::env::var("GH_FIXTURE_HEAD").unwrap_or_default();
            view = field(view, "headRefOid", &format!("\"{head}\""));
        }
        println!("{view}");
    } else {
        println!(
            "{}",
            std::env::var("GH_FIXTURE_CREATED")
                .unwrap_or_else(|_| "https://github.com/owner/project/pull/17".into())
        );
    }
    Ok(())
}

fn field(view: String, name: &str, value: &str) -> String {
    match view.strip_suffix('}') {
        Some(open) if !open.trim().ends_with('{') => format!("{open},\"{name}\":{value}}}"),
        _ => view,
    }
}

/// Only rustc compiles the fixture, so its sequence input uses lines rather than JSON.
/// `GH_FIXTURE_VIEWS` lists each read of a PR as `NUMBER=<compact JSON object>`, one per line.
mod states {
    pub type Map = std::collections::BTreeMap<String, Vec<String>>;

    pub fn parse(text: &str) -> Result<Map, Box<dyn std::error::Error>> {
        let mut map = Map::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let (number, state) = line.split_once('=').ok_or("expected NUMBER=STATE")?;
            map.entry(number.trim().to_owned())
                .or_default()
                .push(state.trim().to_owned());
        }
        Ok(map)
    }
}
