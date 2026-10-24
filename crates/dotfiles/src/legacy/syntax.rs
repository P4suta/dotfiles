use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Position {
    offset: usize,
    line: usize,
    col: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Span {
    pos: Position,
    end: Position,
}

impl Span {
    fn text<'a>(&self, source: &'a str) -> Result<&'a str> {
        use crate::legacy_rules::{SpellingSpan, spelling_span};
        let range = match spelling_span(
            self.pos.offset,
            self.end.offset,
            source.len(),
            (self.pos.line != 0 || self.pos.col != 0) && (self.end.line != 0 || self.end.col != 0),
            [
                source.is_char_boundary(self.pos.offset),
                source.is_char_boundary(self.end.offset),
            ],
        ) {
            SpellingSpan::InSource(range) => range,
            SpellingSpan::Unpositioned => {
                anyhow::bail!("shell syntax word has an invalid position")
            }
            SpellingSpan::Empty => anyhow::bail!("shell syntax word has an empty span"),
            SpellingSpan::Outside => anyhow::bail!("shell syntax word lies outside its source"),
            SpellingSpan::NonBoundary => {
                anyhow::bail!("shell syntax word is not on UTF-8 boundaries")
            }
        };
        let (start, end) = range.bounds();
        source
            .get(start..end)
            .context("shell syntax word lies outside its UTF-8 source")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Call {
    #[serde(flatten)]
    span: Span,
    #[serde(default)]
    assigns: Vec<Span>,
    #[serde(default)]
    args: Vec<Span>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Declaration {
    #[serde(flatten)]
    span: Span,
    variant: Span,
    #[serde(default)]
    args: Vec<Span>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct File {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(flatten)]
    span: Span,
    #[serde(default)]
    stmts: Vec<Statement>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Statement {
    #[serde(flatten)]
    span: Span,
    cmd: Option<Value>,
    #[serde(default)]
    redirs: Vec<Redirect>,
}

#[derive(Deserialize)]
enum CommandKind {
    CallExpr,
    IfClause,
    WhileClause,
    ForClause,
    CaseClause,
    Block,
    Subshell,
    BinaryCmd,
    FuncDecl,
    ArithmCmd,
    TestClause,
    DeclClause,
    LetClause,
    TimeClause,
    CoprocClause,
    TestDecl,
}

fn check_command(command: &Value) -> Result<()> {
    let _: CommandKind = serde_json::from_value(
        command
            .get("Type")
            .context("shell syntax command type is missing")?
            .clone(),
    )?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Redirect {
    #[serde(flatten)]
    span: Span,
    op: String,
    hdoc: Option<Span>,
    word: Option<Span>,
    n: Option<Span>,
}

pub(super) enum Input<'a> {
    Script(Cow<'a, str>),
    Unavailable,
}

impl Redirect {
    fn literal_stdin(&self, source: &str, input: &Input<'_>) -> Result<bool> {
        if let Some(descriptor) = &self.n
            && descriptor.text(source)? != "0"
        {
            return Ok(false);
        }
        let Input::Script(body) = input else {
            return Ok(false);
        };
        let Some(word) = &self.word else {
            return Ok(false);
        };
        ensure!(
            self.span.pos.offset <= word.pos.offset && word.end.offset <= self.span.end.offset,
            "shell heredoc delimiter lies outside its redirect"
        );
        let delimiter = word.text(source)?;
        Ok(delimiter.contains(['\'', '"', '\\']) || !body.contains(['$', '`', '\\']))
    }

    fn input<'a>(&self, source: &'a str) -> Result<Option<Input<'a>>> {
        match self.op.as_str() {
            "<<" | "<<-" => {
                let Some(body) = self.hdoc.as_ref() else {
                    return Ok(Some(Input::Script(Cow::Borrowed(""))));
                };
                let document = source
                    .get(body.pos.offset..self.span.end.offset)
                    .context("shell heredoc lies outside its UTF-8 source")?;
                let delimiter = document.rfind('\n').map_or(0, |index| index + 1);
                let body = &document[..delimiter];
                let body = if self.op == "<<-" {
                    Cow::Owned(
                        body.split_inclusive('\n')
                            .map(|line| line.trim_start_matches('\t'))
                            .collect(),
                    )
                } else {
                    Cow::Borrowed(body)
                };
                Ok(Some(Input::Script(body)))
            }
            "<" | "<&" | "<>" | "<<<" => Ok(Some(Input::Unavailable)),
            ">" | ">>" | ">&" | ">|" | "&>" | "&>>" => Ok(None),
            _ => anyhow::bail!("shell syntax redirection is unsupported"),
        }
    }
}

struct Word<'a> {
    value: String,
    spelling: Option<&'a str>,
}

pub(super) struct Argument<'a> {
    pub value: &'a str,
    pub spelling: &'a str,
    pub order: usize,
}

pub(super) struct Invocation<'a> {
    offset: usize,
    words: Vec<Word<'a>>,
    inputs: Vec<Input<'a>>,
    literal_stdin: Option<String>,
    file_inputs_safe: bool,
}

impl<'a> Invocation<'a> {
    fn new(
        source: &'a str,
        span: &Span,
        mut words: Vec<Span>,
        redirects: &[Redirect],
    ) -> Result<Self> {
        span.text(source)?;
        ensure!(!words.is_empty(), "shell syntax call has no words");
        words.sort_by_key(|word| word.pos.offset);
        let mut end = span.pos.offset;
        let words = words
            .into_iter()
            .map(|word| {
                ensure!(
                    end <= word.pos.offset && word.end.offset <= span.end.offset,
                    "shell syntax words overlap or leave their call"
                );
                end = word.end.offset;
                let spelling = word.text(source)?;
                let mut parsed = crate::shell_write_rules::segments(spelling);
                let value = if parsed.len() == 1 && parsed[0].len() == 1 {
                    parsed.remove(0).remove(0)
                } else {
                    spelling.to_owned()
                };
                Ok(Word {
                    value,
                    spelling: Some(spelling),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut inputs = Vec::new();
        let mut literal_stdin = None;
        let mut uncertain_stdin = false;
        for redirect in redirects {
            if let Some(input) = redirect.input(source)? {
                if redirect.literal_stdin(source, &input)?
                    && let Input::Script(body) = &input
                {
                    literal_stdin = Some(body.to_string());
                }
                inputs.push(input);
            } else if let Some(descriptor) = &redirect.n
                && descriptor.text(source)? == "0"
            {
                uncertain_stdin = true;
            }
        }
        if inputs.len() != 1 || uncertain_stdin {
            literal_stdin = None;
        }
        Ok(Self {
            offset: span.pos.offset,
            words,
            inputs,
            literal_stdin,
            file_inputs_safe: redirects.is_empty(),
        })
    }

    pub(super) fn values(&self) -> Vec<String> {
        self.words.iter().map(|word| word.value.clone()).collect()
    }

    pub(super) fn unparsed(values: Vec<String>) -> Self {
        Self {
            offset: 0,
            words: values
                .into_iter()
                .map(|value| Word {
                    value,
                    spelling: None,
                })
                .collect(),
            inputs: Vec::new(),
            literal_stdin: None,
            file_inputs_safe: false,
        }
    }

    pub(super) fn inputs(&self) -> &[Input<'_>] {
        &self.inputs
    }

    pub(super) fn literal_stdin(&self) -> Option<&str> {
        self.literal_stdin.as_deref()
    }

    pub(super) const fn file_inputs_safe(&self) -> bool {
        self.file_inputs_safe
    }

    pub(super) fn arguments(&self, start: usize) -> Result<Vec<Argument<'_>>> {
        self.words
            .get(start..)
            .context("shell argument offset is outside its call")?
            .iter()
            .enumerate()
            .map(|(order, word)| {
                Ok(Argument {
                    value: &word.value,
                    spelling: word
                        .spelling
                        .context("shell argument spelling is unavailable")?,
                    order,
                })
            })
            .collect::<Result<Vec<_>>>()
    }
}

fn collect<'a>(
    source: &'a str,
    node: &Value,
    calls: &mut Vec<Invocation<'a>>,
    redirects: &[Redirect],
) -> Result<()> {
    match node {
        Value::Object(fields) => {
            match fields.get("Type").and_then(Value::as_str) {
                Some("CallExpr") => {
                    let call: Call = serde_json::from_value(node.clone())?;
                    calls.push(Invocation::new(
                        source,
                        &call.span,
                        call.assigns.into_iter().chain(call.args).collect(),
                        redirects,
                    )?);
                }
                Some("DeclClause") => {
                    let declaration: Declaration = serde_json::from_value(node.clone())?;
                    calls.push(Invocation::new(
                        source,
                        &declaration.span,
                        core::iter::once(declaration.variant)
                            .chain(declaration.args)
                            .collect(),
                        redirects,
                    )?);
                }
                _ => {}
            }
            for (name, value) in fields {
                let redirects: Vec<Redirect> = if name == "Cmd" {
                    check_command(value)?;
                    fields
                        .get("Redirs")
                        .map(|value| serde_json::from_value(value.clone()))
                        .transpose()?
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                collect(source, value, calls, &redirects)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                collect(source, value, calls, &[])?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn decode<'a>(source: &'a str, tree: &str) -> Result<Vec<Invocation<'a>>> {
    crate::lint_hook::check_shell_syntax_source(source)?;
    crate::lint_hook::check_shell_syntax_output(tree)?;
    let tree: File = serde_json::from_str(tree)?;
    ensure!(tree.kind == "File", "shell syntax output is not a File");
    source
        .get(tree.span.pos.offset..tree.span.end.offset)
        .context("shell syntax file lies outside its UTF-8 source")?;
    let mut calls = Vec::new();
    for statement in tree.stmts {
        statement.span.text(source)?;
        if let Some(command) = statement.cmd {
            check_command(&command)?;
            collect(source, &command, &mut calls, &statement.redirs)?;
        }
    }
    calls.sort_by_key(|call| call.offset);
    Ok(calls)
}

pub(super) fn parse(source: &str) -> Result<Vec<Invocation<'_>>> {
    decode(source, &crate::lint_hook::shell_syntax_tree(source)?)
}

pub(super) fn check(source: &str, tree: &str) -> Result<()> {
    decode(source, tree).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_parser_keeps_source_words_separate_from_redirections() -> Result<()> {
        let source = include_str!("../../../../fuzz/corpus/shell_suggestions/quoting");
        let calls = parse(source)?;
        assert_eq!(calls.len(), 5);
        assert_eq!(
            calls[0].values(),
            ["jq", "-c", ".foo", "~/.codex/hooks.json"]
        );
        for (index, expected) in [
            (0, vec!["-c", "'.foo'", "~/.codex/hooks.json"]),
            (1, vec!["'.foo'", "'~/path with spaces'"]),
            (2, vec!["'.foo'", "~/path\\ with\\ spaces"]),
            (4, vec!["'.foo'", "'literal'\"suffix\""]),
        ] {
            assert_eq!(
                calls[index]
                    .arguments(1)?
                    .iter()
                    .map(|argument| argument.spelling)
                    .collect::<Vec<_>>(),
                expected
            );
        }
        let arguments = calls[3].arguments(4)?;
        assert_eq!(
            arguments
                .iter()
                .map(|argument| argument.spelling)
                .collect::<Vec<_>>(),
            ["'.foo'", "\"$HOME/file name\"", "''"]
        );
        assert_eq!(
            arguments
                .iter()
                .map(|argument| argument.value)
                .collect::<Vec<_>>(),
            [".foo", "$HOME/file name", ""]
        );
        Ok(())
    }

    #[test]
    fn assignments_and_declarations_remain_available_for_literal_script_resolution() -> Result<()> {
        let calls = parse("SCRIPT='check file.sh'; export COPY=\"$SCRIPT\"; bash \"$COPY\"")?;
        assert_eq!(
            calls.iter().map(Invocation::values).collect::<Vec<_>>(),
            [
                vec!["SCRIPT=check file.sh"],
                vec!["export", "COPY=$SCRIPT"],
                vec!["bash", "$COPY"]
            ]
        );
        Ok(())
    }

    #[test]
    fn heredoc_inputs_exclude_delimiters_and_strip_only_requested_leading_tabs() -> Result<()> {
        for (source, expected) in [
            ("sh <<'EOF'\ngrep x f\nEOF\n", "grep x f\n"),
            ("sh <<'END'\nEND\n", ""),
            ("sh <<'END'\nfirst\nlast\nEND\n", "first\nlast\n"),
            ("sh <<'EOF'\r\ngrep x f\r\nEOF\r\n", "grep x f\r\n"),
            ("sh <<'EOF'\ngrep x f\nEOF", "grep x f\n"),
            ("sh <<-EOF\n\tgrep x f\n\tEOF\n", "grep x f\n"),
            ("sh <<EOF\n\tgrep x f\nEOF\n", "\tgrep x f\n"),
        ] {
            let calls = parse(source)?;
            let [Input::Script(body)] = calls[0].inputs() else {
                panic!("stdin script missing");
            };
            assert_eq!(body, expected);
        }
        Ok(())
    }

    #[test]
    fn literal_stdin_requires_one_known_standard_input_heredoc() -> Result<()> {
        for (source, expected) in [
            (
                "gh pr comment 3 -F - <<'EOF'\n$literal \\ text\nEOF",
                Some("$literal \\ text\n"),
            ),
            (
                "gh pr comment 3 -F - <<-EOF\n\tText.\n\tEOF",
                Some("Text.\n"),
            ),
            ("gh pr comment 3 -F - <<EOF\nText.\nEOF", Some("Text.\n")),
            ("gh pr comment 3 -F - <<EOF\n$expanded\nEOF", None),
            ("gh pr comment 3 -F - <<EOF\nback\\slash\nEOF", None),
            ("gh pr comment 3 -F - 3<<'EOF'\nText.\nEOF", None),
            ("gh pr comment 3 -F - <<'EOF' 0<&3\nText.\nEOF", None),
            ("gh pr comment 3 -F - <<'EOF' 0>out\nText.\nEOF", None),
            (
                "gh pr comment 3 -F - <<'EOF' <<'END'\nOne.\nEOF\nTwo.\nEND",
                None,
            ),
            ("gh pr comment 3 -F - <<< 'Text.'", None),
        ] {
            let calls = parse(source)?;
            assert_eq!(calls[0].literal_stdin(), expected, "{source}");
        }
        Ok(())
    }

    #[test]
    fn malformed_or_unreadable_syntax_never_supplies_argument_spellings() {
        assert!(parse("jq '").is_err());
        assert!(parse("jq \0").is_err());
        assert!(decode("jq", "{}").is_err());
        assert!(decode("jq", r#"{"Type":"File"}"#).is_err());
        assert!(decode("jq", r#"{"Type":"File","Stmts":[42]}"#).is_err());
        for (source, start, end) in [("jq", 0, 3), ("jq", 2, 1), ("é", 1, 2)] {
            let position = |offset| serde_json::json!({"Offset":offset,"Line":1,"Col":1});
            let tree = serde_json::json!({"Type":"File","Pos":position(0),"End":position(source.len()),"Stmts":[{"Pos":position(0),"End":position(source.len()),"Cmd":{
                "Type":"CallExpr","Pos":position(0),"End":position(source.len()),
                "Args":[{"Pos":position(start),"End":position(end)}]
            }}]});
            assert!(
                decode(source, &tree.to_string()).is_err(),
                "{source} {start} {end}"
            );
        }
    }

    #[test]
    fn byte_offsets_remain_usable_after_the_parser_drops_an_overflowed_column() -> Result<()> {
        const COLUMN_OVERFLOW: usize = 1 << 14;
        let source = format!("jq '{}'", "x".repeat(COLUMN_OVERFLOW));
        let calls = parse(&source)?;
        assert_eq!(
            calls[0].arguments(1)?[0].spelling,
            source.strip_prefix("jq ").unwrap()
        );
        Ok(())
    }

    #[test]
    fn the_registered_decoder_corpus_has_a_valid_seed_and_a_rejecting_range() -> Result<()> {
        for (fixture, accepted) in [
            (
                include_str!("../../../../fuzz/corpus/shell_suggestions/valid-tree"),
                true,
            ),
            (
                include_str!("../../../../fuzz/corpus/shell_suggestions/invalid-range"),
                false,
            ),
        ] {
            let (source, tree) = fixture
                .split_once("\n---\n")
                .context("corpus frame missing")?;
            assert_eq!(check(source, tree).is_ok(), accepted);
        }
        Ok(())
    }

    #[test]
    fn unknown_command_and_redirection_kinds_leave_inspection_incomplete() -> Result<()> {
        let source = "sh < input";
        let tree = crate::lint_hook::shell_syntax_tree(source)?;
        let mut unknown_command: Value = serde_json::from_str(&tree)?;
        unknown_command["Stmts"][0]["Cmd"]["Type"] = Value::String("FutureCommand".to_owned());
        assert!(check(source, &unknown_command.to_string()).is_err());
        let mut unknown_redirect: Value = serde_json::from_str(&tree)?;
        unknown_redirect["Stmts"][0]["Redirs"][0]["Op"] =
            Value::String("FutureRedirect".to_owned());
        assert!(check(source, &unknown_redirect.to_string()).is_err());
        Ok(())
    }
}
