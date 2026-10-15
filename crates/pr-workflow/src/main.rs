use clap::Parser;

fn main() {
    let result = pr_workflow::freshness::print_build_identity().and_then(|printed| {
        if printed {
            return Ok(());
        }
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        pr_workflow::freshness::require_current(pr_workflow::freshness::Built::PrWorkflow)
            .and_then(|()| pr_workflow::execute(pr_workflow::Cli::parse().command, &arguments))
    });
    if let Err(error) = result {
        pr_workflow::report(&error);
        std::process::exit(1);
    }
}
