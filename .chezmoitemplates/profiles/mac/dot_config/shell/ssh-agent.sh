# shellcheck shell=sh
# Select the socket that holds the commit-signing key.
# A local session uses the 1Password socket in its app-group container.
# A remote SSH session keeps the socket that sshd forwards.
# A Herdr pane uses the Herdr socket while `herdr-agent ready` reports loaded keys, and falls back to 1Password otherwise.
# GIT_SSH_COMMAND also points Git there, because ~/.ssh/config pins IdentityAgent to 1Password for every host.
# Turn on the 1Password setting under Developer once.
onepassword_agent="${HOME}/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"
herdr_agent=""
if [ -n "${HERDR_ENV:-}" ] && [ -z "${SSH_CONNECTION:-}" ] && [ -x "${HOME}/.local/bin/herdr-agent" ]; then
  herdr_agent="$("${HOME}/.local/bin/herdr-agent" ready 2>/dev/null || true)"
fi
if [ -n "${DOTFILES_SSH_AUTH_SOCK:-}" ] && [ -S "${DOTFILES_SSH_AUTH_SOCK}" ]; then
  SSH_AUTH_SOCK="${DOTFILES_SSH_AUTH_SOCK}"
  export SSH_AUTH_SOCK
elif [ -n "${herdr_agent}" ]; then
  SSH_AUTH_SOCK="${herdr_agent}"
  GIT_SSH_COMMAND="ssh -o IdentityAgent=${herdr_agent}"
  export SSH_AUTH_SOCK GIT_SSH_COMMAND
elif [ -z "${SSH_CONNECTION:-}" ] && [ -S "${onepassword_agent}" ]; then
  SSH_AUTH_SOCK="${onepassword_agent}"
  export SSH_AUTH_SOCK
fi
unset onepassword_agent herdr_agent
