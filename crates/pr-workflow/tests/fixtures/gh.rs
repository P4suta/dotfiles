use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::env::current_exe()?.file_stem() == Some(std::ffi::OsStr::new("git")) {
        return git(args);
    }
    if std::env::var("GH_HOST")? != "github.com" {
        return Err("incorrect GitHub host".into());
    }
    let log_path = std::env::var_os("GH_FIXTURE_LOG").ok_or("log missing")?;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    writeln!(log, "invocation\n{}", args.join("\n"))?;
    if let Some(path) = std::env::var_os("GH_FIXTURE_REWRITE_FILE")
        && std::env::var("GH_FIXTURE_REWRITE_ENDPOINT")
            .ok()
            .is_none_or(|endpoint| args.get(1) == Some(&endpoint))
    {
        let contents = std::env::var("GH_FIXTURE_REWRITE_CONTENT")
            .unwrap_or_else(|_| "changed after validation".into());
        std::fs::write(path, contents)?;
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
                .unwrap_or_else(|_| format!("HTTP/2.0 200 OK\nX-Ratelimit-Remaining: 4990\nX-Ratelimit-Reset: 1800000000\nX-Ratelimit-Resource: {}", if variable == "GH_FIXTURE_GRAPHQL" { "graphql" } else { "core" }))
        };
        let variable = if endpoint == "graphql" {
            "GH_FIXTURE_GRAPHQL"
        } else if endpoint.contains("/branches/") && !endpoint.contains("/rules/") {
            "GH_FIXTURE_BRANCH"
        } else if endpoint.contains("/reviews?") {
            "GH_FIXTURE_REVIEWS"
        } else if endpoint.contains("/statuses?") {
            "GH_FIXTURE_STATUSES"
        } else if endpoint == "user" {
            "GH_FIXTURE_USER"
        } else if endpoint.contains("/ssh_signing_keys") {
            "GH_FIXTURE_SIGNING_KEYS"
        } else if endpoint.contains("/rules/branches/") {
            "GH_FIXTURE_RULES"
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
        let responses = match variable {
            "GH_FIXTURE_REPOSITORY" => Some("GH_FIXTURE_REPOSITORIES"),
            "GH_FIXTURE_BRANCH" => Some("GH_FIXTURE_BRANCHES"),
            _ => None,
        };
        if let Some(responses) = responses
            && let Some(response) = endpoint_response(responses, &endpoint)?
        {
            println!("{response}");
            return Ok(());
        }
        if variable == "GH_FIXTURE_GRAPHQL" {
            let head = current_graph_head(&args, &log_path)?;
            println!("{}", std::env::var(variable).unwrap_or_else(|_| format!("{{\"data\":{{\"repository\":{{\"pullRequest\":{{\"headRefOid\":\"{head}\",\"reviewThreads\":{{\"pageInfo\":{{\"hasNextPage\":false,\"endCursor\":null}},\"nodes\":[]}}}}}}}}}}")));
        } else if variable == "GH_FIXTURE_BRANCH" {
            let name = endpoint.rsplit('/').next().ok_or("branch missing")?;
            let base = std::env::var("GH_FIXTURE_BASE")?;
            println!(
                "{}",
                std::env::var(variable).unwrap_or_else(|_| format!(
                    "{{\"name\":\"{name}\",\"commit\":{{\"sha\":\"{base}\"}}}}"
                ))
            );
        } else if variable == "GH_FIXTURE_COMPARE" {
            // A head listed in GH_FIXTURE_BEHIND lacks commits of its base.
            let head = endpoint.rsplit("...").next().unwrap_or_default();
            let behind = std::env::var("GH_FIXTURE_BEHIND")
                .unwrap_or_default()
                .split(',')
                .any(|listed| listed == head);
            println!("{{\"behind_by\":{}}}", u8::from(behind));
        } else if [
            "GH_FIXTURE_PULLS",
            "GH_FIXTURE_RULES",
            "GH_FIXTURE_REVIEWS",
            "GH_FIXTURE_STATUSES",
        ]
        .contains(&variable)
        {
            println!(
                "{}",
                std::env::var(variable).unwrap_or_else(|_| "[]".into())
            );
        } else {
            let mut response = std::env::var(variable)?;
            if variable == "GH_FIXTURE_REPOSITORY" && !response.contains("\"default_branch\"") {
                response = field(response, "default_branch", "\"main\"");
            }
            println!("{response}");
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
            println!(
                "{}",
                view_fields(states[(read - 1).min(states.len() - 1)].clone())?
            );
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
            // A push that GH_FIXTURE_PUSHED_HEAD names lands right after `gh pr ready`, so a later read returns it.
            let pushed = std::env::var("GH_FIXTURE_PUSHED_HEAD").ok().filter(|_| {
                std::fs::read_to_string(&log_path).is_ok_and(|log| log.contains("\nready\n"))
            });
            let head = pushed
                .or_else(|| std::env::var("GH_FIXTURE_HEAD").ok())
                .unwrap_or_default();
            view = field(view, "headRefOid", &format!("\"{head}\""));
        }
        if !view.contains("baseRefName") {
            view = field(view, "baseRefName", "\"main\"");
        }
        println!("{}", view_fields(view)?);
    } else {
        println!(
            "{}",
            std::env::var("GH_FIXTURE_CREATED")
                .unwrap_or_else(|_| "https://github.com/owner/project/pull/17".into())
        );
    }
    Ok(())
}

fn endpoint_response(
    variable: &str,
    endpoint: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let responses = match std::env::var(variable) {
        Ok(responses) => responses,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    responses
        .lines()
        .find_map(|line| {
            line.split_once('=')
                .filter(|(key, _)| *key == endpoint)
                .map(|(_, response)| response.to_owned())
        })
        .map(Some)
        .ok_or_else(|| format!("no owned response for {endpoint}").into())
}

fn git(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let real = std::env::var_os("PWF_FIXTURE_REAL_GIT").ok_or("real fixture Git missing")?;
    let mut command = std::process::Command::new(real);
    if args == ["fetch", "--quiet", "origin"] {
        let checkout =
            std::env::var_os("PWF_FIXTURE_FETCH_CHECKOUT").ok_or("owned fetch checkout missing")?;
        let bare = std::env::var_os("PWF_FIXTURE_FETCH_BARE")
            .ok_or("owned fetch bare repository missing")?;
        let current = std::env::current_dir()?.canonicalize()?;
        if current != std::path::Path::new(&checkout).canonicalize()? {
            return Err("fixture fetch escaped its declared checkout".into());
        }
        let repository = std::path::Path::new(&bare).canonicalize()?;
        let parent = current.parent().ok_or("fixture parent missing")?;
        if repository.parent() != Some(parent)
            || !repository.join("HEAD").is_file()
            || !repository.join("objects").is_dir()
        {
            return Err("fixture fetch requires its exact sibling bare repository".into());
        }
        command
            .args(["fetch", "--quiet"])
            .arg(bare)
            .arg("+refs/heads/*:refs/remotes/origin/*");
    } else {
        command.args(args);
    }
    let status = command.status()?;
    std::process::exit(status.code().unwrap_or(1));
}

fn current_graph_head(
    args: &[String],
    log_path: &std::ffi::OsStr,
) -> Result<String, Box<dyn std::error::Error>> {
    if let (Ok(views), Some(index)) = (
        std::env::var("GH_FIXTURE_VIEWS"),
        args.iter().position(|arg| arg == "--input"),
    ) {
        let input = std::fs::read_to_string(&args[index + 1])?;
        if let Some(number) = input.split("\"number\":").nth(1) {
            let number = number
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            let log = std::fs::read_to_string(log_path)?;
            let count = log.matches(&format!("\n{number}\n--json\n")).count();
            let views = states::parse(&views)?;
            if let Some(states) = views.get(&number) {
                let latest = &states[count.saturating_sub(1).min(states.len() - 1)];
                if let Some((start, end)) = string_field(latest, "headRefOid") {
                    return Ok(latest[start..end].into());
                }
            }
        }
    }
    Ok(std::env::var("GH_FIXTURE_HEAD")?)
}

fn string_field(view: &str, name: &str) -> Option<(usize, usize)> {
    let needle = format!("\"{name}\":");
    let mut start = view.find(&needle)? + needle.len();
    while view
        .as_bytes()
        .get(start)
        .is_some_and(u8::is_ascii_whitespace)
    {
        start += 1;
    }
    if view.as_bytes().get(start) != Some(&b'"') {
        return None;
    }
    let mut escaped = false;
    for (offset, byte) in view.as_bytes()[start + 1..].iter().copied().enumerate() {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some((start + 1, start + 1 + offset));
        }
    }
    None
}

fn view_fields(mut view: String) -> Result<String, Box<dyn std::error::Error>> {
    let head = std::env::var("GH_FIXTURE_HEAD")?;
    let mut base = std::env::var("GH_FIXTURE_BASE")?;
    if let (Ok(bases), Some((start, end))) = (
        std::env::var("GH_FIXTURE_BASES"),
        string_field(&view, "headRefOid"),
    ) {
        if let Some(selected) = bases.lines().find_map(|line| {
            line.split_once('=')
                .filter(|(head, _)| *head == &view[start..end])
                .map(|(_, base)| base.to_owned())
        }) {
            base = selected;
        }
    }
    for (name, value) in [
        ("headRefOid", format!("\"{head}\"")),
        ("baseRefOid", format!("\"{base}\"")),
        ("baseRefName", "\"main\"".into()),
        ("author", "{\"login\":\"P4suta\"}".into()),
        ("labels", "[]".into()),
    ] {
        if !view.contains(&format!("\"{name}\":")) {
            view = field(view, name, &value);
        }
    }
    let assessment = if let (Ok(records), Some((start, end))) = (
        std::env::var("GH_FIXTURE_ASSESSMENTS"),
        string_field(&view, "headRefOid"),
    ) {
        records.lines().find_map(|line| {
            line.split_once('=')
                .filter(|(head, _)| *head == &view[start..end])
                .map(|(_, packet)| packet.to_owned())
        })
    } else {
        None
    }
    .or_else(|| std::env::var("GH_FIXTURE_ASSESSMENT").ok());
    if let (Some(assessment), Some((start, end))) = (assessment, string_field(&view, "body")) {
        if !view[start..end].contains("pr-review-assessment:") {
            let standing = if std::env::var("GH_FIXTURE_LEVEL").as_deref() != Ok("critical")
                && !view[start..end].contains("@coderabbitai ignore")
            {
                "\\n@coderabbitai ignore\\n"
            } else {
                "\\n"
            };
            let encoded = assessment
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .ok_or("encoded assessment missing")?;
            view.insert_str(end, &format!("{standing}{encoded}\\n"));
        }
    }
    Ok(view)
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
