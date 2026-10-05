//! Reaper configuration files hold `KEY=VALUE` lines with the semantics of `set -a; . file`: the file overrides the environment, and built-in defaults fill the rest.

use std::collections::HashMap;
use std::path::Path;

/// Parse a config file into a map.
pub fn load(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            out.insert(k.trim().to_owned(), v.trim().trim_matches('"').to_owned());
        }
    }
    out
}

/// Looks a key up in the file, then the environment, then the default, and expands `$VAR` and `${VAR}` in the result.
pub fn get(file: &HashMap<String, String>, key: &str, default: &str) -> String {
    let raw = file
        .get(key)
        .cloned()
        .or_else(|| std::env::var(key).ok())
        .unwrap_or_else(|| default.to_owned());
    expand(&raw)
}

/// Expand `$NAME` and `${NAME}` from the environment, the way a sourced shell file would.
/// Existing values such as `SESSION_REAPER_USER_PATH_REGEX="^(${HOME}|…)"` depend on this expansion.
fn expand(value: &str) -> String {
    expand_with(value, |name| std::env::var(name).ok())
}

fn expand_with<F>(value: &str, lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = String::with_capacity(value.len());
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let (name, next) = if chars.get(i + 1) == Some(&'{') {
            let end = chars[i + 2..]
                .iter()
                .position(|c| *c == '}')
                .map(|p| i + 2 + p);
            match end {
                Some(e) => (chars[i + 2..e].iter().collect::<String>(), e + 1),
                None => (String::new(), i + 1),
            }
        } else {
            let e = chars[i + 1..]
                .iter()
                .position(|c| !(c.is_ascii_alphanumeric() || *c == '_'))
                .map_or(chars.len(), |p| i + 1 + p);
            (chars[i + 1..e].iter().collect::<String>(), e)
        };
        if name.is_empty() {
            out.push('$');
        } else if let Some(v) = lookup(&name) {
            out.push_str(&v);
        }
        i = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{get, load};
    use std::collections::HashMap;

    #[test]
    fn file_values_win_over_env_and_default() {
        let mut file = HashMap::new();
        file.insert("A".to_owned(), "from-file".to_owned());
        assert_eq!(get(&file, "A", "d"), "from-file");
        assert_eq!(get(&file, "MISSING", "d"), "d");
    }

    #[test]
    fn shell_variables_expand_the_way_sourcing_would() {
        let vars = |name: &str| (name == "REAPER_TEST_HOME").then(|| "/Users/test".to_owned());
        let e = |v: &str| super::expand_with(v, vars);
        assert_eq!(
            e("^(${REAPER_TEST_HOME}|/opt/homebrew)/"),
            "^(/Users/test|/opt/homebrew)/"
        );
        assert_eq!(e("$REAPER_TEST_HOME/bin"), "/Users/test/bin");
        assert_eq!(e("^(a|b)$"), "^(a|b)$");
        assert_eq!(e("$NOT_SET"), "");
        assert_eq!(e("no variables here"), "no variables here");
    }

    #[test]
    fn parsing_skips_comments_and_quotes() {
        let dir = std::env::temp_dir().join("reaper-test-config");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config");
        std::fs::write(
            &p,
            "# comment\n\n MODE=full\nNAME=\"quoted value\"\nbroken line\n",
        )
        .unwrap();
        let m = load(&p);
        assert_eq!(m.get("MODE").map(String::as_str), Some("full"));
        assert_eq!(m.get("NAME").map(String::as_str), Some("quoted value"));
        assert!(!m.contains_key("broken"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
