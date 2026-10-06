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
    pub herdr_agent: Option<HerdrAgent>,
    pub ocaml: Option<Ocaml>,
    pub palette: Option<BTreeMap<String, String>>,
    pub platforms: Option<Platforms>,
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
