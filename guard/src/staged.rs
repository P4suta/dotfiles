//! Scans the content staged for commit.
//!
//! `dotguard:allow-foreign` exempts this file, because its tests need real Chinese in a diff.
//!
//! The scan reads only *added* lines, so a gate never fires on existing history near an edit.
//! `git diff --cached` skips binary content, and rename detection keeps a moved file from counting as added lines.

use crate::lang;
use crate::realgit;

pub struct FileHits {
    pub path: String,
    pub hits: Vec<(usize, lang::Hit)>,
}

/// Parse a unified diff with zero context and scan each added line.
///
/// `@@ -a,b +c,d @@` gives the first line number on the new side, and each added line increments it, so a hit reports its line number after the commit.
pub fn scan_diff(diff: &str) -> Vec<FileHits> {
    let mut out: Vec<FileHits> = Vec::new();
    let mut path = String::new();
    let mut lineno = 0usize;
    // File headers appear only before the first hunk of a file, and inside a hunk `+++ ` marks an added line that begins with `++ `.
    let mut in_hunk = false;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            in_hunk = false;
            continue;
        }
        if !in_hunk {
            if let Some(p) = line.strip_prefix("+++ b/") {
                p.clone_into(&mut path);
                continue;
            }
            if line.starts_with("+++ ") || line.starts_with("--- ") {
                continue;
            }
        }
        if let Some(rest) = line.strip_prefix("@@") {
            in_hunk = true;
            lineno = rest
                .split('+')
                .nth(1)
                .and_then(|s| s.split([',', ' ']).next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            continue;
        }
        if let Some(added) = line.strip_prefix('+') {
            for hit in lang::scan(added, lang::Mode::Content) {
                if !out.iter().any(|f| f.path == path) {
                    out.push(FileHits {
                        path: path.clone(),
                        hits: Vec::new(),
                    });
                }
                if let Some(entry) = out.iter_mut().find(|f| f.path == path) {
                    entry.hits.push((lineno, hit));
                }
            }
            lineno += 1;
        }
    }
    out
}

/// A file exempts itself from the content scan by carrying this marker in its header, the first `EXEMPT_SCAN_LINES` lines.
///
/// `lang.rs` and its tests must contain the Chinese text they detect, so the gate needs this exemption to commit itself.
/// The header limit keeps a document that only names the marker from exempting itself.
/// The check reads the staged blob, the content that the commit records.
pub const EXEMPT_MARKER: &str = "dotguard:allow-foreign";
const EXEMPT_SCAN_LINES: usize = 40;

/// Reports whether this text carries the marker in its header.
pub fn exempts_itself(text: &str) -> bool {
    text.lines()
        .take(EXEMPT_SCAN_LINES)
        .any(|l| l.contains(EXEMPT_MARKER))
}

/// Reports whether the staged version of `path` exempts itself.
pub fn is_exempt(path: &str) -> bool {
    realgit::capture(&["show", &format!(":{path}")]).is_some_and(|blob| exempts_itself(&blob))
}

/// The staged diff, or `None` without a repository or staged changes.
pub fn staged_diff() -> Option<String> {
    realgit::capture(&[
        "-c",
        "core.quotePath=false",
        "diff",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--cached",
        "--unified=0",
        "--no-color",
        "--no-ext-diff",
        "--find-renames",
        "--diff-filter=ACMR",
    ])
}

#[cfg(test)]
mod tests {
    use super::{EXEMPT_MARKER, exempts_itself, scan_diff};

    #[test]
    fn the_marker_only_counts_in_the_header() {
        let header = format!("//! {EXEMPT_MARKER}\nfn main() {{}}\n");
        assert!(exempts_itself(&header));

        // A document that names the marker below the header stays in scope.
        let prose = "x\n".repeat(60) + &format!("see `{EXEMPT_MARKER}` in the guard crate\n");
        assert!(!exempts_itself(&prose));
    }

    #[test]
    fn reports_the_post_commit_line_number() {
        let diff = "diff --git a/src/x.rs b/src/x.rs\n\
                    --- a/src/x.rs\n\
                    +++ b/src/x.rs\n\
                    @@ -10,0 +11,2 @@ fn main() {\n\
                    +    // ok\n\
                    +    // 这个\n";
        let files = scan_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/x.rs");
        assert_eq!(files[0].hits.len(), 2); // one hit per character
        assert_eq!(files[0].hits[0].0, 12);
    }

    #[test]
    fn removed_lines_are_not_scanned() {
        let diff =
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-这个\n+ok\n";
        assert!(scan_diff(diff).is_empty());
    }

    #[test]
    fn an_added_line_that_looks_like_a_header_is_still_scanned() {
        let diff = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -0,0 +1,2 @@\n+++ b/这个\n+ok\n";
        let files = scan_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].hits[0].0, 1);
    }
}
