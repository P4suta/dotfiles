use clap::Parser;

fn main() {
    if let Err(error) =
        dotfiles_xtask::pr_workflow::run(dotfiles_xtask::pr_workflow::Cli::parse().command)
    {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
