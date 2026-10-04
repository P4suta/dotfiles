# shellcheck shell=sh
# Prefer the native Linux 1Password agent for a local desktop session.
# An SSH session keeps the socket injected by sshd, so Windows agent forwarding remains authoritative when connecting over Tailscale/SSH.
onepassword_agent="${HOME}/.1password/agent.sock"
if [ -n "${DOTFILES_SSH_AUTH_SOCK:-}" ] && [ -S "${DOTFILES_SSH_AUTH_SOCK}" ]; then
  SSH_AUTH_SOCK="${DOTFILES_SSH_AUTH_SOCK}"
  export SSH_AUTH_SOCK
elif [ -z "${SSH_CONNECTION:-}" ] && [ -S "${onepassword_agent}" ]; then
  SSH_AUTH_SOCK="${onepassword_agent}"
  export SSH_AUTH_SOCK
fi
unset onepassword_agent
