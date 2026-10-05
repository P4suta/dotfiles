# Normalize line endings to LF on every host and agent write

Status: Accepted.

## Context

This repository normalizes text to LF through its own `.gitattributes`, but other repositories do not, and Windows tools such as jq, PowerShell, and editors write CRLF.
Git converts those bytes only when it touches the file, so tools that read the working tree before a commit see different content on different hosts.

## Decision

Every profile installs `~/.config/git/attributes` with `* text=auto eol=lf` and names it in `core.attributesFile`, so every repository stages LF text by default.
A repository's own attributes still take precedence over the global file.

Claude Code and Codex run `dotfiles-xtask line-endings hook` after every tool call, registered in their machine-local settings by the adapters setup step without reordering existing hook groups.
OpenCode runs the same command from the managed `line-endings` plugin.
The hook converts CRLF to LF in the changed and untracked files of the work tree at the session directory, and in the file the tool names even when it is ignored.
It runs after every tool rather than a list of writing tools, because shell commands and client tools whose names differ between clients also write files.

`git check-attr` decides which paths are exempt: a path whose attributes resolve to `eol=crlf`, `-text`, or `binary` keeps its bytes, and this is the only escape hatch.
Content that Git's `text=auto` detection treats as binary, with a NUL byte or a lone carriage return, is kept unless `text` is set explicitly.
A file outside every Git work tree is kept, because no attributes can declare an exception for it.
The decision and the conversion live in `xtask/src/eol_rules.rs`, whose Kani harnesses check every attribute and content combination and the conversion of each byte of every four-byte input, with a rejecting counterexample for rewriting a declared CRLF path.

`just check` refuses a tracked text file whose working-tree bytes contain CRLF without such an attribute, and names the command that converts it.

## Alternatives

`core.autocrlf` and `core.eol` alone were rejected: they apply only to files Git already treats as text and leave the working tree unchanged until checkout.
Matching only known writing tools was rejected because Codex and OpenCode name their tools differently and shell commands write files without naming them.
Normalizing every file in the work tree on each call was rejected because unchanged tracked files already hold what Git checked out.

## Consequences

Each tool call spends a `git ls-files` and a `git check-attr` in the session's repository.
A replacement checks that the file still holds the bytes the hook read, keeps its permissions, and skips symbolic links, but another writer can still change the file between that check and the rename.
Hook trust and activation in each client remain native verification, as for the skill observer.
