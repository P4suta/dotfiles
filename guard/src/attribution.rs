//! Removing an agent's byline from a commit message.
//!
//! Whose name appears on a commit is the author's call, and the answer here is that the tooling does not get one.
//! The session URL is worse than a byline: it points at a private conversation, in the most durable and most widely copied text a project emits, and unlike a credential it cannot be rotated afterwards.
//!
//! This is the second line of defence.
//! `~/.claude/settings.json` carries `includeCoAuthoredBy: false` and empty `attribution.commit` / `attribution.pr`, which stop the lines being written at all — but Claude Code rewrites that file at runtime, so the profiles merge only a few keys into it and leave these to each machine, and a setting that does not travel is not a policy.
//! This hook travels.
//! (One gap remains and is not closeable from here: a forge composes the message for a squash merge on its own server, where no local hook runs.)
//!
//! Every pattern is anchored to trailer position, which matters more than it looks.
//! The rules below were validated against 29,264 lines of real commit messages across ten repositories: they remove 795 attribution lines and leave untouched the author's own `Co-authored-by: Example User <…>`, dependabot's 86, and the prose that merely mentions CLAUDE.md.
//! "Any line mentioning Claude" would have eaten all of those.
//!
//! It strips rather than refuses, because the line was written by an agent and not by the person standing at the terminal — refusing the commit would only punish the wrong party.
//! It says what it removed, because a hook that edits your commit message silently is worse than the problem it solves.

/// Does this line claim authorship for an agent?
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
    // Keyed on the address rather than the word "Claude": the display name changes with the model ("Claude Fable 5", "Claude Opus 5 (1M context)"), the address does not, and a human co-author must never match.
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

    // A bare session URL on a line of its own.
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

/// Returns the cleaned message and how many lines were removed.
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

    // Collapse the blank run the trailer block left behind, so the message does not end in stray whitespace.
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
