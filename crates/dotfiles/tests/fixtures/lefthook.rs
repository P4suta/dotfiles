//! A stand-in for lefthook that fails to load its configuration.
//! With `FIXTURE_LEFTHOOK_JOB` it loads the configuration and fails that job, as Lefthook prints a failure on standard output.
//! With `FIXTURE_LEFTHOOK_REFUSAL` set as well, the job prints a refusal record of its own.

fn main() {
    let job = std::env::var("FIXTURE_LEFTHOOK_JOB").ok();
    match (std::env::args().nth(1).as_deref(), job) {
        (Some("dump"), None) => {
            eprintln!("fixture: invalid lefthook configuration");
            std::process::exit(1);
        }
        (Some("dump"), Some(_)) => println!("pre-push: {{}}"),
        (Some("run"), Some(job)) => {
            println!("\u{1b}[1mchecking the change\u{1b}[0m");
            println!("the change lacks an issue");
            if std::env::var_os("FIXTURE_LEFTHOOK_REFUSAL").is_some() {
                println!(
                    "dotfiles-refusal/2 {{\"rule\":\"tool.stale\",\"cause\":\"the installed tool is stale\",\"evidence\":[\"fixture\"],\"next\":\"just install-pr-workflow\",\"bypass\":null}}"
                );
            }
            println!("\nexit status 1");
            println!("\u{1f94a} {job} (0.03 seconds)");
            std::process::exit(1);
        }
        _ => {}
    }
}
