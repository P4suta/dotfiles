use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use prose::{self, Bundle, Channel, Scope};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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
    /// Find unfinished structure and common editorial shortcuts in Markdown.
    Editorial {
        #[arg(long, value_enum, default_value = "expository")]
        profile: prose::editorial::Profile,
        #[arg(long, value_enum, default_value = "text")]
        format: EditorialFormat,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Typecheck the adapters and exercise all three pinned language pipelines.
    CopyeditSelftest,
    /// Run the locked local linguistic engines for an explicitly selected language.
    Copyedit {
        #[arg(long, value_enum)]
        language: prose::copyedit::Language,
        /// Write a new source-bound JSON report without overwriting an earlier pass.
        #[arg(long)]
        report: PathBuf,
        /// Print the coverage summary and keep every diagnostic in the report.
        #[arg(long)]
        quiet: bool,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Create pending review decisions without retaining any diagnostic.
    CopyeditReview {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Refuse unresolved or stale findings and repeat every local linguistic check.
    CopyeditVerify {
        #[arg(long)]
        report: PathBuf,
        /// Keep only the exact review candidates with recorded source evidence.
        #[arg(long)]
        decisions: Option<PathBuf>,
    },
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

#[derive(Clone, Copy, clap::ValueEnum)]
enum EditorialFormat {
    Text,
    Json,
}

fn editorial(
    bundle: &Bundle,
    profile: prose::editorial::Profile,
    format: EditorialFormat,
    report: Option<&Path>,
    paths: &[PathBuf],
) -> Result<i32> {
    anyhow::ensure!(
        report.is_none_or(|path| !path.exists()),
        "the editorial report already exists"
    );
    let sources = paths
        .iter()
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .with_context(|| format!("read {}", path.display()))?;
            let mut checked = prose::editorial::inspect(&path.to_string_lossy(), &source, profile)?;
            checked.findings.extend(
                prose::house::check(&path.to_string_lossy(), &source, || Ok(bundle.clone()))?
                    .into_iter()
                    .map(|finding| prose::editorial::Finding {
                        line: finding.line,
                        column: finding.column,
                        rule: finding.rule,
                        message: finding.message,
                        snippet: finding.excerpt,
                    }),
            );
            checked.findings.sort_by(|left, right| {
                (left.line, left.column, &left.rule).cmp(&(right.line, right.column, &right.rule))
            });
            Ok(checked)
        })
        .collect::<Result<Vec<_>>>()?;
    let result = prose::editorial::Report::new(profile, sources);
    let encoded = serde_json::to_vec_pretty(&result)?;
    if let Some(path) = report {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
    }
    match format {
        EditorialFormat::Json => {
            std::io::stdout().write_all(&encoded)?;
            println!();
        }
        EditorialFormat::Text => {
            for source in &result.sources {
                for finding in &source.findings {
                    println!(
                        "{}:{}:{}: {}: {}",
                        source.path, finding.line, finding.column, finding.rule, finding.message
                    );
                }
            }
            println!(
                "Checked {} source(s); {} editorial candidate(s)",
                result.sources.len(),
                result
                    .sources
                    .iter()
                    .map(|source| source.findings.len())
                    .sum::<usize>()
            );
        }
    }
    Ok(i32::from(!result.clean()))
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

fn copyedit(
    bundle: &Bundle,
    language: prose::copyedit::Language,
    report: &PathBuf,
    paths: &[PathBuf],
    quiet: bool,
) -> Result<i32> {
    anyhow::ensure!(!report.exists(), "the copyediting report already exists");
    let documents = paths
        .iter()
        .map(|path| prose::copyedit::Document::read(path))
        .collect::<Result<Vec<_>>>()?;
    let result = prose::copyedit::inspect(bundle, language, &documents)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report)?;
    file.write_all(&serde_json::to_vec_pretty(&result)?)?;
    file.sync_all()?;
    for finding in result.diagnostics.iter().filter(|_| !quiet) {
        eprintln!(
            "{}:{}:{}: {}: {}",
            finding.path, finding.line, finding.column, finding.rule, finding.message
        );
    }
    println!(
        "Checked {} source(s), {} prose block(s), {} diagnostic(s); report: {}",
        result.sources.len(),
        result.segments.len(),
        result.diagnostics.len(),
        report.display()
    );
    Ok(i32::from(!result.clean()))
}

fn copyedit_review(report: &Path, output: &Path) -> Result<i32> {
    let report: prose::copyedit::Report = serde_json::from_reader(std::fs::File::open(report)?)?;
    let decisions = report.review_template()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec_pretty(&decisions)?)?;
    file.sync_all()?;
    println!("Created pending decisions: {}", output.display());
    Ok(0)
}

fn copyedit_verify(bundle: &Bundle, report: &Path, decisions: Option<&Path>) -> Result<i32> {
    let report: prose::copyedit::Report = serde_json::from_reader(std::fs::File::open(report)?)?;
    match decisions {
        Some(path) => {
            let decisions = serde_json::from_reader(std::fs::File::open(path)?)?;
            report.verify_reviewed(bundle, &decisions)?;
        }
        None => report.verify_current(bundle)?,
    }
    println!(
        "Repeated all engines for {} unchanged source(s); {} retained review candidate(s)",
        report.sources.len(),
        report.diagnostics.len()
    );
    Ok(0)
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
        Commands::Editorial {
            profile,
            format,
            report,
            paths,
        } => editorial(&bundle()?, profile, format, report.as_deref(), &paths),
        Commands::CopyeditSelftest => {
            prose::copyedit::verify_runtime(&bundle()?)?;
            println!(
                "Verified typed adapters and the English, Japanese, and Chinese runtime controls"
            );
            Ok(0)
        }
        Commands::Copyedit {
            language,
            report,
            paths,
            quiet,
        } => copyedit(&bundle()?, language, &report, &paths, quiet),
        Commands::CopyeditReview { report, output } => copyedit_review(&report, &output),
        Commands::CopyeditVerify { report, decisions } => {
            copyedit_verify(&bundle()?, &report, decisions.as_deref())
        }
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
