//! The gate that holds every profile to the modern tool table: each preferred tool must reach every profile at its pinned version.

use anyhow::{Context, Result, ensure};
use model::data::Modern;
use std::fs;
use std::path::Path;

/// The table, relative to the source state.
pub const TABLE: &str = ".chezmoidata/modern.toml";

/// The rendered mise configuration that installs a profile's tools.
pub const MISE_CONFIG: &str = ".config/mise/config.toml";

/// The modern tool table of the source state at `source`.
///
/// # Errors
///
/// Returns an error when reading or parsing the table fails.
pub fn table(source: &Path) -> Result<Modern> {
    let path = source.join(TABLE);
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    model::data::parse(TABLE, &text)
        .map_err(|error| anyhow::anyhow!("{TABLE} is invalid: {error}"))?
        .modern
        .with_context(|| format!("{TABLE} holds no [modern] table"))
}

/// The preferred tools that a rendered mise configuration lacks or pins at another version.
///
/// # Errors
///
/// Returns an error when the configuration fails to parse as TOML.
pub fn missing(table: &Modern, config: &str) -> Result<Vec<String>> {
    let config: toml::Table =
        toml::from_str(config).context("the rendered mise configuration is invalid TOML")?;
    let tools = config.get("tools").and_then(toml::Value::as_table);
    Ok(table
        .tools
        .iter()
        .filter(|tool| {
            tools
                .and_then(|tools| tools.get(&tool.tool))
                .and_then(toml::Value::as_str)
                != Some(tool.version.as_str())
        })
        .map(|tool| format!("{}@{} ({})", tool.tool, tool.version, tool.preferred))
        .collect())
}

/// Refuses a profile whose mise configuration lacks a preferred tool or pins it at another version.
///
/// # Errors
///
/// Returns an error when the profile renders no mise configuration, or the configuration misses a pin.
pub fn check(table: &Modern, profile: &str, config: Option<&str>) -> Result<()> {
    let config = config.with_context(|| {
        format!("{profile} renders no {MISE_CONFIG}, so no preferred tool from {TABLE} reaches it")
    })?;
    let missing = missing(table, config)?;
    ensure!(
        missing.is_empty(),
        "{profile} doesn't install these preferred tools at their pins in {TABLE}, so the shell hook would suggest a tool the host lacks: {}; render them from the table in home/.chezmoitemplates/native/mise/, or take the entry out of the table",
        missing.join(", ")
    );
    Ok(())
}
