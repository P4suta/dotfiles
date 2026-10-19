//! The environment validation every `cargo xtask` run performs before it dispatches.
//! A path environment variable that still holds a variable reference sends every build to one literal directory, as the incident of #97 records.

use crate::env_expansion_rules;
use anyhow::{Result, bail};

/// The path environment variables the repository's tools read, which must hold expanded paths.
const PATH_VARIABLES: [&str; 3] = ["CARGO_TARGET_DIR", "CARGO_HOME", "RUSTUP_HOME"];

/// Refuses a path environment variable that still holds a `$name`, `${name}`, or `%NAME%` reference.
///
/// # Errors
///
/// Returns an error that names the variable, the reference, and the next action when the value still holds one.
pub fn check() -> Result<()> {
    for name in PATH_VARIABLES {
        let Some(value) = std::env::var_os(name) else {
            continue;
        };
        let value = value.to_string_lossy();
        let unexpanded = env_expansion_rules::marker(&value);
        if let env_expansion_rules::Verdict::Refuse =
            env_expansion_rules::verdict(unexpanded.is_some())
        {
            let reference = unexpanded
                .as_ref()
                .map_or("", env_expansion_rules::Reference::as_str);
            bail!(
                "Refused: the environment variable {name} still holds the unexpanded variable reference `{reference}`.\nNext action: {}",
                env_expansion_rules::NEXT_ACTION
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expanded_environment_passes() {
        assert!(check().is_ok());
    }
}
