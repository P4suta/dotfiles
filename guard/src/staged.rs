//! Scanning what is about to be committed.
//!
//! dotguard:allow-foreign — the tests below need real Chinese in a diff to assert that a diff carrying it is refused.
//!
//! Only *added* lines are examined.
//! Scanning whole files would mean a gate that fires on someone else's history the first time you touch a file near it, which is how a gate gets a `--no-verify` habit built around it.
//! The diff is also where the cheap wins are: `git diff --cached` already skips binary content, and rename detection keeps a moved file from being re-read as 3,000 added lines.

use crate::lang;
use crate::realgit;

pub struct FileHits {
    pub path: String,
    pub hits: Vec<(usize, lang::Hit)>,
}

/// Parse a unified diff with zero context and scan each added line.
///
/// `@@ -a,b +c,d @@` gives the first line number on the new side, which is incremented per added line so a hit reports the line number the file will actually have after the commit.
pub fn scan_diff(diff: &str) -> Vec<FileHits> {
    let mut out: Vec<FileHits> = Vec::new();
    let mut path = String::new();
    let mut lineno = 0usize;

    for line in diff.lines() {
        if let Some(p) = line.strip_prefix("+++ b/") {
            p.clone_into(&mut path);
            continue;
        }
        if line.starts_with("+++ ") || line.starts_with("--- ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@") {
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

/// A file may exempt itself from the content scan by carrying this marker anywhere in it, the way a linter's own fixtures exempt themselves.
///
/// It exists because this crate cannot be written without it: `lang.rs` has to contain the simplified-Chinese characters it detects, and its tests have to contain Chinese sentences to assert that they are caught.
/// A gate that cannot be committed is not a gate.
///
/// Only honoured in the file's header — the first `EXEMPT_SCAN_LINES` lines — the way a shellcheck directive or a license header is.
/// That rule is not decoration: without it a README that merely *names* the marker would exempt itself from the scan, which is the one thing an opt-out must never do by accident.
/// In a header it can only be deliberate.
///
/// Checked against the staged blob rather than the working tree, so the answer is about the content actually being committed.
pub const EXEMPT_MARKER: &str = "dotguard:allow-foreign";
const EXEMPT_SCAN_LINES: usize = 40;

/// Does this text carry the marker in its header?
pub fn exempts_itself(text: &str) -> bool {
    text.lines()
        .take(EXEMPT_SCAN_LINES)
        .any(|l| l.contains(EXEMPT_MARKER))
}

/// Does the staged version of `path` exempt itself?
pub fn is_exempt(path: &str) -> bool {
    realgit::capture(&["show", &format!(":{path}")]).is_some_and(|blob| exempts_itself(&blob))
}

/// The staged diff, or `None` when there is no repository / nothing staged.
pub fn staged_diff() -> Option<String> {
    realgit::capture(&[
        "diff",
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

        // A document that merely names the marker, far enough down to be prose rather than a directive, must not exempt itself.
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
        assert_eq!(files[0].hits.len(), 2); // 这 and 个
        assert_eq!(files[0].hits[0].0, 12);
    }

    #[test]
    fn removed_lines_are_not_scanned() {
        let diff = "+++ b/a.txt\n@@ -1 +1 @@\n-这个\n+ok\n";
        assert!(scan_diff(diff).is_empty());
    }
}
