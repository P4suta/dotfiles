extern crate alloc;
use self::alloc::string::String;
use self::alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileType {
    Shell,
    Zsh,
    Python,
    JavaScript,
    Workflow,
    Action,
    Toml,
    Dockerfile,
    Justfile,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Linter {
    Shellcheck,
    Shfmt,
    Zsh,
    RuffCheck,
    RuffFormat,
    Biome,
    Actionlint,
    Zizmor,
    TaploFormat,
    TaploCheck,
    Hadolint,
    Just,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Configuration {
    Repository,
    Strict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Clean,
    Excluded,
    Findings,
    Missing,
    Timeout,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    Admit,
    Block,
}

#[must_use]
pub fn classify(path: &str, first_line: &str) -> FileType {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if let Some(shebang) = first_line.strip_prefix("#!") {
        let mut words = shebang.split_whitespace();
        let interpreter = words.next().unwrap_or_default();
        let interpreter = if interpreter.ends_with("/env") {
            words
                .find(|word| !word.starts_with('-') && !word.contains('='))
                .unwrap_or_default()
        } else {
            interpreter
        };
        match interpreter.rsplit('/').next().unwrap_or_default() {
            "zsh" => return FileType::Zsh,
            "sh" | "bash" | "dash" | "ksh" => return FileType::Shell,
            _ => {}
        }
    }
    match name.rsplit_once('.') {
        Some((_, "sh" | "bash")) => FileType::Shell,
        Some((_, "zsh")) => FileType::Zsh,
        Some((_, "py" | "pyi")) => FileType::Python,
        Some((_, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts")) => {
            FileType::JavaScript
        }
        Some((_, "yml" | "yaml"))
            if path.starts_with(".github/workflows/")
                || path.contains("/.github/workflows/")
                || path.starts_with(".github\\workflows\\")
                || path.contains("\\.github\\workflows\\") =>
        {
            FileType::Workflow
        }
        Some(("action", "yml" | "yaml")) => FileType::Action,
        Some((_, "toml")) => FileType::Toml,
        _ if name == "Dockerfile" || name.starts_with("Dockerfile.") => FileType::Dockerfile,
        _ if name.eq_ignore_ascii_case("justfile") || name == ".justfile" => FileType::Justfile,
        _ => FileType::Other,
    }
}

#[must_use]
pub fn linters(kind: FileType) -> &'static [Linter] {
    match kind {
        FileType::Shell => &[Linter::Shellcheck, Linter::Shfmt],
        FileType::Zsh => &[Linter::Zsh],
        FileType::Python => &[Linter::RuffCheck, Linter::RuffFormat],
        FileType::JavaScript => &[Linter::Biome],
        FileType::Workflow | FileType::Action => &[Linter::Actionlint, Linter::Zizmor],
        FileType::Toml => &[Linter::TaploFormat, Linter::TaploCheck],
        FileType::Dockerfile => &[Linter::Hadolint],
        FileType::Justfile => &[Linter::Just],
        FileType::Other => &[],
    }
}

#[must_use]
pub fn configuration(present: bool) -> Configuration {
    if present {
        Configuration::Repository
    } else {
        Configuration::Strict
    }
}

#[must_use]
pub fn admission(outcomes: &[Outcome]) -> Admission {
    if outcomes
        .iter()
        .all(|outcome| matches!(outcome, Outcome::Clean | Outcome::Excluded))
    {
        Admission::Admit
    } else {
        Admission::Block
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScriptPath {
    Literal(String),
    Unresolved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptArgument {
    Literal,
    NeedsLiteral,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptInput {
    Literal,
    Missing,
    Replaced,
}

#[must_use]
pub const fn script_argument(xargs: bool, input: ScriptInput, inline: bool) -> ScriptArgument {
    if xargs && !inline && !matches!(input, ScriptInput::Literal) {
        ScriptArgument::NeedsLiteral
    } else {
        ScriptArgument::Literal
    }
}

#[must_use]
pub fn resolve_script(value: &str, variables: &[(&str, &str)]) -> ScriptPath {
    let mut result = String::new();
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '$' => {
                let braced = chars.peek() == Some(&'{');
                if braced {
                    chars.next();
                }
                let mut name = String::new();
                while chars
                    .peek()
                    .is_some_and(|character| character.is_ascii_alphanumeric() || *character == '_')
                {
                    if let Some(character) = chars.next() {
                        name.push(character);
                    }
                }
                if name.is_empty() || (braced && chars.next() != Some('}')) {
                    return ScriptPath::Unresolved;
                }
                let Some((_, literal)) = variables.iter().find(|(key, _)| *key == name) else {
                    return ScriptPath::Unresolved;
                };
                if literal.contains(['$', '`', '~', '*', '?', '[']) {
                    return ScriptPath::Unresolved;
                }
                result.push_str(literal);
            }
            '`' | '~' | '*' | '?' | '[' => return ScriptPath::Unresolved,
            _ => result.push(character),
        }
    }
    if result.is_empty() {
        ScriptPath::Unresolved
    } else {
        ScriptPath::Literal(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BashFeature {
    Mapfile,
    Readarray,
    AssociativeArray,
    Lowercase,
    Uppercase,
    PipeStderr,
    AppendStderr,
}

#[must_use]
pub fn bash4_features(line: &str) -> Vec<BashFeature> {
    let mut code = String::new();
    let mut expansions = String::new();
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            escaped = false;
            code.push(' ');
            expansions.push(' ');
            continue;
        }
        if character == '\\' && !single {
            escaped = true;
            continue;
        }
        if character == '\'' && !double {
            single = !single;
            code.push(' ');
            continue;
        }
        if character == '"' && !single {
            double = !double;
            code.push(' ');
            continue;
        }
        if !single && !double && character == '#' {
            break;
        }
        code.push(if single || double { ' ' } else { character });
        expansions.push(if single { ' ' } else { character });
    }
    let commands: Vec<_> = code
        .split([';', '|', '&', '(', ')'])
        .map(|command| {
            command
                .split_whitespace()
                .skip_while(|word| {
                    word.contains('=')
                        || matches!(*word, "command" | "builtin" | "exec" | "then" | "do")
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut found = Vec::new();
    if commands
        .iter()
        .any(|words| words.first() == Some(&"mapfile"))
    {
        found.push(BashFeature::Mapfile);
    }
    if commands
        .iter()
        .any(|words| words.first() == Some(&"readarray"))
    {
        found.push(BashFeature::Readarray);
    }
    if commands.iter().any(|words| {
        words
            .first()
            .is_some_and(|word| matches!(*word, "declare" | "typeset" | "local" | "readonly"))
            && words
                .iter()
                .skip(1)
                .any(|word| word.starts_with('-') && word.contains('A'))
    }) {
        found.push(BashFeature::AssociativeArray);
    }
    if expansions.split("${").skip(1).any(|part| {
        part.split_once('}')
            .is_some_and(|(expansion, _)| expansion.contains(",,"))
    }) {
        found.push(BashFeature::Lowercase);
    }
    if expansions.split("${").skip(1).any(|part| {
        part.split_once('}')
            .is_some_and(|(expansion, _)| expansion.contains("^^"))
    }) {
        found.push(BashFeature::Uppercase);
    }
    if code.contains("|&") {
        found.push(BashFeature::PipeStderr);
    }
    if code.contains("&>>") {
        found.push(BashFeature::AppendStderr);
    }
    found
}

#[cfg(any(test, kani))]
mod proofs {
    use super::*;

    pub fn spec_admission(outcomes: &[Outcome]) {
        let blocked = outcomes
            .iter()
            .any(|outcome| !matches!(outcome, Outcome::Clean | Outcome::Excluded));
        assert_eq!(
            admission(outcomes),
            if blocked {
                Admission::Block
            } else {
                Admission::Admit
            }
        );
    }

    #[cfg(kani)]
    fn kind() -> FileType {
        match kani::any::<u8>() {
            0 => FileType::Shell,
            1 => FileType::Zsh,
            2 => FileType::Python,
            3 => FileType::JavaScript,
            4 => FileType::Workflow,
            5 => FileType::Action,
            6 => FileType::Toml,
            7 => FileType::Dockerfile,
            8 => FileType::Justfile,
            _ => FileType::Other,
        }
    }

    #[cfg(kani)]
    fn outcome() -> Outcome {
        match kani::any::<u8>() {
            0 => Outcome::Clean,
            1 => Outcome::Excluded,
            2 => Outcome::Findings,
            3 => Outcome::Missing,
            4 => Outcome::Timeout,
            _ => Outcome::Failed,
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(5)]
    fn any_finding_or_missing_linter_blocks_and_unknown_types_are_admitted() {
        let outcomes = [outcome(), outcome(), outcome()];
        spec_admission(&outcomes);
        assert!(linters(FileType::Other).is_empty());
        assert_eq!(admission(&[]), Admission::Admit);
        kani::cover!(admission(&outcomes) == Admission::Block);
        kani::cover!(admission(&outcomes) == Admission::Admit);
        kani::cover!(outcomes[0] == Outcome::Missing);
        kani::cover!(outcomes[0] == Outcome::Findings);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(4)]
    fn linter_choice_and_configuration_cover_every_type() {
        let kind = kind();
        let present: bool = kani::any();
        assert_eq!(linters(kind).is_empty(), kind == FileType::Other);
        assert_eq!(configuration(present) == Configuration::Repository, present);
        kani::cover!(present && kind == FileType::Shell);
        kani::cover!(!present && kind == FileType::Other);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(32)]
    fn classification_preserves_the_registered_file_types() {
        for (path, line, expected) in [
            ("x.sh", "", FileType::Shell),
            ("x.py", "", FileType::Python),
            ("x.md", "", FileType::Other),
            ("x", "#!/bin/zsh", FileType::Zsh),
        ] {
            assert_eq!(classify(path, line), expected);
        }
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(16)]
    fn unresolved_variables_never_become_literal_paths() {
        assert_eq!(resolve_script("$f", &[]), ScriptPath::Unresolved);
        assert_eq!(
            resolve_script("$f", &[("f", "x.sh")]),
            ScriptPath::Literal(String::from("x.sh"))
        );
    }

    pub fn spec_script_argument(xargs: bool, input: ScriptInput, inline: bool) {
        assert_eq!(
            script_argument(xargs, input, inline) == ScriptArgument::NeedsLiteral,
            xargs && !inline && matches!(input, ScriptInput::Missing | ScriptInput::Replaced)
        );
    }

    #[cfg(kani)]
    #[kani::proof]
    fn xargs_cannot_supply_an_uninspected_script_path() {
        let xargs: bool = kani::any();
        let input = match kani::any::<u8>() {
            0 => ScriptInput::Literal,
            1 => ScriptInput::Missing,
            _ => ScriptInput::Replaced,
        };
        let inline: bool = kani::any();
        spec_script_argument(xargs, input, inline);
        kani::cover!(script_argument(xargs, input, inline) == ScriptArgument::NeedsLiteral);
        kani::cover!(script_argument(xargs, input, inline) == ScriptArgument::Literal);
    }

    #[cfg(kani)]
    #[kani::proof]
    #[kani::unwind(32)]
    fn bash4_features_distinguish_comments_and_commands() {
        assert!(bash4_features("mapfile a").contains(&BashFeature::Mapfile));
        assert_eq!(bash4_features("# mapfile a"), []);
        assert_eq!(bash4_features("echo 'mapfile'"), []);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_paths_and_interpreters() {
        for (path, line, kind) in [
            ("a.sh", "", FileType::Shell),
            ("a.bash", "", FileType::Shell),
            ("script", "#!/usr/bin/env bash", FileType::Shell),
            ("script", "#!/bin/dash", FileType::Shell),
            ("script", "#!/bin/ksh", FileType::Shell),
            ("a.sh", "#!/bin/zsh", FileType::Zsh),
            ("a.zsh", "", FileType::Zsh),
            (
                "C:\\work\\.github\\workflows\\check.yml",
                "",
                FileType::Workflow,
            ),
            ("a.py", "", FileType::Python),
            ("a.tsx", "", FileType::JavaScript),
            ("a.mjs", "", FileType::JavaScript),
            (".github/workflows/check.yml", "", FileType::Workflow),
            (
                "/tmp/a/.github/workflows/check.yaml",
                "",
                FileType::Workflow,
            ),
            ("action.yml", "", FileType::Action),
            ("a/action.yaml", "", FileType::Action),
            ("a.toml", "", FileType::Toml),
            ("Dockerfile", "", FileType::Dockerfile),
            ("justfile", "", FileType::Justfile),
            ("a.rs", "", FileType::Other),
            ("a.md", "", FileType::Other),
            ("a.yml", "", FileType::Other),
        ] {
            assert_eq!(classify(path, line), kind, "{path}: {line}");
        }
    }

    #[test]
    fn chooses_every_linter_and_the_repository_configuration() {
        for (kind, expected) in [
            (FileType::Shell, &[Linter::Shellcheck, Linter::Shfmt][..]),
            (FileType::Zsh, &[Linter::Zsh][..]),
            (
                FileType::Python,
                &[Linter::RuffCheck, Linter::RuffFormat][..],
            ),
            (FileType::JavaScript, &[Linter::Biome][..]),
            (
                FileType::Workflow,
                &[Linter::Actionlint, Linter::Zizmor][..],
            ),
            (FileType::Action, &[Linter::Actionlint, Linter::Zizmor][..]),
            (
                FileType::Toml,
                &[Linter::TaploFormat, Linter::TaploCheck][..],
            ),
            (FileType::Dockerfile, &[Linter::Hadolint][..]),
            (FileType::Justfile, &[Linter::Just][..]),
            (FileType::Other, &[][..]),
        ] {
            assert_eq!(linters(kind), expected);
        }
        assert_eq!(configuration(true), Configuration::Repository);
        assert_eq!(configuration(false), Configuration::Strict);
    }

    #[test]
    fn refuses_findings_missing_linters_timeouts_and_failures() {
        for outcome in [
            Outcome::Findings,
            Outcome::Missing,
            Outcome::Timeout,
            Outcome::Failed,
        ] {
            assert_eq!(admission(&[Outcome::Clean, outcome]), Admission::Block);
            assert_eq!(admission(&[outcome]), Admission::Block);
        }
        assert_eq!(
            admission(&[Outcome::Clean, Outcome::Excluded]),
            Admission::Admit
        );
        assert_eq!(admission(&[]), Admission::Admit);
        for third in [
            Outcome::Clean,
            Outcome::Excluded,
            Outcome::Findings,
            Outcome::Missing,
            Outcome::Timeout,
            Outcome::Failed,
        ] {
            for first in [
                Outcome::Clean,
                Outcome::Excluded,
                Outcome::Findings,
                Outcome::Missing,
                Outcome::Timeout,
                Outcome::Failed,
            ] {
                for second in [
                    Outcome::Clean,
                    Outcome::Excluded,
                    Outcome::Findings,
                    Outcome::Missing,
                    Outcome::Timeout,
                    Outcome::Failed,
                ] {
                    proofs::spec_admission(&[first, second, third]);
                }
            }
        }
    }

    #[test]
    fn resolves_only_literal_assigned_script_paths() {
        assert_eq!(
            resolve_script("$ROOT/a.sh", &[("ROOT", "/tmp")]),
            ScriptPath::Literal(String::from("/tmp/a.sh"))
        );
        assert_eq!(
            resolve_script("${f}", &[("f", "a.sh")]),
            ScriptPath::Literal(String::from("a.sh"))
        );
        for value in ["$f", "$(pwd)/a.sh", "`pwd`/a.sh", "~/a.sh", "", "${f:-a}"] {
            assert_eq!(resolve_script(value, &[]), ScriptPath::Unresolved);
        }
    }

    #[test]
    fn xargs_requires_a_known_script_argument() {
        for [xargs, inline] in crate::domain::flags() {
            for input in [
                ScriptInput::Literal,
                ScriptInput::Missing,
                ScriptInput::Replaced,
            ] {
                proofs::spec_script_argument(xargs, input, inline);
            }
        }
    }

    #[test]
    fn detects_bash4_features_without_flagging_single_quoted_programs_or_comments() {
        for (line, feature) in [
            ("mapfile a", BashFeature::Mapfile),
            ("readarray a", BashFeature::Readarray),
            ("declare -A a", BashFeature::AssociativeArray),
            ("echo \"${a,,}\"", BashFeature::Lowercase),
            ("echo ${a^^}", BashFeature::Uppercase),
            ("a |& b", BashFeature::PipeStderr),
            ("a &>> b", BashFeature::AppendStderr),
        ] {
            assert_eq!(bash4_features(line), [feature]);
        }
        assert_eq!(bash4_features("# mapfile a"), []);
        assert_eq!(bash4_features("echo 'mapfile'"), []);
        assert_eq!(bash4_features("printf '%s' \"mapfile |& &>>\""), []);
        assert_eq!(bash4_features("echo mapfile"), []);
        assert_eq!(
            bash4_features("declare -Ag values"),
            [BashFeature::AssociativeArray]
        );
    }
}
