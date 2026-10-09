//! Every theme renders from the one palette, and every pair a theme draws text with stays legible.

use rules::contrast_rules::LINEAR;
use serde_json::json;
use std::path::Path;
use xtask::theme::{check_contrast, contrasts, literals, palette, strays};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
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
