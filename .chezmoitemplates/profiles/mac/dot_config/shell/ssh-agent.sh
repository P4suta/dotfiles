# shellcheck shell=sh
# Select the SSH agent that holds the commit-signing key.
#
# Local Mac session : the 1Password app publishes a stable socket inside its signed app-group container.
# That path is the macOS counterpart of ~/.1password/agent.sock on Linux.
# Remote SSH session: SSH_CONNECTION is set, so sshd's forwarded socket is already authoritative and must not be overwritten.
# Herdr pane        : herdr-agent's socket, but only while `herdr-agent ready` says it holds keys, so an unloaded or locked agent falls back to 1Password.
# GIT_SSH_COMMAND points git's ssh at it too, because ~/.ssh/config pins IdentityAgent to 1Password for every host and beats SSH_AUTH_SOCK.
#
# Enable it once in 1Password: Settings -> Developer -> Use the SSH agent.
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
