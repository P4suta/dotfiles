//! Black-and-white D2 diagrams that personal PR and issue bodies show from the repository's `diagrams` branch.

use crate::figure;
use anyhow::{Context, Result, ensure};
use pulldown_cmark::{Event, Tag, TagEnd};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// The branch of a personal repository that holds each rendered diagram beside its full source.
const BRANCH: &str = "diagrams";
/// The extension of a stored diagram image.
const IMAGE_EXTENSION: &str = ".svg";
/// The hexadecimal digits of a diagram name: a SHA-256 digest of its full source.
const NAME_DIGITS: usize = 64;

/// The configuration that starts every source: the layered layout of the Eclipse Layout Kernel and a theme whose every color is black or white.
const HEADER: &str = r##"vars: {
  d2-config: {
    layout-engine: elk
    pad: 40
    theme-overrides: {
      N1: "#000000"
      N2: "#000000"
      N3: "#000000"
      N4: "#FFFFFF"
      N5: "#FFFFFF"
      N6: "#FFFFFF"
      N7: "#FFFFFF"
      B1: "#000000"
      B2: "#000000"
      B3: "#FFFFFF"
      B4: "#FFFFFF"
      B5: "#FFFFFF"
      B6: "#FFFFFF"
      AA2: "#000000"
      AA4: "#FFFFFF"
      AA5: "#FFFFFF"
      AB4: "#FFFFFF"
      AB5: "#FFFFFF"
    }
  }
}
"##;

/// The paint values a source or a rendered image may use.
const PAINTS: [&str; 8] = [
    "#000000",
    "#ffffff",
    "#000",
    "#fff",
    "black",
    "white",
    "none",
    "transparent",
];
/// The D2 style keys that choose a color.
const PAINT_KEYS: [&str; 3] = ["fill", "stroke", "font-color"];
/// The D2 keys that replace the monochrome configuration or add transparency, effects, or foreign imagery.
const REFUSED_KEYS: [&str; 7] = [
    "d2-config",
    "opacity",
    "shadow",
    "fill-pattern",
    "3d",
    "animated",
    "icon",
];
/// The SVG attributes and declarations that choose a color.
const PAINT_ATTRIBUTES: [&str; 5] = ["fill", "stroke", "stop-color", "flood-color", "color"];
/// The SVG attributes and declarations that set transparency.
const OPACITY_ATTRIBUTES: [&str; 3] = ["opacity", "fill-opacity", "stroke-opacity"];
/// The SVG elements that paint outside the theme.
const REFUSED_ELEMENTS: [&str; 5] = [
    "linearGradient",
    "radialGradient",
    "filter",
    "pattern",
    "image",
];
/// The SVG class that D2 gives a translucent blended shape.
const BLEND_CLASS: &str = "blend";
/// The longest hexadecimal color: `#RRGGBBAA`.
const HEX_COLOR_DIGITS: usize = 8;
/// The shortest hexadecimal color: `#RGB`.
const SHORT_HEX_COLOR_DIGITS: usize = 3;

fn paint_allowed(value: &str) -> bool {
    let value = value.trim().trim_matches('"').trim().to_ascii_lowercase();
    PAINTS.contains(&value.as_str())
}

/// The statements of a D2 source outside strings and comments, which braces, semicolons, and line breaks separate.
fn statements(source: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut escaped, mut comment) = (false, false, false);
    for character in source.chars() {
        if comment {
            if character == '\n' {
                comment = false;
                statements.push(std::mem::take(&mut current));
            }
            continue;
        }
        if quoted {
            current.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => {
                quoted = true;
                current.push(character);
            }
            '#' if current.trim().is_empty() || current.ends_with(char::is_whitespace) => {
                comment = true;
            }
            '{' | '}' | ';' | '\n' => statements.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    statements.push(current);
    statements
}

/// The statements of a source that would leave black and white, each with its reason.
fn source_gaps(source: &str) -> Vec<String> {
    let mut gaps = Vec::new();
    for statement in statements(source) {
        let statement = statement.trim();
        if statement.starts_with("...@") {
            gaps.push(format!("{statement:?}: an import can bring its own colors"));
            continue;
        }
        let Some((path, value)) = statement.split_once(':') else {
            continue;
        };
        let key = path.rsplit('.').next().unwrap_or_default().trim();
        if REFUSED_KEYS.contains(&key) {
            gaps.push(format!(
                "{statement:?}: `{key}` leaves the monochrome theme"
            ));
        } else if PAINT_KEYS.contains(&key) && !paint_allowed(value) {
            gaps.push(format!("{statement:?}: use \"#000000\" or \"#FFFFFF\""));
        } else if value.trim().starts_with('@') {
            gaps.push(format!("{statement:?}: an import can bring its own colors"));
        }
    }
    gaps
}

/// The `name="value"` attributes of an SVG tag's interior.
fn attributes(interior: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    let mut rest = interior;
    while let Some((before, after)) = rest.split_once("=\"") {
        let name = before
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default();
        let Some((value, tail)) = after.split_once('"') else {
            break;
        };
        found.push((name, value));
        rest = tail;
    }
    found
}

fn opaque(value: &str) -> bool {
    value
        .trim()
        .parse::<f64>()
        .is_ok_and(|opacity| opacity >= 1.0)
}

/// The paint of one SVG attribute or style declaration, as a gap when it leaves black and white.
fn paint_gap(property: &str, value: &str) -> Option<String> {
    let property = property.trim();
    let value = value.trim();
    if PAINT_ATTRIBUTES.contains(&property) && !paint_allowed(value) && !value.starts_with("url(") {
        return Some(format!("{property}={value:?}"));
    }
    if OPACITY_ATTRIBUTES.contains(&property) && !opaque(value) {
        return Some(format!("{property}={value:?}"));
    }
    matches!(property, "mix-blend-mode" | "filter").then(|| format!("{property}={value:?}"))
}

/// The hexadecimal colors of a style sheet other than black and white.
fn sheet_gaps(sheet: &str) -> Vec<String> {
    let mut gaps = Vec::new();
    for (index, _) in sheet.match_indices('#') {
        let digits = sheet
            .get(index + 1..)
            .unwrap_or_default()
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>();
        let color = format!("#{digits}");
        if (SHORT_HEX_COLOR_DIGITS..=HEX_COLOR_DIGITS).contains(&digits.len())
            && digits.chars().all(|digit| digit.is_ascii_hexdigit())
            && !paint_allowed(&color)
        {
            gaps.push(format!("style sheet color {color}"));
        }
    }
    for function in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if sheet.contains(function) {
            gaps.push(format!("style sheet color function {function}"));
        }
    }
    gaps
}

/// The paint of a rendered SVG that leaves black and white.
fn svg_gaps(svg: &str) -> Vec<String> {
    let mut gaps = Vec::new();
    for tag in svg.split('<').skip(1) {
        let interior = tag.split('>').next().unwrap_or_default();
        if interior.starts_with(['!', '?', '/']) {
            continue;
        }
        let name = interior
            .split(|character: char| character.is_whitespace() || character == '/')
            .next()
            .unwrap_or_default();
        if REFUSED_ELEMENTS.contains(&name) {
            gaps.push(format!("element <{name}>"));
        }
        for (attribute, value) in attributes(interior) {
            if attribute == "class" && value.split_whitespace().any(|class| class == BLEND_CLASS) {
                gaps.push(format!("class {BLEND_CLASS:?} on <{name}>"));
            } else if attribute == "style" {
                gaps.extend(value.split(';').filter_map(|declaration| {
                    let (property, value) = declaration.split_once(':')?;
                    paint_gap(property, value)
                }));
            } else if let Some(gap) = paint_gap(attribute, value) {
                gaps.push(gap);
            }
        }
    }
    for block in svg.split("<style").skip(1) {
        let sheet = block.split("</style>").next().unwrap_or_default();
        gaps.extend(sheet_gaps(sheet));
    }
    gaps
}

/// Whether an image address names a stored diagram of the repository.
fn stored_image(repository: &str, address: &str) -> bool {
    figure::stored_name(repository, BRANCH, address)
        .and_then(|file| file.strip_suffix(IMAGE_EXTENSION))
        .is_some_and(|name| {
            name.len() == NAME_DIGITS
                && name
                    .chars()
                    .all(|digit| digit.is_ascii_digit() || ('a'..='f').contains(&digit))
        })
}

/// Whether a body shows, outside comments and code, a described diagram that the repository stores.
pub(crate) fn shown(repository: &str, body: &str) -> bool {
    let mut image: Option<(bool, String)> = None;
    for event in pulldown_cmark::Parser::new(body) {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((stored_image(repository, &dest_url), String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, description)) = image.as_mut() {
                    description.push_str(&text);
                }
            }
            Event::End(TagEnd::Image)
                if image.take().is_some_and(|(stored, description)| {
                    stored && !description.trim().is_empty()
                }) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// The refusal for a personal body without a stored diagram.
pub(crate) fn missing(repository: &str) -> crate::Refusal {
    crate::Refusal::new(
        "pr.diagram",
        "a PR or issue in a personal repository shows its change as a black-and-white D2 diagram",
        format!(
            "pr-workflow diagram --repo {repository} --source FILE.d2 --alt TEXT, then paste the printed image line into the body"
        ),
    )
    .evidence(format!(
        "the body shows no described image from {}",
        figure::prefix(repository, BRANCH)
    ))
}

/// Renders a source in black and white, stores it on the repository's `diagrams` branch, and returns the body's image line.
///
/// # Errors
/// Returns a refusal for a source or image that leaves black and white, and an error when D2 or GitHub fails.
pub(crate) fn publish(repository: &str, source_file: &Path, alt: &str) -> Result<String> {
    figure::check_alt(alt)?;
    let source = fs::read_to_string(source_file).context("read the UTF-8 D2 source")?;
    let gaps = source_gaps(&source);
    if !gaps.is_empty() {
        let refusal = crate::Refusal::new(
            "pr.diagram",
            "the D2 source sets configuration, colors, or effects that leave black and white",
            "remove each named statement; the command adds the monochrome theme itself",
        );
        return Err(gaps
            .into_iter()
            .fold(refusal, crate::Refusal::evidence)
            .into());
    }
    let full = format!("{HEADER}\n{source}");
    let name = crate::build_inputs::hex(&Sha256::digest(full.as_bytes()));
    let directory = tempfile::tempdir()?;
    let input = directory.path().join(format!("{name}.d2"));
    let output = directory.path().join(format!("{name}.svg"));
    fs::write(&input, &full)?;
    let rendered = crate::tool::owner_d2()?
        .arg(&input)
        .arg(&output)
        .output()
        .context("start D2")?;
    ensure!(
        rendered.status.success(),
        "D2 failed: {}",
        String::from_utf8_lossy(&rendered.stderr).trim()
    );
    let svg = fs::read_to_string(&output).context("read the rendered SVG")?;
    let gaps = svg_gaps(&svg);
    if !gaps.is_empty() {
        let refusal = crate::Refusal::new(
            "pr.diagram",
            "the rendered image paints outside black and white",
            "remove the shapes or styles that produce the named paint",
        );
        return Err(gaps
            .into_iter()
            .fold(refusal, crate::Refusal::evidence)
            .into());
    }
    let image = format!("{name}.svg");
    figure::store(
        repository,
        BRANCH,
        &[
            figure::File {
                name: image.clone(),
                content: figure::Content::Text(&svg),
            },
            figure::File {
                name: format!("{name}.d2"),
                content: figure::Content::Text(&full),
            },
        ],
    )?;
    Ok(figure::image_line(repository, BRANCH, &image, alt))
}

#[cfg(test)]
mod tests {
    use super::{HEADER, shown, source_gaps, svg_gaps};

    const NAME: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn line(repository: &str, alt: &str) -> String {
        format!("![{alt}](https://github.com/{repository}/blob/diagrams/{NAME}.svg?raw=true)")
    }

    #[test]
    fn a_body_shows_a_described_diagram_that_its_repository_stores() {
        let body = format!("## Why\n\n{}\n", line("P4suta/project", "Flow"));
        assert!(shown("P4suta/project", &body));
        assert!(shown("p4suta/PROJECT", &body));
    }

    #[test]
    fn foreign_undescribed_hidden_or_malformed_images_show_nothing() {
        let foreign = line("someone/else", "Flow");
        let bare = line("P4suta/project", "");
        let commented = format!("<!-- {} -->", line("P4suta/project", "Flow"));
        let fenced = format!("```\n{}\n```", line("P4suta/project", "Flow"));
        let short = line("P4suta/project", "Flow").replace(NAME, "0123");
        let upper = line("P4suta/project", "Flow").replace(NAME, &NAME.to_uppercase());
        let png = line("P4suta/project", "Flow").replace(".svg", ".png");
        let link = line("P4suta/project", "Flow").replacen('!', "", 1);
        for body in [foreign, bare, commented, fenced, short, upper, png, link] {
            assert!(!shown("P4suta/project", &body), "{body}");
        }
    }

    /// D2 0.9.0 paints text, strokes, and connections with these theme colors, in `GetStroke` and `text.GetColor`.
    /// <https://github.com/terrastruct/d2/blob/v0.9.0/d2graph/d2graph.go>
    const INK: [&str; 6] = ["N1", "N2", "N3", "B1", "B2", "AA2"];
    /// D2 0.9.0 fills shapes and the canvas with these theme colors in `GetFill`, such as N4 for a diamond.
    const PAPER: [&str; 12] = [
        "N4", "N5", "N6", "N7", "B3", "B4", "B5", "B6", "AA4", "AA5", "AB4", "AB5",
    ];

    #[test]
    fn the_theme_inks_text_and_strokes_black_and_fills_shapes_white() {
        let overrides = HEADER
            .lines()
            .filter_map(|line| line.trim().split_once(": \"#"))
            .map(|(key, color)| (key, color.trim_end_matches('"')))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(overrides.len(), INK.len() + PAPER.len());
        for key in INK {
            assert_eq!(overrides.get(key), Some(&"000000"), "{key}");
        }
        for key in PAPER {
            assert_eq!(overrides.get(key), Some(&"FFFFFF"), "{key}");
        }
    }

    #[test]
    fn a_monochrome_source_passes_and_its_header_names_only_black_and_white() {
        let source = "a: Start {style.fill: \"#FFFFFF\"; style.stroke: black}\nb: \"Label # not a comment: fill: red\"\na -> b: next # fill: red\n";
        assert_eq!(source_gaps(source), Vec::<String>::new());
        assert_eq!(
            source_gaps(HEADER),
            vec!["\"d2-config:\": `d2-config` leaves the monochrome theme".to_owned()]
        );
    }

    #[test]
    fn colors_effects_imports_and_configuration_are_refused() {
        for source in [
            "a.style.fill: \"#FF0000\"",
            "a: {style: {stroke: red}}",
            "a.style.font-color: \"${accent}\"",
            "a.style.opacity: 0.4",
            "a.style.shadow: true",
            "a.style.fill-pattern: dots",
            "a.style.3d: true",
            "a -> b: {style.animated: true}",
            "a.icon: https://example.com/a.svg",
            "vars: {d2-config: {theme-id: 4}}",
            "...@colors",
            "a: @colors",
        ] {
            assert_eq!(source_gaps(source).len(), 1, "{source}");
        }
    }

    #[test]
    fn a_monochrome_image_passes() {
        let svg = r##"<svg><style type="text/css"><![CDATA[.blend{mix-blend-mode:multiply;opacity:0.5}.fill-N1{fill:#000000;}.md{--color-fg-default:#FFFFFF}]]></style><mask id="d2-1"></mask><rect fill="white" stroke="#000000" style="stroke-width:2;" opacity="1"></rect><text fill="black">PR #600 and #abc</text><path fill="none" mask="url(#d2-1)"/></svg>"##;
        assert_eq!(svg_gaps(svg), Vec::<String>::new());
    }

    #[test]
    fn colored_translucent_blended_and_foreign_paint_is_refused() {
        for svg in [
            r##"<svg><rect fill="#FF0000"/></svg>"##,
            r#"<svg><rect stroke="red"/></svg>"#,
            r#"<svg><rect opacity="0.5"/></svg>"#,
            r#"<svg><rect style="fill:#123456"/></svg>"#,
            r#"<svg><rect style="filter:blur(2px)"/></svg>"#,
            r#"<svg><g class="shape blend"/></svg>"#,
            r#"<svg><linearGradient id="g"/></svg>"#,
            r#"<svg><image href="a.png"/></svg>"#,
            r"<svg><style>.a{fill:#EDF0FD}</style></svg>",
            r"<svg><style>.a{fill:rgba(0,0,0,0.5)}</style></svg>",
        ] {
            assert!(!svg_gaps(svg).is_empty(), "{svg}");
        }
    }
}
