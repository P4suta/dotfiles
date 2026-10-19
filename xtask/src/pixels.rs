//! Advisory pixel comparisons in one pinned terminal recorder image.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use clap::Args;
use rules::contrast_rules::parse;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

pub const IMAGE: &str = "ghcr.io/charmbracelet/vhs:v0.12.1@sha256:ea49a6a1c529be83153e88321892b5585964418f0b9055e8c1e0d732194234a3";
const MOUNT: &str = "/vhs";
const TAPE: &str = "pixels.tape";
const STATS: &str = "ssim.txt";
const COMPARE: &str = "vhs pixels.tape && ffmpeg -v error -i golden.png -i rendered.png -filter_complex ssim=stats_file=ssim.txt -f null -";
const FOREGROUND: u8 = 38;
const BACKGROUND: u8 = 48;
const DEFAULT_FOREGROUND: &str = "39";
const DEFAULT_BACKGROUND: &str = "49";
const DIM: u8 = 2;
const ITALIC: u8 = 3;
const UNDERLINE: u8 = 4;
const REVERSE: u8 = 7;

#[derive(Args, Debug)]
pub struct Request {
    #[arg(long)]
    pub golden: PathBuf,
    #[arg(long)]
    pub rendered: PathBuf,
    #[arg(long)]
    pub output: PathBuf,
}

fn color(value: &str, foreground: bool) -> Result<String> {
    let base = if foreground { FOREGROUND } else { BACKGROUND };
    if value == "-" {
        return Ok(if foreground {
            DEFAULT_FOREGROUND
        } else {
            DEFAULT_BACKGROUND
        }
        .to_owned());
    }
    if let Some(color) = parse(value.as_bytes()) {
        Ok(format!(
            "{base};2;{};{};{}",
            color.red, color.green, color.blue
        ))
    } else {
        let index: u8 = value.parse().context("invalid terminal palette index")?;
        Ok(format!("{base};5;{index}"))
    }
}

/// Replays a canonical cell grid without permitting embedded terminal control characters.
///
/// # Errors
///
/// Returns an error for an empty grid, malformed cell, duplicate position, or unsupported attribute.
pub fn replay(grid: &str) -> Result<String> {
    let mut output = String::from("\x1b[0m\x1b[2J\x1b[H\x1b[?25l");
    let mut seen = BTreeSet::new();
    for line in grid.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [position, text, foreground, background, flags] = fields.as_slice() else {
            anyhow::bail!("invalid cell fields");
        };
        let (row, column) = position.split_once(',').context("invalid cell position")?;
        let (row, column): (u16, u16) = (row.parse()?, column.parse()?);
        ensure!(
            row < crate::theme::GRID_ROWS && column < crate::theme::GRID_COLUMNS,
            "cell lies outside the fixed terminal"
        );
        ensure!(seen.insert((row, column)), "duplicate cell position");
        ensure!(
            !text.chars().any(char::is_control),
            "control character in cell text"
        );
        write!(
            output,
            "\x1b[{};{}H\x1b[0;{};{}",
            row + 1,
            column + 1,
            color(foreground, true)?,
            color(background, false)?
        )?;
        for flag in flags.chars() {
            let code = match flag {
                'b' => 1,
                'd' => DIM,
                'i' => ITALIC,
                'u' => UNDERLINE,
                'r' => REVERSE,
                _ => anyhow::bail!("unknown cell attribute {flag}"),
            };
            write!(output, ";{code}")?;
        }
        write!(output, "m{text}")?;
    }
    ensure!(!seen.is_empty(), "empty terminal grid");
    output.push_str("\x1b[0m\x1b[H");
    Ok(output)
}

fn compare(request: &Request) -> Result<String> {
    fs::create_dir_all(&request.output)?;
    let directory = crate::canonical(&request.output)?;
    for (source, name) in [
        (&request.golden, "golden.ansi"),
        (&request.rendered, "rendered.ansi"),
    ] {
        fs::write(directory.join(name), replay(&fs::read_to_string(source)?)?)?;
    }
    let tape = format!(
        "Output comparison.gif\nSet Shell bash\nSet Columns {}\nSet Rows {}\nSet FontFamily \"DejaVu Sans Mono\"\nSet FontSize 16\nSet Padding 0\nSet Margin 0\nSet CursorBlink false\nHide\nType \"export PS1=''; cat golden.ansi\"\nEnter\nSleep 1s\nShow\nSleep 200ms\nScreenshot golden.png\nHide\nType \"cat rendered.ansi\"\nEnter\nSleep 1s\nShow\nSleep 200ms\nScreenshot rendered.png\n",
        crate::theme::GRID_COLUMNS,
        crate::theme::GRID_ROWS
    );
    fs::write(directory.join(TAPE), tape)?;
    let mut mount = std::ffi::OsString::from("type=bind,\"source=");
    mount.push(&directory);
    mount.push(format!("\",target={MOUNT}"));
    let output = Tool::Docker
        .command()
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--cpus",
            "2",
            "--memory",
            "2g",
            "--mount",
        ])
        .arg(mount)
        .args([
            "--entrypoint",
            "sh",
            "--workdir",
            MOUNT,
            IMAGE,
            "-ec",
            COMPARE,
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "VHS comparison failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stats = fs::read_to_string(directory.join(STATS))?;
    ensure!(
        stats
            .split_whitespace()
            .any(|field| field.starts_with("All:")),
        "SSIM report has no overall score"
    );
    Ok(stats)
}

/// Reports a pixel score or an unavailable renderer without affecting the cell-grid gate.
pub fn advisory(request: &Request) {
    match compare(request) {
        Ok(stats) => println!("Pixel advisory in {IMAGE}: {}", stats.trim()),
        Err(error) => println!("Pixel advisory unavailable: {error:#}"),
    }
}
