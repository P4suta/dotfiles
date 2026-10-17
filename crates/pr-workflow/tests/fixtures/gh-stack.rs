use std::io::Write;

/// Records each gh-stack invocation with the repository it targets and whether it ran under the stack lease.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let token = std::env::var("DOTGUARD_STACK").ok();
    let lease = std::env::var_os("GH_STACK_FIXTURE_LEASE")
        .and_then(|path| std::fs::read_to_string(path).ok());
    let leased = token.is_some() && token == lease;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var_os("GH_STACK_FIXTURE_LOG").ok_or("log missing")?)?;
    writeln!(
        log,
        "gh-stack {} repo={} leased={leased} notifier={}",
        args.join(" "),
        std::env::var("GH_REPO").unwrap_or_default(),
        std::env::var("GH_STACK_NO_UPDATE_NOTIFIER").unwrap_or_default()
    )?;
    if let Ok(code) = std::env::var("GH_STACK_FIXTURE_EXIT") {
        std::process::exit(code.parse()?);
    }
    Ok(())
}
