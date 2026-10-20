//! The data files under `home/.chezmoidata/`, which chezmoi merges into one value before it renders a template.
//! Each file holds part of [`Data`], so every field stays optional here, and an unknown key fails to parse with its name.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Every key a data file may hold.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Data {
    pub coderabbit: Option<Coderabbit>,
    pub domyjob: Option<Domyjob>,
    pub herdr_agent: Option<HerdrAgent>,
    /// The mise release that every host installs before any other step.
    pub mise: Option<MiseRelease>,
    pub ocaml: Option<Ocaml>,
    /// The one palette every themed target renders its colors from.
    pub palette: Option<Palette>,
    /// The foreground and background palette names that some theme draws text with, each of which keeps a text contrast of 4.5:1.
    pub contrast: Option<Vec<ContrastPair>>,
    pub platforms: Option<Platforms>,
    /// Named sets of profiles.
    pub profiles: Option<BTreeMap<String, Vec<String>>>,
    /// Targets that only the profiles of the named set receive.
    pub targets: Option<BTreeMap<String, String>>,
    /// Retired targets that the profiles of the named set remove.
    pub retired: Option<BTreeMap<String, String>>,
    /// Setup launchers that only the profiles of the named set run, which `just gen` writes from the setup steps.
    pub scripts: Option<BTreeMap<String, String>>,
    /// The modern command-line tools that replace legacy ones for agents, pinned on every profile.
    pub modern: Option<Modern>,
    /// The interactive behavior every shell a profile configures renders in its own syntax.
    pub shell: Option<Shell>,
}

/// The shortcuts, functions, history, line editing keys, and pager and editor variables of every interactive shell.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Shell {
    /// Commands each shell keeps in its history, which PowerShell caps at its own limit.
    pub history_size: u32,
    pub edit_mode: EditMode,
    /// Editors in order of preference, each with its options, of which a host takes the first it finds.
    pub editors: Vec<String>,
    pub fallback_editor: FallbackEditor,
    pub pager: String,
    /// The `LESS` options for the programs that still page through less.
    pub less: String,
    pub keys: Vec<ShellKey>,
    pub aliases: Vec<ShellAlias>,
    pub functions: Vec<ShellFunction>,
}

/// The line editor mode of every shell.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EditMode {
    #[default]
    Vi,
    Emacs,
}

/// The editor of a host that has none of the preferred editors, for each OS family.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackEditor {
    pub unix: String,
    pub windows: String,
}

/// One insert-mode key and the action every shell binds it to.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellKey {
    pub chord: Chord,
    pub action: KeyAction,
}

/// A key chord that every shell's line editor can bind.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, PartialEq, Eq, Serialize)]
pub enum Chord {
    #[default]
    #[serde(rename = "alt+left")]
    AltLeft,
    #[serde(rename = "alt+right")]
    AltRight,
    #[serde(rename = "alt+b")]
    AltB,
    #[serde(rename = "alt+f")]
    AltF,
    #[serde(rename = "alt+backspace")]
    AltBackspace,
    #[serde(rename = "ctrl+left")]
    CtrlLeft,
    #[serde(rename = "ctrl+right")]
    CtrlRight,
    #[serde(rename = "ctrl+a")]
    CtrlA,
    #[serde(rename = "ctrl+e")]
    CtrlE,
    #[serde(rename = "ctrl+k")]
    CtrlK,
    #[serde(rename = "ctrl+u")]
    CtrlU,
    #[serde(rename = "ctrl+w")]
    CtrlW,
}

/// A line editing action that every shell's line editor provides.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    #[default]
    WordLeft,
    WordRight,
    KillWord,
    KillToStart,
    KillToEnd,
    LineStart,
    LineEnd,
}

/// A shortcut that stands for one command and its options.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellAlias {
    pub name: String,
    pub command: String,
}

/// A function that each shell defines in its own syntax, because it changes the directory or asks for a choice.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellFunction {
    pub name: String,
    pub summary: String,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Domyjob {
    pub chat: Chat,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Chat {
    pub peers: Vec<String>,
}

/// The modern tool table: each preferred tool, the bootstrap scripts that keep legacy tools, and the candidates it leaves out.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Modern {
    pub tools: Vec<ModernTool>,
    #[serde(default)]
    pub exceptions: Vec<ModernException>,
    #[serde(default)]
    pub rejected: Vec<ModernRejected>,
}

/// One preferred tool and the legacy commands it replaces.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModernTool {
    /// The legacy commands the hooks refuse.
    pub legacy: Vec<String>,
    /// The flags that mark an invocation as legacy when the tool replaces only some uses, such as `-l` for `wc`.
    #[serde(default)]
    pub when: Vec<String>,
    /// Whether the legacy command stays admitted when it reads only standard input, such as `wc -l` in a pipeline.
    #[serde(default)]
    pub stream: bool,
    /// The command the next action names.
    pub preferred: String,
    /// The mise tool that installs the preferred command on every profile.
    pub tool: String,
    /// The pinned version.
    pub version: String,
    pub reason: String,
    /// When the legacy tool stays correct.
    pub keep: String,
    /// The next action, where `{args}` stands for the legacy command's arguments, and `grep` or `find` asks for a flag translation.
    pub next: String,
}

/// A script that runs before mise provisions the preferred tools, so it keeps the legacy ones.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModernException {
    pub path: String,
    pub reason: String,
}

/// A candidate the table leaves out, with the condition that would admit it.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModernRejected {
    pub legacy: String,
    pub preferred: String,
    pub reason: String,
    pub revisit: String,
}

/// The Tokyo Night colors of every themed tool, each a `#rrggbb` literal.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub bg: Option<Hex>,
    pub bg_dim: Option<Hex>,
    pub bg_highlight: Option<Hex>,
    pub bg_panel: Option<Hex>,
    pub blue: Option<Hex>,
    pub blue1: Option<Hex>,
    pub bright_black: Option<Hex>,
    pub comment: Option<Hex>,
    pub cyan: Option<Hex>,
    pub diff_add: Option<Hex>,
    pub diff_add_gutter: Option<Hex>,
    pub diff_del: Option<Hex>,
    pub diff_del_gutter: Option<Hex>,
    pub fg: Option<Hex>,
    pub fg_dim: Option<Hex>,
    pub green: Option<Hex>,
    pub magenta: Option<Hex>,
    pub orange: Option<Hex>,
    pub purple: Option<Hex>,
    pub red: Option<Hex>,
    pub selection: Option<Hex>,
    pub surface: Option<Hex>,
    pub teal: Option<Hex>,
    pub yellow: Option<Hex>,
}

/// A `#rrggbb` color literal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hex(String);

impl JsonSchema for Hex {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Hex".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^#[0-9a-f]{6}$"
        })
    }
}

impl TryFrom<String> for Hex {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if rules::contrast_rules::parse(text.as_bytes()).is_some()
            && text == text.to_ascii_lowercase()
        {
            Ok(Self(text))
        } else {
            Err(format!("{text} is no lowercase #rrggbb color"))
        }
    }
}

impl From<Hex> for String {
    fn from(hex: Hex) -> Self {
        hex.0
    }
}

impl Hex {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Two palette names that a theme draws text with, the text's color on the background's.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContrastPair {
    pub foreground: String,
    pub background: String,
    /// Where a theme draws this pair.
    pub used: String,
}

/// A mise release and the SHA-256 of its asset for each platform, such as `linux-x64` or `windows-x64`.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MiseRelease {
    pub version: Option<String>,
    pub sha256: Option<BTreeMap<String, String>>,
}

/// The pinned `CodeRabbit` command line and its artifact for each host platform.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Coderabbit {
    pub version: Option<String>,
    pub platforms: Option<BTreeMap<String, Artifact>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub artifact: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HerdrAgent {
    pub public_keys: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ocaml {
    pub compiler: Option<String>,
    pub platform_tools: Option<Vec<String>>,
    pub switch: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Platforms {
    pub linux: Option<Linux>,
    pub mac: Option<Mac>,
    pub windows: Option<Windows>,
    pub wsl: Option<Wsl>,
}

/// The tools mise installs: `common` on every machine, and `personal` or `work` by the machine's role.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tools {
    pub common: Option<Vec<String>>,
    pub personal: Option<Vec<String>>,
    pub work: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mise {
    pub jobs: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Linux {
    pub desktop: Option<LinuxDesktop>,
    pub mise: Option<Mise>,
    pub nix: Option<Nix>,
    pub tool_versions: Option<BTreeMap<String, String>>,
    pub tools: Option<Tools>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinuxDesktop {
    pub keyboard: Option<LinuxKeyboard>,
    pub kitty_channel: Option<String>,
    pub nerd_fonts_version: Option<String>,
    pub opacity: Option<String>,
    pub zed_channel: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinuxKeyboard {
    pub delay: Option<u32>,
    pub repeat_interval: Option<u32>,
}

/// The packages the Nix profile installs, each with the mise tool that replaces it where Nix stays absent, or an empty string for none.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Nix {
    pub packages: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mac {
    pub brew: Option<Brew>,
    pub desktop: Option<MacDesktop>,
    pub macos: Option<Macos>,
    pub mise: Option<Mise>,
    pub racket: Option<Racket>,
    pub tool_versions: Option<BTreeMap<String, String>>,
    pub tools: Option<Tools>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Brew {
    pub casks: Option<Vec<String>>,
    pub formulae: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MacDesktop {
    pub blur: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<String>,
    pub opacity: Option<String>,
    pub zed_channel: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Macos {
    pub keyboard: Option<MacKeyboard>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MacKeyboard {
    pub initial_key_repeat: Option<u32>,
    pub key_repeat: Option<u32>,
    pub press_and_hold: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Racket {
    pub collections: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Windows {
    pub herdr: Option<Herdr>,
    pub keyboard: Option<WindowsKeyboard>,
    pub ocomment: Option<Ocomment>,
    pub scoop: Option<Scoop>,
    pub sshd: Option<Sshd>,
    pub terminal: Option<Terminal>,
    pub tools: Option<Tools>,
    pub winget: Option<Winget>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Herdr {
    pub channel: Option<String>,
    pub enabled: Option<bool>,
    pub expected_build_id: Option<String>,
    pub manifest_url: Option<String>,
    pub retain: Option<u32>,
    pub revision: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsKeyboard {
    pub keyboard_delay: Option<u32>,
    pub keyboard_speed: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ocomment {
    pub destination: Option<String>,
    pub enabled: Option<bool>,
    pub repo_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scoop {
    pub apps: Option<Vec<String>>,
    pub buckets: Option<Vec<String>>,
    pub installer_commit: Option<String>,
    pub installer_sha256: Option<String>,
    pub winget_duplicates: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sshd {
    pub authorize_signing_key: Option<bool>,
    pub default_shell: Option<String>,
    pub enabled: Option<bool>,
    pub firewall_profile: Option<String>,
    pub password_auth: Option<bool>,
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Terminal {
    pub font_family: Option<String>,
    pub font_size: Option<u32>,
    pub max_fps: Option<u32>,
    pub multiplexer: Option<String>,
    pub opacity: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Winget {
    pub apps: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Wsl {
    pub tools: Option<Tools>,
}

/// The directory of the data files, relative to the chezmoi source state.
pub const DIRECTORY: &str = ".chezmoidata";

/// Parses one data file, naming the file and the unknown or mistyped key when it fails.
///
/// # Errors
///
/// Returns the filename with the parser's message when the text holds invalid TOML, a key [`Data`] lacks, or a value of the wrong type.
pub fn parse(file: &str, text: &str) -> Result<Data, String> {
    toml::from_str(text).map_err(|error| format!("{file}: {error}"))
}

/// Every data file of `directory` merged into one value, the way chezmoi merges them, after each file parses as [`Data`].
///
/// # Errors
///
/// Returns a message naming the file when reading the directory or a file fails, or when a file fails to parse.
pub fn load(directory: &Path) -> Result<Value, String> {
    let mut files: Vec<_> = fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    files.sort();
    let mut merged = Value::Object(Map::new());
    for path in files {
        let name = format!(
            "{DIRECTORY}/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let text = fs::read_to_string(&path).map_err(|error| format!("{name}: {error}"))?;
        parse(&name, &text)?;
        let table: toml::Table =
            toml::from_str(&text).map_err(|error| format!("{name}: {error}"))?;
        let value = serde_json::to_value(table).map_err(|error| format!("{name}: {error}"))?;
        merge(&mut merged, value);
    }
    Ok(merged)
}

fn merge(target: &mut Value, value: Value) {
    match (target, value) {
        (Value::Object(target), Value::Object(value)) => {
            for (key, value) in value {
                match target.get_mut(&key) {
                    Some(current) => merge(current, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, value) => *target = value,
    }
}

/// The JSON schema of [`Data`], pretty-printed with a final newline.
#[must_use]
pub fn schema() -> String {
    let schema = schemars::schema_for!(Data);
    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_misspelled_key_fails_with_its_file_and_name() {
        let error = parse("tools.toml", "[platforms.linux.tool]\ncommon = []\n").unwrap_err();
        assert!(error.starts_with("tools.toml: "), "{error}");
        assert!(error.contains("unknown field `tool`"), "{error}");
    }

    #[test]
    fn a_value_of_the_wrong_type_fails() {
        assert!(parse("tools.toml", "[platforms.mac.mise]\njobs = \"eight\"\n").is_err());
    }

    #[test]
    fn the_palette_holds_only_its_named_lowercase_colors() {
        assert!(parse("shared.toml", "[palette]\nbg = \"#1a1b26\"\n").is_ok());
        for refused in [
            "[palette]\nbg = \"#1A1B26\"\n",
            "[palette]\nbg = \"navy\"\n",
            "[palette]\nbackground = \"#1a1b26\"\n",
        ] {
            assert!(parse("shared.toml", refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_file_may_hold_any_part_of_the_data() {
        let data = parse("shared.toml", "[ocaml]\nswitch = \"5.5.0\"\n").unwrap();
        assert_eq!(
            data.ocaml.and_then(|ocaml| ocaml.switch).as_deref(),
            Some("5.5.0")
        );
        assert_eq!(data.platforms, None);
    }

    #[test]
    fn the_files_merge_key_by_key_and_name_the_file_that_fails() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("a.toml"),
            "[platforms.mac.mise]\njobs = 8\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("b.toml"),
            "[platforms.mac.tools]\ncommon = [\"jq\"]\n",
        )
        .unwrap();
        fs::write(directory.path().join("notes.md"), "ignored").unwrap();
        let merged = load(directory.path()).unwrap();
        assert_eq!(merged["platforms"]["mac"]["mise"]["jobs"], 8);
        assert_eq!(merged["platforms"]["mac"]["tools"]["common"][0], "jq");
        fs::write(directory.path().join("c.toml"), "[palete]\nbg = \"#000\"\n").unwrap();
        let error = load(directory.path()).unwrap_err();
        assert!(error.starts_with(".chezmoidata/c.toml: "), "{error}");
        assert!(error.contains("palete"), "{error}");
    }

    #[test]
    fn the_schema_names_every_profile() {
        let schema = schema();
        for profile in ["linux", "mac", "windows", "wsl"] {
            assert!(schema.contains(&format!("\"{profile}\"")), "{profile}");
        }
    }
}
