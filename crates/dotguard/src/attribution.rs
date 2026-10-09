//! Strips AI attribution lines from a commit message.
//!
//! `guard_rules::is_attribution` decides which lines credit an agent, anchored to trailer position, so human `Co-authored-by` trailers and prose that mentions CLAUDE.md survive.
//! The hook strips the lines instead of refusing the commit, and reports the count it removed.

use crate::guard_rules::{is_attribution, strip_attribution};

/// The first attribution line of a document that the gate refuses rather than edits, as its one-based line number and text.
///
/// The gates refuse pull request, issue, and comment bodies instead of stripping them, so the sender gets the refusal and deletes the line.
/// `xtask` shares this rule, so a commit message, a PR document, and a `gh` body judge a line identically.
pub fn find(text: &str) -> Option<(usize, &str)> {
    text.lines()
        .enumerate()
        .find(|(_, line)| is_attribution(line.as_bytes()))
        .map(|(index, line)| (index + 1, line))
}

/// Returns the cleaned message and the count of removed lines.
pub fn strip(text: &str) -> (String, usize) {
    strip_attribution(text)
}

#[cfg(test)]
mod tests {
    use super::{find, strip};

    #[test]
    fn the_first_attribution_line_is_found_with_its_number() {
        let body = "Body.\n\nCloses #1\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\nhttps://claude.ai/code/session_01Qesu\n";
        assert_eq!(
            find(body),
            Some((
                5,
                "🤖 Generated with [Claude Code](https://claude.com/claude-code)"
            ))
        );
        assert_eq!(
            find("Co-authored-by: Example User <example@example.com>\nClaude is mentioned here.\n"),
            None
        );
        assert_eq!(find(""), None);
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
