use crate::shell_write_rules::{Verdict, inspect, next_action};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;

/// The refusal for one PreToolUse event, or `None` when the tool call is admitted.
pub fn refusal(event: &str) -> Result<Option<String>> {
    let event: Value = serde_json::from_str(event).context("PreToolUse event is not JSON")?;
    if event["tool_name"] != "Bash" {
        return Ok(None);
    }
    let command = event["tool_input"]["command"]
        .as_str()
        .context("Bash event has no command")?;
    Ok(match inspect(command) {
        Verdict::Admit => None,
        Verdict::Refuse(reason) => Some(format!(
            "Refused: this command writes a file through an inline interpreter, heredoc, or redirect.\nNext action: {}",
            next_action(reason)
        )),
    })
}

/// Reads one event from standard input and exits 2, the blocking status, with the refusal on standard error.
pub fn run() -> Result<()> {
    let mut event = String::new();
    std::io::stdin().read_to_string(&mut event)?;
    if let Some(message) = refusal(&event)? {
        eprintln!("{message}");
        std::process::exit(2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash(command: &str) -> String {
        json!({"tool_name": "Bash", "tool_input": {"command": command}}).to_string()
    }

    #[test]
    fn an_inline_interpreter_file_write_is_refused_with_the_next_action() {
        let message = refusal(&bash("python -c \"open('a','w').write('x')\""))
            .unwrap()
            .unwrap();
        assert!(message.contains("Next action") && message.contains("Edit or Write tool"));
    }

    #[test]
    fn project_commands_and_other_tools_are_admitted() {
        for command in ["just check", "pr-workflow start", "git status", "cat a.md"] {
            assert_eq!(refusal(&bash(command)).unwrap(), None, "{command}");
        }
        let write = json!({"tool_name": "Write", "tool_input": {"file_path": "a"}}).to_string();
        assert_eq!(refusal(&write).unwrap(), None);
    }

    #[test]
    fn a_malformed_event_is_an_error_rather_than_an_admission() {
        assert!(refusal("not json").is_err());
        assert!(refusal(r#"{"tool_name":"Bash","tool_input":{}}"#).is_err());
    }
}
