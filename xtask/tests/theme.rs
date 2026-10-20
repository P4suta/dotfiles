//! Every theme renders from the one palette, and every pair a theme draws text with stays legible.

use anyhow::Result;
use rules::contrast_rules::LINEAR;
use serde_json::json;
use std::fs;
use std::path::Path;
use xtask::theme::{check_contrast, contrasts, literals, palette, strays};

#[path = "../../crates/dotguard/tests/support/fixture_git.rs"]
mod fixture_git;

const GIT_CONTEXT_FIXTURE: &str = "DOTFILES_THEME_GIT_CONTEXT_FIXTURE";
const GIT_CONTEXT_TEST: &str = "a_prompt_renders_its_fixture_repository_in_a_foreign_git_context";
const CALLER_BRANCH: &str = "caller-branch";
const FZF_TARGET: &str = ".config/fzf/fzfrc";
const RECOLORED_FZF_FOREGROUND: &str = "#ff0000";
const RENDER_ENVIRONMENT_FIXTURE: &str = "DOTFILES_THEME_RENDER_ENVIRONMENT_FIXTURE";
const RENDER_ENVIRONMENT_TEST: &str = "the_grid_renderers_ignore_the_callers_rendering_environment";

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn rendered_mac_profile() -> Result<serde_json::Value> {
    let scope = tempfile::tempdir()?;
    let home = scope.path().join("home");
    fs::create_dir(&home)?;
    let config = scope.path().join("chezmoi.json");
    let data = xtask::profiles::fixture(xtask::profile_rules::Profile::Mac, root(), &home)?;
    fs::write(&config, serde_json::to_vec(&json!({"data": data}))?)?;
    let rendered = xtask::profiles::chezmoi(root(), scope.path(), &config, &home)
        .arg("--override-data")
        .arg(data.to_string())
        .args(["dump", "--include", "files", "--format", "json"])
        .output()?;
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    Ok(serde_json::from_slice(&rendered.stdout)?)
}

#[test]
fn the_committed_linearization_table_follows_the_srgb_transfer_function() {
    for (channel, entry) in LINEAR.iter().enumerate() {
        let encoded = f64::from(u8::try_from(channel).unwrap()) / 255.0;
        let linear = if encoded <= 0.040_45 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        };
        assert_eq!(
            f64::from(*entry),
            (linear * 10_000_000.0).round(),
            "channel {channel}"
        );
    }
}

#[test]
fn every_declared_pair_of_the_repository_keeps_four_and_a_half_to_one() {
    let data = xtask::profiles::source_data(root()).unwrap();
    let (refusals, minimum) = check_contrast(&data).unwrap();
    for (foreground, background, ratio, _) in contrasts(&data).unwrap() {
        println!("{foreground} on {background}: {ratio}");
    }
    assert!(refusals.is_empty(), "{refusals:#?}");
    println!("the lowest pair is {minimum}");
}

#[test]
fn a_grid_records_each_cell_and_one_changed_color_breaks_the_full_match() {
    let golden = xtask::theme::grid(b"\x1b[38;2;192;202;245mab\x1b[0m\nc");
    assert_eq!(
        golden,
        [
            "00,00\ta\t#c0caf5\t-\t",
            "00,01\tb\t#c0caf5\t-\t",
            "01,00\tc\t-\t-\t"
        ]
    );
    assert_eq!(xtask::theme::match_millis(&golden, &golden), 1000);
    assert_eq!(xtask::theme::match_millis(&[], &[]), 0);
    let recolored = xtask::theme::grid(b"\x1b[38;2;192;202;246mab\x1b[0m\nc");
    assert!(xtask::theme::match_millis(&golden, &recolored) < 1000);
}

#[test]
fn the_rendered_fzf_grid_matches_and_rejects_a_changed_foreground() -> Result<()> {
    let dump = rendered_mac_profile()?;
    let config = dump[FZF_TARGET]["contents"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the rendered profile has no fzf configuration"))?;
    let candidate = json!({FZF_TARGET: {"contents": config}});
    assert_eq!(
        xtask::theme::check_grids(root(), "mac", &candidate, false)?,
        Vec::<String>::new()
    );
    let source = xtask::profiles::source_data(root())?;
    let foreground = source["palette"]["fg_dim"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("the palette has no fzf foreground"))?;
    let recolored = config.replace(foreground, RECOLORED_FZF_FOREGROUND);
    assert_ne!(
        config, recolored,
        "the control must change the fzf foreground"
    );
    let changed = json!({FZF_TARGET: {"contents": recolored}});
    let refusals = xtask::theme::check_grids(root(), "mac", &changed, false)?;
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(refusals[0].contains("mac-fzf.grid"), "{refusals:?}");
    Ok(())
}

#[test]
fn the_grid_renderers_ignore_the_callers_rendering_environment() -> Result<()> {
    if std::env::var_os(RENDER_ENVIRONMENT_FIXTURE).is_some() {
        let dump = rendered_mac_profile()?;
        let refusals = xtask::theme::check_grids(root(), "mac", &dump, false)?;
        assert!(refusals.is_empty(), "{refusals:#?}");
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let options = scope.path().join("caller.fzfrc");
    fs::write(&options, "--color=bw\n--prompt=caller\n")?;
    let output = platform::process::command(std::env::current_exe()?)
        .args([RENDER_ENVIRONMENT_TEST, "--exact", "--nocapture"])
        .env(RENDER_ENVIRONMENT_FIXTURE, "1")
        .envs([
            ("NO_COLOR", "1"),
            ("FORCE_COLOR", "1"),
            ("CLICOLOR", "0"),
            ("CLICOLOR_FORCE", "1"),
            ("COLORTERM", "caller"),
            ("TERM", "dumb"),
            ("COLUMNS", "10"),
            ("LINES", "5"),
            ("LANG", "C"),
            ("LC_ALL", "C"),
            ("LC_CTYPE", "C"),
            ("LC_COLLATE", "C"),
            ("LC_MESSAGES", "C"),
            ("FZF_DEFAULT_OPTS", "--color=bw --prompt=caller"),
            ("FZF_DEFAULT_COMMAND", "exit 1"),
            ("BAT_THEME", "TwoDark"),
            ("BAT_STYLE", "numbers"),
            ("STARSHIP_SHELL", "tcsh"),
        ])
        .env("FZF_DEFAULT_OPTS_FILE", &options)
        .output()?;
    assert!(
        output.status.success(),
        "the grids inherited the caller's rendering environment:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn a_prompt_renders_its_fixture_repository_in_a_foreign_git_context() -> Result<()> {
    if std::env::var_os(GIT_CONTEXT_FIXTURE).is_some() {
        let dump = json!({
            ".config/starship.toml": {"contents": r#"
format = "$directory|$git_branch"
add_newline = false
[directory]
format = "$path"
[git_branch]
format = "$branch"
"#}
        });
        let rendered = xtask::theme::grids(root(), "fixture", &dump)?;
        assert_eq!(
            rendered,
            [(
                "fixture-starship.grid".to_owned(),
                xtask::theme::grid(b"project|main")
            )]
        );
        return Ok(());
    }
    let scope = tempfile::tempdir()?;
    let caller = scope.path().join("caller");
    fs::create_dir(&caller)?;
    let status = fixture_git::command("git", &scope.path().join("global.gitconfig"))
        .current_dir(&caller)
        .args(["init", "--quiet", "--initial-branch", CALLER_BRANCH])
        .status()?;
    assert!(status.success(), "initialize the caller repository");
    let output = fixture_git::command(
        std::env::current_exe()?,
        &scope.path().join("global.gitconfig"),
    )
    .current_dir(scope.path())
    .args([GIT_CONTEXT_TEST, "--exact", "--nocapture"])
    .env(GIT_CONTEXT_FIXTURE, "1")
    .env("GIT_DIR", caller.join(".git"))
    .env("GIT_WORK_TREE", &caller)
    .output()?;
    assert!(
        output.status.success(),
        "the prompt inherited the caller repository:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn a_color_outside_the_palette_or_an_illegible_pair_fails() {
    let data = json!({
        "palette": {"bg": "#1a1b26", "fg": "#c0caf5", "comment": "#565f89"},
        "contrast": [
            {"foreground": "fg", "background": "bg", "used": "body"},
            {"foreground": "comment", "background": "bg", "used": "comments"}
        ]
    });
    let colors = palette(&data).unwrap();
    let dump = json!({
        "kitty.conf": {"contents": "foreground #C0CAF5\nbackground #1a1b26\nurl #73daca\n"},
        "notes.md": {"contents": "issue #123 and &#123456; and #1a1b26ff stay out"}
    });
    let refusals = strays("linux", &dump, &colors);
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(
        refusals[0].contains("#73daca")
            && refusals[0].contains("kitty.conf")
            && refusals[0].contains("linux")
    );
    let (refusals, minimum) = check_contrast(&data).unwrap();
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(minimum.starts_with("comment on bg"), "{minimum}");
    assert!(literals("color: #ABCDEF;").contains("#abcdef"));
    let missing = json!({"palette": {"bg": "#1a1b26"}, "contrast": [{"foreground": "fg", "background": "bg", "used": "x"}]});
    assert!(check_contrast(&missing).is_err());
}
