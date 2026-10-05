#!/usr/bin/env bash
# Bash init for Claude Code, which non-interactive shells source through BASH_ENV.
# Commands from the Claude Code Bash tool reach mise-managed CLIs such as rg, fd, tokei, and hyperfine by their real names.
# The interactive shell in ~/.bashrc stays unaffected.

# mise activate puts every tool shim on PATH.
if [ -x "$HOME/.local/bin/mise" ]; then
  eval "$("$HOME/.local/bin/mise" activate bash)"
fi

# User-installed binaries go first, after mise activate, so they take precedence over mise shims.
# The ~/.local/bin/git wrapper, which refuses options such as --no-verify, then applies to the Claude Code Bash tool.
# Interactive shells get the same export from ~/.bashrc.
export PATH="$HOME/.local/bin:$PATH"

# The Claude Code shim directory joins PATH even when empty, and interactive shells never see it.
# Put a script at ~/.claude/shims/<name> when a shim becomes necessary.
if [ -d "$HOME/.claude/shims" ]; then
  export PATH="$HOME/.claude/shims:$PATH"
fi
