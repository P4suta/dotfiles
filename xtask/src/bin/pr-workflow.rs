use clap::Parser;
use dotfiles_xtask::pr_workflow;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result =
        dotfiles_xtask::freshness::require_current(dotfiles_xtask::freshness::Built::PrWorkflow)
            .and_then(|()| pr_workflow::execute(pr_workflow::Cli::parse().command, &arguments));
    if let Err(error) = result {
        dotfiles_xtask::report(&error);
        std::process::exit(1);
    }
}
