use super::{Shell, syntax::Invocation};
use crate::legacy_rules::{DirectoryContext, EnvironmentRole, EnvironmentWord, environment_word};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

const POWERSHELL_ERROR_ACTION_INDEX: usize = 3;

pub(super) type Context = DirectoryContext<PathBuf>;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum VariableScope {
    Event,
    Child(usize),
}

pub(super) enum Flow {
    Call(usize),
    Sequence(Vec<Self>),
    And(Box<Self>, Box<Self>),
    Child(usize, Box<Self>),
    Unresolved,
}

impl Flow {
    fn linear_assignments(&self) -> bool {
        match self {
            Self::Call(_) => true,
            Self::Sequence(items) => items.iter().all(Self::linear_assignments),
            Self::Child(_, child) => child.linear_assignments(),
            Self::And(_, _) | Self::Unresolved => false,
        }
    }
}

struct Outcome {
    success: Context,
    failure: Context,
    next: Context,
}

impl Outcome {
    fn stable(context: Context) -> Self {
        Self {
            success: context.clone(),
            failure: context.clone(),
            next: context,
        }
    }
}

fn target(invocation: &Invocation<'_>, index: usize) -> Option<PathBuf> {
    let value = invocation.literal(index)?;
    let path = Path::new(&value);
    (path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        && !value.contains(['*', '?', '[', ']']))
    .then(|| path.to_path_buf())
}

fn binding_change(shell: Shell, name: &str, words: &[String]) -> bool {
    match shell {
        Shell::Bash => {
            matches!(
                name,
                "." | "source"
                    | "eval"
                    | "alias"
                    | "unalias"
                    | "shopt"
                    | "enable"
                    | "trap"
                    | "builtin"
                    | "read"
                    | "readarray"
                    | "mapfile"
                    | "unset"
                    | "declare"
                    | "typeset"
                    | "local"
                    | "readonly"
            ) || (name == "printf" && words.iter().any(|word| word == "-v"))
        }
        Shell::PowerShell => {
            matches!(
                name,
                "." | "invoke-expression"
                    | "iex"
                    | "set-alias"
                    | "new-alias"
                    | "remove-alias"
                    | "import-module"
                    | "invoke-command"
            ) || words.iter().any(|word| {
                word.to_ascii_lowercase().starts_with("function:")
                    || word.to_ascii_lowercase().starts_with("alias:")
            })
        }
    }
}

fn powershell_location(first: &str) -> bool {
    match first.split_once('\\') {
        Some((module, cmdlet)) => {
            module.eq_ignore_ascii_case("Microsoft.PowerShell.Management")
                && ["set-location", "push-location", "pop-location"]
                    .iter()
                    .any(|name| cmdlet.eq_ignore_ascii_case(name))
        }
        None => {
            [
                "set-location",
                "cd",
                "cd..",
                "cd~",
                "chdir",
                "sl",
                "push-location",
                "pop-location",
                "pushd",
                "popd",
            ]
            .iter()
            .any(|name| first.eq_ignore_ascii_case(name))
                || matches!(first.as_bytes(), [drive, b':'] if drive.is_ascii_alphabetic())
        }
    }
}

fn powershell_script_or_qualified_call(first: &str) -> bool {
    let path = Path::new(first);
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ps1"))
        || (first.contains('\\') && !path.is_absolute())
}

fn uncertain_startup(words: &[String], shell: Shell) -> bool {
    let (command, _) = super::command_words(words);
    let exported = command.first().is_some_and(|word| word == "export");
    let prefix = if exported {
        &command[1..]
    } else {
        &words[..words.len() - command.len()]
    };
    let changed_environment = matches!(shell, Shell::Bash)
        && prefix.iter().any(|word| {
            environment_word(
                word,
                if exported {
                    EnvironmentRole::ExportOperand
                } else {
                    EnvironmentRole::Assignment
                },
            ) == EnvironmentWord::Startup
        });
    let startup = command.split_first().is_some_and(|(first, arguments)| {
        super::SHELLS.contains(&crate::shell_write_rules::program(first).as_str())
            && arguments
                .iter()
                .filter(|word| word.starts_with('-'))
                .any(|word| {
                    matches!(
                        word.as_str(),
                        "--rcfile" | "--init-file" | "--login" | "--interactive"
                    ) || (!word.starts_with("--") && (word.contains('l') || word.contains('i')))
                })
    });
    changed_environment || startup
}

fn call(invocation: &mut Invocation<'_>, context: Context, shell: Shell) -> Outcome {
    invocation.directory = context.clone();
    if invocation.opaque_effect {
        invocation.directory = context.clone().invalidate();
        return Outcome::stable(context.invalidate());
    }
    let words = invocation.values();
    if matches!(super::command_view(&words), super::CommandView::Unknown) {
        invocation.directory = context.clone().changed(None);
        invocation.unresolved_wrapper = true;
        return Outcome::stable(context);
    }
    if uncertain_startup(&words, shell) {
        invocation.directory = context.clone().invalidate();
        invocation.unresolved_wrapper = true;
        return Outcome::stable(context.invalidate());
    }
    let (command, _) = super::command_words(&words);
    let Some(first) = invocation.literal(0) else {
        return Outcome::stable(context.invalidate());
    };
    let name = crate::shell_write_rules::program(&first);
    if binding_change(shell, name.as_str(), &words) {
        return Outcome::stable(context.invalidate());
    }
    let location = match shell {
        Shell::Bash => matches!(first.as_str(), "cd" | "pushd" | "popd"),
        Shell::PowerShell => powershell_location(&first),
    };
    if !location {
        if matches!(shell, Shell::PowerShell) && powershell_script_or_qualified_call(&first) {
            return Outcome::stable(context.invalidate());
        }
        if name == "command"
            && command.first().is_some_and(|word| {
                matches!(
                    word.as_str(),
                    "cd" | "pushd" | "popd" | "source" | "." | "eval"
                )
            })
        {
            return Outcome::stable(context.invalidate());
        }
        return Outcome::stable(context);
    }
    let (destination, stops) = match shell {
        Shell::Bash => match words.as_slice() {
            _ if name != "cd" => (None, false),
            [_, separator, _] if separator == "--" => (target(invocation, 2), false),
            [_, _] => (target(invocation, 1), false),
            _ => (None, false),
        },
        Shell::PowerShell => match words.as_slice() {
            _ if !matches!(name.as_str(), "set-location" | "cd" | "chdir" | "sl") => (None, false),
            [_, literal, _, error, stop]
                if literal.eq_ignore_ascii_case("-LiteralPath")
                    && error.eq_ignore_ascii_case("-ErrorAction")
                    && invocation.parameter(1, "-LiteralPath")
                    && invocation.parameter(POWERSHELL_ERROR_ACTION_INDEX, "-ErrorAction")
                    && stop.eq_ignore_ascii_case("Stop") =>
            {
                (target(invocation, 2), true)
            }
            _ => (None, false),
        },
    };
    let success = context.clone().changed(destination);
    let failure = context;
    let next = if stops {
        success.clone()
    } else {
        success.clone().joined(&failure)
    };
    Outcome {
        success,
        failure,
        next,
    }
}

fn walk(
    flow: &Flow,
    calls: &mut [Invocation<'_>],
    context: Context,
    shell: Shell,
    scope: VariableScope,
) -> Outcome {
    match flow {
        Flow::Call(offset) => calls
            .iter_mut()
            .find(|call| call.offset == *offset)
            .map_or_else(
                || Outcome::stable(context.clone().invalidate()),
                |invocation| {
                    invocation.variable_scope = Some(scope);
                    call(invocation, context.clone(), shell)
                },
            ),
        Flow::Sequence(items) => {
            let mut outcome = Outcome::stable(context);
            for item in items {
                outcome = walk(item, calls, outcome.next, shell, scope);
            }
            outcome
        }
        Flow::And(left, right) => {
            let left = walk(left, calls, context, shell, scope);
            let right = walk(right, calls, left.success, shell, scope);
            Outcome {
                success: right.success,
                failure: left.failure.clone().joined(&right.failure),
                next: left.failure.joined(&right.next),
            }
        }
        Flow::Child(offset, child) => {
            walk(
                child,
                calls,
                context.clone(),
                shell,
                VariableScope::Child(*offset),
            );
            Outcome::stable(context)
        }
        Flow::Unresolved => Outcome::stable(context.invalidate()),
    }
}

pub(super) fn locate(
    flow: &Flow,
    calls: &mut [Invocation<'_>],
    directory: Option<&Path>,
    shell: Shell,
) {
    let context = Context::new(
        directory
            .filter(|path| path.is_absolute())
            .map(Path::to_path_buf),
    );
    walk(flow, calls, context, shell, VariableScope::Event);
    if !flow.linear_assignments() {
        for call in calls {
            call.variable_scope = None;
        }
    }
}

fn sequence(node: &Value) -> Flow {
    Flow::Sequence(
        node.get("Stmts")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |items| items.iter().map(bash).collect()),
    )
}

pub(super) fn bash(node: &Value) -> Flow {
    if node.get("Negated").and_then(Value::as_bool) == Some(true)
        || node.get("Background").and_then(Value::as_bool) == Some(true)
        || node.get("Coprocess").and_then(Value::as_bool) == Some(true)
    {
        return Flow::Unresolved;
    }
    if let Some(command) = node.get("Cmd") {
        return bash(command);
    }
    match node.get("Type").and_then(Value::as_str) {
        Some("File" | "Block") => sequence(node),
        Some("Subshell") => node["Pos"]["Offset"]
            .as_u64()
            .and_then(|offset| usize::try_from(offset).ok())
            .map_or(Flow::Unresolved, |offset| {
                Flow::Child(offset, Box::new(sequence(node)))
            }),
        Some("BinaryCmd") if node.get("Op").and_then(Value::as_str) == Some("&&") => {
            match (node.get("X"), node.get("Y")) {
                (Some(left), Some(right)) => Flow::And(Box::new(bash(left)), Box::new(bash(right))),
                _ => Flow::Unresolved,
            }
        }
        Some("CallExpr" | "DeclClause") => node
            .get("Pos")
            .and_then(|position| position.get("Offset"))
            .and_then(Value::as_u64)
            .and_then(|offset| usize::try_from(offset).ok())
            .map_or(Flow::Unresolved, Flow::Call),
        _ => Flow::Unresolved,
    }
}
