#!/usr/bin/env bash

if [[ -x "${HOME}/.local/bin/mise" ]]; then
  if shell_env="$("${HOME}/.local/bin/mise" activate bash)"; then
    eval "${shell_env}"
  else
    echo "bashenv: mise activate failed, so mise tools stay off PATH" >&2
  fi
fi

export PATH="${HOME}/.local/bin:${HOME}/.nix-profile/bin:${PATH}"

# shellcheck source=/dev/null
[[ -r "${HOME}/.config/shell/ssh-agent.sh" ]] && . "${HOME}/.config/shell/ssh-agent.sh"

if command -v opam >/dev/null 2>&1 && [[ -d "${HOME}/.opam" ]]; then
  if shell_env="$(opam env --shell=bash 2>/dev/null)"; then
    eval "${shell_env}"
  else
    echo "bashenv: opam env failed, so the opam switch stays off PATH" >&2
  fi
fi
unset shell_env

if [[ -d "${HOME}/.claude/shims" ]]; then
  export PATH="${HOME}/.claude/shims:${PATH}"
fi
