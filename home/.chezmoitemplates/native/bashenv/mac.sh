#!/usr/bin/env bash
# Claude Code sources this file through BASH_ENV, so its commands find mise tools by name.

# Shells that skip ~/.zprofile need Homebrew first.
if [[ -x /opt/homebrew/bin/brew ]]; then
  if shell_env="$(/opt/homebrew/bin/brew shellenv)"; then
    eval "${shell_env}"
  else
    echo "bashenv: brew shellenv failed, so Homebrew tools stay off PATH" >&2
  fi
fi

if [[ -x "${HOME}/.local/bin/mise" ]]; then
  if shell_env="$("${HOME}/.local/bin/mise" activate bash)"; then
    eval "${shell_env}"
  else
    echo "bashenv: mise activate failed, so mise tools stay off PATH" >&2
  fi
fi

# Prepend after mise so the ~/.local/bin git wrapper wins over the shims.
export PATH="${HOME}/.local/bin:${PATH}"

# shellcheck source=/dev/null
[[ -r "${HOME}/.config/shell/ssh-agent.sh" ]] && . "${HOME}/.config/shell/ssh-agent.sh"

# Load the opam switch.
if command -v opam >/dev/null 2>&1 && [[ -d "${HOME}/.opam" ]]; then
  if shell_env="$(opam env --shell=bash 2>/dev/null)"; then
    eval "${shell_env}"
  else
    echo "bashenv: opam env failed, so the opam switch stays off PATH" >&2
  fi
fi
unset shell_env

# Shims for Claude Code only.
if [[ -d "${HOME}/.claude/shims" ]]; then
  export PATH="${HOME}/.claude/shims:${PATH}"
fi
