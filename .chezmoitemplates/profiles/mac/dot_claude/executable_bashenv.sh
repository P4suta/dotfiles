#!/usr/bin/env bash
# Claude Code's noninteractive Bash sources this file through BASH_ENV, so its commands find mise-managed tools by name.

# Noninteractive shells skip ~/.zprofile, so add Homebrew first.
if [[ -x /opt/homebrew/bin/brew ]]; then
  eval "$(/opt/homebrew/bin/brew shellenv || true)"
fi

if [[ -x "${HOME}/.local/bin/mise" ]]; then
  eval "$("${HOME}/.local/bin/mise" activate bash || true)"
fi

# Prepend after mise so ~/.local/bin, including the git wrapper, wins over the shims.
export PATH="${HOME}/.local/bin:${PATH}"

# shellcheck source=dot_config/shell/ssh-agent.sh
[[ -r "${HOME}/.config/shell/ssh-agent.sh" ]] && . "${HOME}/.config/shell/ssh-agent.sh"

# Load the default or directory-local opam switch for OCaml commands.
if command -v opam >/dev/null 2>&1 && [[ -d "${HOME}/.opam" ]]; then
  eval "$(opam env --shell=bash 2>/dev/null || true)"
fi

# Shims for Claude Code only, at ~/.claude/shims/<name>.
if [[ -d "${HOME}/.claude/shims" ]]; then
  export PATH="${HOME}/.claude/shims:${PATH}"
fi
