//! Strips AI attribution lines from a commit message.
//!
//! A session link exposes a private conversation in permanent, widely copied text.
//! Claude Code rewrites `~/.claude/settings.json` at runtime, so the attribution settings there stay machine-local, and this hook enforces the rule on every machine.
//! A forge composes a squash-merge message on its server, beyond the reach of any local hook.
//!
//! Each pattern anchors to trailer position, so human `Co-authored-by` trailers and prose that mentions Claude survive.
//! The hook strips the lines instead of refusing the commit, and reports the count it removed.

/// Reports whether this line credits an AI assistant.
fn is_attribution(line: &str) -> bool {
    let t = line.trim_start();
    let tail = t.trim_end();

    // Claude-Session: https://…
    if let Some(rest) = strip_prefix_ci(t, "claude-session:") {
        let r = rest.trim_start();
        if r.starts_with("https://") || r.starts_with("http://") {
            return true;
        }
    }

    // Co-Authored-By: <anything> <…@anthropic.com>
    //
    // Match the address, which stays fixed while the display name changes with each model, so a human trailer never matches.
    if strip_prefix_ci(t, "co-authored-by:").is_some()
        && (tail.ends_with("@anthropic.com") || tail.ends_with("@anthropic.com>"))
    {
        return true;
    }

    // 🤖 Generated with [Claude Code](…)
    if let Some(rest) = t.strip_prefix('🤖')
        && rest
            .trim_start()
            .starts_with("Generated with [Claude Code]")
    {
        return true;
    }

    // A bare session link on a line of its own.
    if let Some(rest) = tail.strip_prefix("https://claude.ai/code/") {
        return !rest.is_empty() && !rest.contains(char::is_whitespace);
    }

    false
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// Returns the cleaned message and the count of removed lines.
pub fn strip(text: &str) -> (String, usize) {
    let mut kept: Vec<&str> = Vec::new();
    let mut removed = 0;

    for line in text.lines() {
        if is_attribution(line) {
            removed += 1;
        } else {
            kept.push(line);
        }
    }
    if removed == 0 {
        return (text.to_owned(), 0);
    }

    // Collapse the blank run that the trailer block leaves behind, so the message ends cleanly.
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }

    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    (out, removed)
}

#[cfg(test)]
mod tests {
    use super::{is_attribution, strip};

    #[test]
    fn agent_trailers_are_removed() {
        for line in [
            "Claude-Session: https://claude.ai/code/session_01Qesu",
            "  Claude-Session:   https://claude.ai/code/session_01Qesu",
            "Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>",
            "co-authored-by: Claude <noreply@anthropic.com>",
            "🤖 Generated with [Claude Code](https://claude.com/claude-code)",
            "https://claude.ai/code/session_01Qesu",
        ] {
            assert!(is_attribution(line), "should strip: {line}");
        }
    }

    #[test]
    fn humans_and_prose_survive() {
        for line in [
            "Co-authored-by: Example User <example@example.com>",
            "Co-authored-by: dependabot[bot] <support@github.com>",
            "Document the CLAUDE.md contract for new contributors",
            "The Claude-Session: prefix is what this hook looks for.",
            "See https://claude.ai/code/session_01Qesu for the transcript.",
            "Signed-off-by: Example User <example@example.com>",
        ] {
            assert!(!is_attribution(line), "should keep: {line}");
        }
    }

    #[test]
    fn trailing_blank_lines_are_collapsed() {
        let msg = "feat: do the thing\n\nSome body text.\n\n\
                   Co-Authored-By: Claude <noreply@anthropic.com>\n";
        let (out, n) = strip(msg);
        assert_eq!(n, 1);
        assert_eq!(out, "feat: do the thing\n\nSome body text.\n");
    }

    #[test]
    fn a_clean_message_is_returned_byte_for_byte() {
        let msg = "feat: do the thing\n\nBody.\n";
        let (out, n) = strip(msg);
        assert_eq!(n, 0);
        assert_eq!(out, msg);
    }
}
