use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use prose::{self, Bundle, Channel, Scope};
use std::io::Read;
use std::path::PathBuf;

/// The exit status of a run that failed before it could check anything, distinct from a refusal.
const FAILED: i32 = 2;

#[derive(Parser)]
#[command(about = "Check persisted prose against the pinned writing standard")]
struct Cli {
    /// Read the configuration from this dotfiles checkout instead of the installed one.
    #[arg(long, global = true)]
    source: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check texts for one channel, reading standard input for `-` or no path.
    Check {
        #[arg(long, value_enum)]
        channel: Channel,
        /// The title of a pull request or issue, checked with its body.
        #[arg(long)]
        title: Option<String>,
        paths: Vec<PathBuf>,
    },
    /// Check the repository with its exemptions.
    Repository {
        #[arg(long, value_enum, required = true)]
        scope: Vec<Scope>,
        /// Print each finding that no exemption covers instead of judging them.
        #[arg(long)]
        findings: bool,
    },
}

fn read(path: &PathBuf) -> Result<String> {
    if path.as_os_str() == "-" {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        Ok(text)
    } else {
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))
    }
}

fn run(cli: Cli) -> Result<i32> {
    let root = match &cli.source {
        Some(root) => prose::canonical(root)?,
        None => std::env::current_dir()?,
    };
    let bundle = || match &cli.source {
        Some(_) => Bundle::source(&root),
        None => Bundle::installed(),
    };
    match cli.command {
        Commands::Check {
            channel,
            title,
            paths,
        } => {
            let paths = if paths.is_empty() {
                vec![PathBuf::from("-")]
            } else {
                paths
            };
            let bundle = bundle()?;
            let mut findings = Vec::new();
            if channel == Channel::Comment {
                let names: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                findings.extend(prose::check_comments(&bundle, &root, &names)?);
            } else {
                for path in &paths {
                    let mut text = read(path)?;
                    if let Some(title) = &title {
                        text = prose::publication(Some(title), &text);
                    }
                    let name = path.to_string_lossy();
                    findings.extend(prose::check(&bundle, channel, &name, &text)?);
                }
            }
            if findings.is_empty() {
                return Ok(0);
            }
            eprintln!(
                "{}\n\nRewrite these sentences and rerun `prose check --channel {}`.",
                prose::render(&findings),
                format!("{channel:?}").to_lowercase()
            );
            Ok(1)
        }
        Commands::Repository { scope, findings } => {
            let bundle = Bundle::source(&root)?;
            for scope in scope {
                if findings {
                    for finding in prose::findings(&root, scope, &bundle)? {
                        println!(
                            "{}",
                            serde_json::json!({
                                "scope": scope,
                                "path": finding.path,
                                "line": finding.line,
                                "rule": finding.rule,
                                "sentence": finding.sentence,
                            })
                        );
                    }
                } else {
                    prose::repository(&root, scope, &bundle)?;
                }
            }
            Ok(0)
        }
    }
}

fn main() {
    match run(Cli::parse()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Error: {error:#}");
            std::process::exit(FAILED);
        }
    }
}
