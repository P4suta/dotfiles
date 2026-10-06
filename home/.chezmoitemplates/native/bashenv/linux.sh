#!/usr/bin/env bash

if [[ -x "${HOME}/.local/bin/mise" ]]; then
  eval "$("${HOME}/.local/bin/mise" activate bash || true)"
fi

export PATH="${HOME}/.local/bin:${HOME}/.nix-profile/bin:${PATH}"

# shellcheck source=dot_config/shell/ssh-agent.sh
[[ -r "${HOME}/.config/shell/ssh-agent.sh" ]] && . "${HOME}/.config/shell/ssh-agent.sh"

if command -v opam >/dev/null 2>&1 && [[ -d "${HOME}/.opam" ]]; then
  eval "$(opam env --shell=bash 2>/dev/null || true)"
fi

if [[ -d "${HOME}/.claude/shims" ]]; then
  export PATH="${HOME}/.claude/shims:${PATH}"
fi
