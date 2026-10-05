use clap::Parser;
use dotfiles_xtask::pr_workflow;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = pr_workflow::execute(pr_workflow::Cli::parse().command, &arguments) {
        dotfiles_xtask::report(&error);
        std::process::exit(1);
    }
}
