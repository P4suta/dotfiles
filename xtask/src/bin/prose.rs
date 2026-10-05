use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dotfiles_xtask::prose::{self, Bundle, Channel, Client, Scope};
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Check persisted prose and final replies against the pinned writing standard")]
struct Cli {
    /// Read the configuration from this dotfiles checkout instead of the installed one.
    #[arg(long, global = true)]
    source: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Check texts for one channel, reading standard input for `-` or no path.
    Check {
        #[arg(long, value_enum)]
        channel: Channel,
        /// The title of a pull request or issue, checked with its body.
        #[arg(long)]
        title: Option<String>,
        paths: Vec<PathBuf>,
    },
    /// Check the repository with its exemptions and legacy ledger.
    Repository {
        #[arg(long, value_enum, required = true)]
        scope: Vec<Scope>,
        /// Print each finding that no exemption covers, with its ledger fingerprint, instead of judging them.
        #[arg(long)]
        findings: bool,
    },
    /// Remove legacy ledger entries whose findings no longer occur.
    Tighten {
        #[arg(long, value_enum, required = true)]
        scope: Vec<Scope>,
    },
    /// Answer a client's end-of-turn hook for its final reply.
    Reply {
        #[arg(long, value_enum)]
        client: Client,
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
        Some(root) => dotfiles_xtask::canonical(root)?,
        None => std::env::current_dir()?,
    };
    let bundle = || match &cli.source {
        Some(_) => Bundle::source(&root),
        None => Bundle::installed(),
    };
    match cli.command {
        Action::Check {
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
        Action::Repository { scope, findings } => {
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
                                "fingerprint": prose::fingerprint(&finding.sentence),
                            })
                        );
                    }
                } else {
                    prose::repository(&root, scope, &bundle)?;
                }
            }
            Ok(0)
        }
        Action::Tighten { scope } => {
            let bundle = Bundle::source(&root)?;
            for scope in scope {
                prose::tighten(&root, scope, &bundle)?;
            }
            Ok(0)
        }
        Action::Reply { client: _ } => {
            let mut text = String::new();
            std::io::stdin().take(8_388_609).read_to_string(&mut text)?;
            let input = serde_json::from_str(&text).context("invalid native hook input")?;
            println!("{}", prose::stop(&input, bundle()));
            Ok(0)
        }
    }
}

fn main() {
    match run(Cli::parse()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Error: {error:#}");
            std::process::exit(2);
        }
    }
}
