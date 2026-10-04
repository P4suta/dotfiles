#!/usr/bin/env bash
# bash init for Claude Code only, sourced by non-interactive shells via BASH_ENV.
# Purpose: let commands issued through the Bash tool call mise-managed CLIs (rg, fd, tokei, hyperfine, ...)
# by their real name.
# The user's own interactive shell (~/.zshrc) is unaffected.

# Homebrew first: a non-interactive shell never passed through ~/.zprofile, so /opt/homebrew is not on PATH yet and `docker`, `borders`, `ffmpeg` would all look missing to the agent.
if [[ -x /opt/homebrew/bin/brew ]]; then
  eval "$(/opt/homebrew/bin/brew shellenv || true)"
fi

# mise activate — put every tool's shim on PATH
if [[ -x "${HOME}/.local/bin/mise" ]]; then
  eval "$("${HOME}/.local/bin/mise" activate bash || true)"
fi

# Prepended *after* mise activate so user-installed binaries win over the shims.
# Required for the ~/.local/bin/git wrapper (which rejects --no-verify and friends) to actually take effect from the Bash tool.
# The interactive shell gets the same export from ~/.zshrc.
export PATH="${HOME}/.local/bin:${PATH}"

# Claude's non-interactive bash may not have sourced ~/.zprofile.
# Select the 1Password app-group agent locally while preserving a forwarded SSH agent.
# shellcheck source=dot_config/shell/ssh-agent.sh
[[ -r "${HOME}/.config/shell/ssh-agent.sh" ]] && . "${HOME}/.config/shell/ssh-agent.sh"

# Non-interactive agent shells do not source ~/.zshrc.
# Import the default or directory-local opam switch explicitly so OCaml Platform commands are usable.
if command -v opam >/dev/null 2>&1 && [[ -d "${HOME}/.opam" ]]; then
  eval "$(opam env --shell=bash 2>/dev/null || true)"
fi

# Shim dir for Claude Code only, never on the user's interactive PATH.
# Added even when empty; drop a script at ~/.claude/shims/<name> when one is needed.
if [[ -d "${HOME}/.claude/shims" ]]; then
  export PATH="${HOME}/.claude/shims:${PATH}"
fi
