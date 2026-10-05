# Normalize line endings to LF on every host and agent write

Status: accepted.

## Context

This repository normalizes text to LF through its own `.gitattributes`, but other repositories don't, and Windows tools such as jq, PowerShell, and editors write CRLF.
Git converts those bytes only when it touches the file, so tools that read the working tree before a commit see different content on different hosts.

## Decision

Every profile installs `~/.config/git/attributes` with `* text=auto eol=lf` and names it in `core.attributesFile`, so every repository stages LF text by default.
A repository's own attributes still take precedence over the global file.

Claude Code and Codex run `dotfiles-xtask line-endings hook` as a `PostToolUse` hook with no matcher.
The adapters setup step registers it in their machine-local settings without reordering existing hook groups.
The Codex hook documentation states three facts.
A group without a matcher matches every supported tool.
`PostToolUse` follows Bash, `apply_patch`, MCP, and other local function tools.
The input carries the session `cwd`.
OpenCode runs the same command from the managed `line-endings` plugin.
The hook converts CRLF to LF in the changed and untracked files of the work tree at the session directory.
It also converts the file the tool names, even when Git ignores that file.
It runs after every tool instead of a list of writing tools, because shell commands and client tools whose names differ between clients also write files.

`git check-attr` decides which paths stay exempt.
A path whose attributes resolve to `eol=crlf`, `-text`, or `binary` keeps its bytes, and this exemption stands as the only declared escape hatch.
Explicit `text`, or `eol=lf` without a `text` value, converts every CRLF, as Git does.
Otherwise the hook keeps content that Git's `text=auto` detection treats as binary.
That means a lone carriage return, a NUL byte, or more than one nonprintable byte per 128 printable ones, counted as Git's `gather_stats` counts them.
Content detection also keeps a path whose index entry already holds text with CRLF, which `git ls-files --eol` reports as `i/crlf` or `i/mixed`, because `git add` stages such a file unchanged.
An edit to a repository that commits CRLF without attributes thus stays a change to the edited lines.
`line-endings normalize` and `just check` ignore the index entry, as `git add --renormalize` does, so they still refuse committed CRLF and convert it on request.
The hook keeps a file outside every Git work tree, because no attributes can declare an exception for it.
The decision, the byte statistics, and the conversion live in `xtask/src/eol_rules.rs`.
Its Kani harnesses check the decision for every attribute combination, index state, staging mode, and statistic.
They also compare the statistics and the binary verdict of every four-byte input with Git's byte classes.
They check the conversion of each byte of every four-byte input too.
Rejecting counterexamples rewrite a declared CRLF path, control-heavy content, and CRLF the index keeps.
Integration tests compare the hook's result with the blob Git stages for the same path.
They include the 128-byte ratio boundary that the four-byte harness can't reach and index entries that hold CRLF, mixed, LF, and binary content.

`just check` refuses a tracked text file whose working-tree bytes contain CRLF without such an attribute, and names the command that converts it.

## Alternatives

`core.autocrlf` and `core.eol` alone would apply only to files Git already treats as text and would leave the working tree unchanged until checkout.
Matching only known writing tools would miss Codex and OpenCode tool names and shell commands that write files without naming them.
Normalizing every file in the work tree on each call would reconvert unchanged tracked files that already hold what Git checked out.

## Consequences

Each tool call spends a `git ls-files --eol` and a `git check-attr` in the session's repository, and reads every modified and untracked file and the index blob of every modified file in full.
A replacement checks that the file stays writable and still holds the bytes the hook read.
It keeps permissions and skips symbolic links.
Another writer can still change the file between that check and the rename.
A read-only file keeps its CRLF, and the hook names the command that converts it.
Git can't inspect some directories, for example a removed session directory or an untrusted repository owner.
For those the hook reports the cause and a `git -C` command on standard error and exits successfully, so one broken directory doesn't fail every tool call.
`just check` still refuses tracked CRLF.
Hook trust and activation in each client remain native verification, as for the skill observer.
