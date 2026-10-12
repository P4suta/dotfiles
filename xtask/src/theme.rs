//! Every theme uses one palette and keeps text contrast at 4.5:1 or higher.

use crate::tool::Tool;
use anyhow::{Context, Result, ensure};
use rules::contrast_rules::{self, Rgb};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The hexadecimal digits of a `#rrggbb` literal.
const HEX_DIGITS: usize = 6;

/// Shares, rates, and ratios in thousandths.
const MILLIS: u64 = 1000;

/// The scale for a 1% match rate.
const MILLIS_PER_PERCENT: u64 = 10;

/// The terminal every cell grid renders into.
pub const GRID_ROWS: u16 = 24;
pub const GRID_COLUMNS: u16 = 80;

/// The committed cell grids and the file the pager renders, relative to the checkout.
pub const GOLDENS: &str = "xtask/tests/goldens";
const PAGER_FIXTURE: &str = "xtask/tests/goldens/fixture.rs";

/// The starship modules whose output depends on the clock, the host, or the previous command, which the grid turns off.
const VOLATILE_MODULES: [&str; 5] = ["time", "os", "cmd_duration", "hostname", "username"];
const FZF_ITEMS: [&str; 3] = ["alpha.rs", "beta.toml", "gamma.md"];
const FZF_READY: &str = "Dotfiles palette fixture";
const TERMINAL_TIMEOUT: Duration = Duration::from_secs(10);
const TERMINAL_READ_BYTES: usize = 4096;
const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CONTAINER_OPTIONS_START: usize = 2;
#[cfg(windows)]
const FZF_WSL_DISTRIBUTION: &str = "DOTFILES_FZF_WSL_DISTRIBUTION";
#[cfg(windows)]
const WSL_EXECUTABLE: &str = "wsl.exe";
#[cfg(windows)]
const WSL_FZF_GATE: &str = "set -euo pipefail; export PATH=\"$HOME/.local/bin:$PATH\"; export MISE_PARANOID=1 MISE_TRUSTED_CONFIG_PATHS=\"$PWD/mise.toml\" MISE_CEILING_PATHS=\"$(dirname \"$PWD\")\"; export CARGO_TARGET_DIR=\"$HOME/.cache/dotfiles/fzf-wsl/target\" CARGO_BUILD_JOBS=2; rust=\"$(mise config get --file mise.toml tools.rust.version)\"; fzf=\"$(mise config get --file mise.toml tools.fzf)\"; test \"$(mise tool rust --requested)\" = \"$rust\"; test \"$(mise tool fzf --requested)\" = \"$fzf\"; exec mise --locked x \"rust@$rust\" \"fzf@$fzf\" -- cargo xtask fzf-grid";

fn color(color: vt100::Color) -> String {
    match color {
        vt100::Color::Default => "-".to_owned(),
        vt100::Color::Idx(index) => format!("{index}"),
        vt100::Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
    }
}

/// The cells a terminal shows after `output`, one line each for every cell with text, a color, or an attribute.
#[must_use]
pub fn grid(output: &[u8]) -> Vec<String> {
    let mut parser = vt100::Parser::new(GRID_ROWS, GRID_COLUMNS, 0);
    let mut normalized = Vec::with_capacity(output.len());
    for byte in output {
        if *byte == b'\n' {
            normalized.push(b'\r');
        }
        normalized.push(*byte);
    }
    parser.process(&normalized);
    let screen = parser.screen();
    let mut cells = Vec::new();
    for row in 0..GRID_ROWS {
        for column in 0..GRID_COLUMNS {
            let Some(cell) = screen.cell(row, column) else {
                continue;
            };
            let flags: String = [
                (cell.bold(), 'b'),
                (cell.dim(), 'd'),
                (cell.italic(), 'i'),
                (cell.underline(), 'u'),
                (cell.inverse(), 'r'),
            ]
            .iter()
            .filter(|(set, _)| *set)
            .map(|(_, flag)| *flag)
            .collect();
            let (foreground, background) = (color(cell.fgcolor()), color(cell.bgcolor()));
            if cell.has_contents() || foreground != "-" || background != "-" || !flags.is_empty() {
                cells.push(format!(
                    "{row:02},{column:02}\t{}\t{foreground}\t{background}\t{flags}",
                    cell.contents()
                ));
            }
        }
    }
    cells
}

/// The share of cells, in thousandths, that two grids agree on.
#[must_use]
pub fn match_millis(golden: &[String], rendered: &[String]) -> u64 {
    let golden: BTreeSet<&String> = golden.iter().collect();
    let rendered: BTreeSet<&String> = rendered.iter().collect();
    let all = golden.union(&rendered).count();
    let agreed = golden.intersection(&rendered).count();
    if all == 0 {
        return 0;
    }
    let agreed = u64::try_from(agreed).unwrap_or(u64::MAX / MILLIS);
    let all = u64::try_from(all).unwrap_or(1);
    agreed * MILLIS / all
}

/// The rendered prompt of a starship configuration in a fixture repository under a fixture home, with its volatile modules off.
fn starship_output(config: &str, scope: &Path) -> Result<Vec<u8>> {
    let mut table: toml::Table =
        toml::from_str(config).context("parse the rendered starship.toml")?;
    for module in VOLATILE_MODULES {
        let entry = table
            .entry(module)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(entry) = entry.as_table_mut() {
            entry.insert("disabled".to_owned(), toml::Value::Boolean(true));
        }
    }
    // The fixture repository lies in a temporary directory, so the directory module shows only the repository's own name.
    if let Some(directory) = table
        .entry("directory")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
    {
        for (key, value) in [
            ("use_os_path_sep", toml::Value::Boolean(false)),
            ("truncate_to_repo", toml::Value::Boolean(true)),
            ("truncation_length", toml::Value::Integer(1)),
            ("fish_style_pwd_dir_length", toml::Value::Integer(0)),
        ] {
            directory.insert(key.to_owned(), value);
        }
    }
    let home = scope.join("home");
    let project = home.join("project");
    fs::create_dir_all(&project)?;
    let status = Tool::Git
        .command()
        .current_dir(&project)
        .args(["init", "--quiet", "--initial-branch", "main"])
        .status()?;
    ensure!(status.success(), "git init failed for the prompt fixture");
    let file = scope.join("starship.toml");
    fs::write(&file, toml::to_string(&table)?)?;
    let output = Tool::Starship
        .command()
        .current_dir(&project)
        .args([
            "prompt",
            "--terminal-width",
            &GRID_COLUMNS.to_string(),
            "--status",
            "0",
            "--cmd-duration",
            "0",
            "--jobs",
            "0",
        ])
        .arg("--path")
        .arg(&project)
        .arg("--logical-path")
        .arg(&project)
        .env("PWD", &project)
        .env("STARSHIP_CONFIG", &file)
        .env("STARSHIP_CACHE", scope.join("cache"))
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env_remove("STARSHIP_SHELL")
        .env("TERM", "xterm-256color")
        .env("COLUMNS", GRID_COLUMNS.to_string())
        .env("LINES", GRID_ROWS.to_string())
        .output()
        .context("start starship")?;
    ensure!(
        output.status.success(),
        "starship failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

/// The pager's rendering of the committed fixture with a bat configuration.
fn bat_output(root: &Path, config: &str, scope: &Path) -> Result<Vec<u8>> {
    let file = scope.join("bat-config");
    fs::write(&file, config)?;
    let output = crate::tool::external("bat")
        .args([
            "--color=always",
            "--paging=never",
            "--decorations=always",
            "--terminal-width",
            &GRID_COLUMNS.to_string(),
        ])
        .arg(root.join(PAGER_FIXTURE))
        .env("BAT_CONFIG_PATH", &file)
        .env("BAT_CACHE_PATH", scope.join("cache"))
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("COLUMNS", GRID_COLUMNS.to_string())
        .env("LINES", GRID_ROWS.to_string())
        .output()
        .context("start bat")?;
    ensure!(
        output.status.success(),
        "bat failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn fzf_output(config: &str, scope: &Path) -> Result<Vec<u8>> {
    use portable_pty::{NativePtySystem, PtySize, PtySystem};
    for file in FZF_ITEMS {
        fs::write(scope.join(file), "fixture\n")?;
    }
    let settings = scope.with_extension("fzfrc");
    fs::write(&settings, config)?;
    let pair = NativePtySystem::default().openpty(PtySize {
        rows: GRID_ROWS,
        cols: GRID_COLUMNS,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut command = Tool::Fzf.command();
    command
        .current_dir(scope)
        .args([
            "--no-clear",
            "--no-mouse",
            "--walker=file",
            "--walker-root=.",
            "--no-sort",
            "--bind",
        ])
        .arg(format!("load:change-header({FZF_READY})"))
        .env("FZF_DEFAULT_OPTS_FILE", &settings)
        .env_remove("FZF_DEFAULT_OPTS")
        .env_remove("FZF_DEFAULT_COMMAND")
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("COLUMNS", GRID_COLUMNS.to_string())
        .env("LINES", GRID_ROWS.to_string());
    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let mut child = pair
        .slave
        .spawn_command(platform::process::pty_command(&command))?;
    drop(pair.slave);
    let (sender, receiver) = mpsc::channel();
    let reading = thread::spawn(move || {
        let mut buffer = [0; TERMINAL_READ_BYTES];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(length) => {
                    if sender.send(Ok(buffer[..length].to_vec())).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _sent = sender.send(Err(error));
                    break;
                }
            }
        }
    });
    let capture = (|| -> Result<Vec<u8>> {
        let deadline = Instant::now() + TERMINAL_TIMEOUT;
        let mut output = Vec::new();
        let mut terminal = vt100::Parser::new(GRID_ROWS, GRID_COLUMNS, 0);
        let mut answered = 0;
        loop {
            let chunk = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .with_context(|| {
                    format!(
                        "fzf didn't draw its complete loaded fixture: {:?}",
                        terminal.screen().contents()
                    )
                })??;
            terminal.process(&chunk);
            output.extend(chunk);
            let requested = output
                .windows(CURSOR_QUERY.len())
                .filter(|window| *window == CURSOR_QUERY)
                .count();
            for _ in answered..requested {
                let (row, column) = terminal.screen().cursor_position();
                write!(writer, "\x1b[{};{}R", row + 1, column + 1)?;
                writer.flush()?;
            }
            answered = requested;
            let contents = terminal.screen().contents();
            if contents.contains(FZF_READY) && FZF_ITEMS.iter().all(|file| contents.contains(file))
            {
                return Ok(output);
            }
        }
    })();
    platform::cleanup::report(child.kill(), "stop the fzf fixture");
    platform::cleanup::report(child.wait(), "reap the fzf fixture");
    drop(writer);
    drop(pair.master);
    drop(receiver);
    ensure!(reading.join().is_ok(), "the fzf terminal reader panicked");
    capture
}

fn transported_fzf_grid(mut command: Command, config: &str) -> Result<Vec<String>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start the Linux fzf renderer")?;
    child
        .stdin
        .take()
        .context("the fzf renderer has piped input")?
        .write_all(config.as_bytes())?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "the Linux fzf renderer failed: {}",
        output.status
    );
    serde_json::from_slice(&output.stdout).context("the Linux fzf renderer returned no cell grid")
}

#[cfg(windows)]
fn wsl_fzf_grid(root: &Path, distribution: &std::ffi::OsStr, config: &str) -> Result<Vec<String>> {
    ensure!(
        !distribution.is_empty(),
        "{FZF_WSL_DISTRIBUTION} must name the owned renderer distribution"
    );
    let mut command = platform::process::command(WSL_EXECUTABLE);
    command
        .arg("--distribution")
        .arg(distribution)
        .arg("--cd")
        .arg(crate::canonical(root)?)
        .args(["--exec", "bash", "-lc", WSL_FZF_GATE]);
    transported_fzf_grid(command, config)
}

/// Renders the fzf fixture on a Unix terminal, using the declared WSL distribution or the pinned Linux image on Windows.
///
/// # Errors
///
/// Returns an error if the configured terminal renderer fails.
pub fn fzf_grid(root: &Path, config: &str) -> Result<Vec<String>> {
    #[cfg(windows)]
    if let Some(distribution) = std::env::var_os(FZF_WSL_DISTRIBUTION) {
        return wsl_fzf_grid(root, &distribution, config);
    }
    if cfg!(windows) {
        let root = crate::canonical(root)?;
        let image = crate::skill_proofs::proof_image(&root)?;
        crate::skill_proofs::ensure_image(
            &root,
            &root.join(crate::skill_proofs::PROOF_IMAGE),
            &image,
        )?;
        let volume = crate::skill_proofs::container_target_volume(&root);
        let mut arguments = crate::skill_proofs::container_run_arguments(&root, &image, &volume);
        arguments.splice(
            CONTAINER_OPTIONS_START..CONTAINER_OPTIONS_START,
            [
                "-i".into(),
                "--cpus".into(),
                "3".into(),
                "--memory".into(),
                "6g".into(),
                "--volume".into(),
                format!("{volume}-mise:/root/.local/share/mise").into(),
            ],
        );
        *arguments.last_mut().context("the image command has a renderer")? = "set -eo pipefail; tar -C /source --exclude=.git --exclude=node_modules --exclude=target -cf - . | tar -C /work -xf -; mise trust --quiet; cargo run --locked --package dotfiles --bin dotfiles -- mise-install fzf; export MISE_AUTO_INSTALL=false; exec mise x -- cargo xtask fzf-grid".into();
        let mut command = Tool::Docker.command();
        command.args(arguments);
        return transported_fzf_grid(command, config);
    }
    let scope = tempfile::tempdir()?;
    Ok(grid(&fzf_output(config, scope.path())?))
}

/// The cell grids of one profile's prompt and pager, by golden filename.
///
/// # Errors
///
/// Returns an error when a renderer can't start or fails.
pub fn grids(root: &Path, profile: &str, dump: &Value) -> Result<Vec<(String, Vec<String>)>> {
    let scope = tempfile::tempdir()?;
    let mut found = Vec::new();
    if let Some(config) = dump[".config/starship.toml"]["contents"].as_str() {
        let directory = scope.path().join("starship");
        fs::create_dir_all(&directory)?;
        found.push((
            format!("{profile}-starship.grid"),
            grid(&starship_output(config, &directory)?),
        ));
    }
    if let Some(config) = dump[".config/bat/config"]["contents"].as_str() {
        let directory = scope.path().join("bat");
        fs::create_dir_all(&directory)?;
        found.push((
            format!("{profile}-bat.grid"),
            grid(&bat_output(root, config, &directory)?),
        ));
    }
    if let Some(config) = dump[".config/fzf/fzfrc"]["contents"].as_str() {
        found.push((format!("{profile}-fzf.grid"), fzf_grid(root, config)?));
    }
    Ok(found)
}

/// Compares one profile's cell grids with the committed goldens, or with `write` records them, and returns a refusal for each grid below a full match.
///
/// # Errors
///
/// Returns an error when rendering or reading or writing a golden fails.
pub fn check_grids(root: &Path, profile: &str, dump: &Value, write: bool) -> Result<Vec<String>> {
    let mut refusals = Vec::new();
    for (name, rendered) in grids(root, profile, dump)? {
        let path = root.join(GOLDENS).join(&name);
        if write {
            fs::write(&path, format!("{}\n", rendered.join("\n")))?;
            continue;
        }
        let golden: Vec<String> = fs::read_to_string(&path)
            .with_context(|| {
                format!("{GOLDENS}/{name} is missing; run `cargo xtask theme-grids --write`")
            })?
            .lines()
            .map(str::to_owned)
            .collect();
        let rate = match_millis(&golden, &rendered);
        println!(
            "{name} matches {}.{}% of its cells",
            rate / MILLIS_PER_PERCENT,
            rate % MILLIS_PER_PERCENT
        );
        if rate < MILLIS {
            refusals.push(format!(
                "the {profile} profile renders {name} with {}.{}% of its cells matching {GOLDENS}/{name}; review the change and run `cargo xtask theme-grids --write`",
                rate / MILLIS_PER_PERCENT,
                rate % MILLIS_PER_PERCENT
            ));
        }
    }
    Ok(refusals)
}

/// The palette's colors by lowercase literal, each with the names that hold it.
///
/// # Errors
///
/// Returns an error for a missing palette or an invalid color.
pub fn palette(data: &Value) -> Result<BTreeMap<String, Vec<String>>> {
    let mut colors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in data["palette"]
        .as_object()
        .context("the data holds no palette")?
    {
        let literal = value
            .as_str()
            .with_context(|| format!("palette.{name} is no string"))?
            .to_ascii_lowercase();
        contrast_rules::parse(literal.as_bytes())
            .with_context(|| format!("palette.{name} is no #rrggbb color"))?;
        colors.entry(literal).or_default().push(name.clone());
    }
    Ok(colors)
}

/// Every `#rrggbb` literal in a text that no longer word continues, in lowercase.
#[must_use]
pub fn literals(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut found = BTreeSet::new();
    for (index, _) in text.match_indices('#') {
        let digits = bytes.get(index + 1..index + 1 + HEX_DIGITS);
        let after = bytes.get(index + 1 + HEX_DIGITS);
        let before = index.checked_sub(1).and_then(|before| bytes.get(before));
        if let Some(digits) = digits
            && digits.iter().all(u8::is_ascii_hexdigit)
            && after.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
            && before.is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'&')
        {
            found.insert(
                String::from_utf8_lossy(&bytes[index..index + 1 + HEX_DIGITS]).to_ascii_lowercase(),
            );
        }
    }
    found
}

/// Refusals for every rendered color literal outside the palette.
#[must_use]
pub fn strays(profile: &str, dump: &Value, palette: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut refusals = Vec::new();
    for (target, entry) in dump.as_object().into_iter().flatten() {
        let Some(contents) = entry["contents"].as_str() else {
            continue;
        };
        for literal in literals(contents) {
            if !palette.contains_key(&literal) {
                refusals.push(format!(
                    "the {profile} profile renders {literal} in {target}, which no palette entry in home/.chezmoidata/shared.toml holds; name a palette color instead"
                ));
            }
        }
    }
    refusals
}

/// The contrast of every declared pair, lowest first, as its foreground, background, and ratio in thousandths.
///
/// # Errors
///
/// Returns an error when a pair names a color the palette doesn't hold.
pub fn contrasts(data: &Value) -> Result<Vec<(String, String, u64, bool)>> {
    let color = |name: &str| -> Result<Rgb> {
        data["palette"][name]
            .as_str()
            .and_then(|literal| contrast_rules::parse(literal.as_bytes()))
            .with_context(|| {
                format!("a contrast pair names palette.{name}, which the palette doesn't hold")
            })
    };
    let mut measured = Vec::new();
    for pair in data["contrast"].as_array().into_iter().flatten() {
        let foreground = pair["foreground"].as_str().unwrap_or_default();
        let background = pair["background"].as_str().unwrap_or_default();
        let (text, ground) = (color(foreground)?, color(background)?);
        measured.push((
            foreground.to_owned(),
            background.to_owned(),
            contrast_rules::ratio_millis(text, ground),
            contrast_rules::legible(text, ground),
        ));
    }
    measured.sort_by_key(|(_, _, ratio, _)| *ratio);
    Ok(measured)
}

/// Refusals for every declared pair below 4.5:1, and the lowest pair.
///
/// # Errors
///
/// Returns an error when the data declares no pair or a pair names a color the palette doesn't hold.
pub fn check_contrast(data: &Value) -> Result<(Vec<String>, String)> {
    let measured = contrasts(data)?;
    let (foreground, background, ratio, _) = measured.first().context(
        "the data declares no contrast pair, so nothing holds the palette to legible text",
    )?;
    let minimum = format!(
        "{foreground} on {background} at {}.{:03}:1",
        ratio / MILLIS,
        ratio % MILLIS
    );
    let refusals = measured
        .iter()
        .filter(|(_, _, _, legible)| !legible)
        .map(|(foreground, background, ratio, _)| {
            format!(
                "palette.{foreground} on palette.{background} reaches {}.{:03}:1, below the 4.5:1 that text needs",
                ratio / MILLIS,
                ratio % MILLIS
            )
        })
        .collect();
    Ok((refusals, minimum))
}

#[cfg(test)]
mod terminal_tests {
    use super::*;

    const JQ_EXECUTABLE: &str = "jq";
    const TRANSPORT_CONFIG: &str = "owned configuration with ' quotes\nand a newline\n";

    #[test]
    #[cfg(windows)]
    fn the_wsl_fzf_transport_refuses_an_empty_or_missing_distribution() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("workspace root")?;
        let empty = wsl_fzf_grid(root, std::ffi::OsStr::new(""), TRANSPORT_CONFIG).unwrap_err();
        assert!(format!("{empty:#}").contains(FZF_WSL_DISTRIBUTION));
        let missing = format!("dotfiles-fzf-missing-{}", std::process::id());
        let failure =
            wsl_fzf_grid(root, std::ffi::OsStr::new(&missing), TRANSPORT_CONFIG).unwrap_err();
        if let Some(cause) = failure.downcast_ref::<std::io::Error>() {
            assert_eq!(cause.kind(), std::io::ErrorKind::NotFound, "{failure:#}");
            assert!(format!("{failure:#}").contains("start the Linux fzf renderer"));
            return Ok(());
        }
        assert!(
            format!("{failure:#}").contains("renderer failed"),
            "{failure:#}"
        );
        Ok(())
    }

    #[test]
    fn the_fzf_transport_preserves_input_and_refuses_missing_malformed_or_failed_renderers()
    -> Result<()> {
        let scope = tempfile::tempdir()?;
        let missing = platform::process::command(scope.path().join("missing-fzf-renderer"));
        let failure = transported_fzf_grid(missing, TRANSPORT_CONFIG).unwrap_err();
        assert!(format!("{failure:#}").contains("start the Linux fzf renderer"));
        for (filter, expected) in [
            ("\"malformed\"", "returned no cell grid"),
            ("[1]", "returned no cell grid"),
            ("error(\"owned renderer refusal\")", "renderer failed"),
        ] {
            let mut command = crate::tool::external(JQ_EXECUTABLE);
            command.args(["--raw-input", "--slurp", "--raw-output", filter]);
            let failure = transported_fzf_grid(command, TRANSPORT_CONFIG).unwrap_err();
            assert!(format!("{failure:#}").contains(expected), "{failure:#}");
        }
        let mut command = crate::tool::external(JQ_EXECUTABLE);
        command.args([
            "--raw-input",
            "--slurp",
            "--arg",
            "expected",
            TRANSPORT_CONFIG,
            "if . == $expected then [\"owned cell\"] else error(\"input changed\") end",
        ]);
        assert_eq!(
            transported_fzf_grid(command, TRANSPORT_CONFIG)?,
            ["owned cell"]
        );
        Ok(())
    }
}
