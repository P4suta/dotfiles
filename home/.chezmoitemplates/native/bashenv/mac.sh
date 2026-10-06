#!/usr/bin/env bash
# Claude Code sources this file through BASH_ENV, so its commands find mise tools by name.

# Shells that skip ~/.zprofile need Homebrew first.
if [[ -x /opt/homebrew/bin/brew ]]; then
  eval "$(/opt/homebrew/bin/brew shellenv || true)"
fi

if [[ -x "${HOME}/.local/bin/mise" ]]; then
  eval "$("${HOME}/.local/bin/mise" activate bash || true)"
fi

# Prepend after mise so the ~/.local/bin git wrapper wins over the shims.
export PATH="${HOME}/.local/bin:${PATH}"

# shellcheck source=dot_config/shell/ssh-agent.sh
[[ -r "${HOME}/.config/shell/ssh-agent.sh" ]] && . "${HOME}/.config/shell/ssh-agent.sh"

# Load the opam switch.
if command -v opam >/dev/null 2>&1 && [[ -d "${HOME}/.opam" ]]; then
  eval "$(opam env --shell=bash 2>/dev/null || true)"
fi

# Shims for Claude Code only.
if [[ -d "${HOME}/.claude/shims" ]]; then
  export PATH="${HOME}/.claude/shims:${PATH}"
fi
