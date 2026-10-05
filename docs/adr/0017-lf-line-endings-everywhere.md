# Normalize line endings to LF on every host and agent write

Status: Accepted.

## Context

This repository normalizes text to LF through its own `.gitattributes`, but other repositories do not, and Windows tools such as jq, PowerShell, and editors write CRLF.
Git converts those bytes only when it touches the file, so tools that read the working tree before a commit see different content on different hosts.

## Decision

Every profile installs `~/.config/git/attributes` with `* text=auto eol=lf` and names it in `core.attributesFile`, so every repository stages LF text by default.
A repository's own attributes still take precedence over the global file.

Claude Code and Codex run `dotfiles-xtask line-endings hook` as a `PostToolUse` hook with no matcher, registered in their machine-local settings by the adapters setup step without reordering existing hook groups.
The Codex hook documentation states that a group without a matcher matches every supported tool, that `PostToolUse` follows Bash, `apply_patch`, MCP, and other local function tools, and that the input carries the session `cwd`.
OpenCode runs the same command from the managed `line-endings` plugin.
The hook converts CRLF to LF in the changed and untracked files of the work tree at the session directory, and in the file the tool names even when it is ignored.
It runs after every tool rather than a list of writing tools, because shell commands and client tools whose names differ between clients also write files.

`git check-attr` decides which paths are exempt: a path whose attributes resolve to `eol=crlf`, `-text`, or `binary` keeps its bytes, and this is the only declared escape hatch.
Explicit `text`, or `eol=lf` without a `text` value, converts every CRLF, as Git does.
Otherwise content that Git's `text=auto` detection treats as binary is kept: a lone carriage return, a NUL byte, or more than one nonprintable byte per 128 printable ones, counted as Git's `gather_stats` counts them.
A file outside every Git work tree is kept, because no attributes can declare an exception for it.
The decision, the byte statistics, and the conversion live in `xtask/src/eol_rules.rs`.
Its Kani harnesses check the decision for every attribute combination and every statistic, the statistics and the binary verdict of every four-byte input against Git's byte classes, and the conversion of each byte of every four-byte input.
Rejecting counterexamples rewrite a declared CRLF path and control-heavy content.
An integration test compares the hook's result with the blob `git hash-object` stores for the same path, including the 128-byte ratio boundary that the four-byte harness cannot reach.

`just check` refuses a tracked text file whose working-tree bytes contain CRLF without such an attribute, and names the command that converts it.

## Alternatives

`core.autocrlf` and `core.eol` alone were rejected: they apply only to files Git already treats as text and leave the working tree unchanged until checkout.
Matching only known writing tools was rejected because Codex and OpenCode name their tools differently and shell commands write files without naming them.
Normalizing every file in the work tree on each call was rejected because unchanged tracked files already hold what Git checked out.

## Consequences

Each tool call spends a `git ls-files` and a `git check-attr` in the session's repository, and reads every modified and untracked file in full.
A replacement checks that the file is writable and still holds the bytes the hook read, keeps its permissions, and skips symbolic links, but another writer can still change the file between that check and the rename.
A read-only file keeps its CRLF and the hook names the command that converts it.
When Git cannot inspect a directory, for example a removed session directory or an untrusted repository owner, the hook reports the cause and a `git -C` command on standard error and exits successfully, so one broken directory does not fail every tool call; `just check` still refuses tracked CRLF.
Hook trust and activation in each client remain native verification, as for the skill observer.
