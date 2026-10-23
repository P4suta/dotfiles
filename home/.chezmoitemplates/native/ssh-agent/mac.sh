# shellcheck shell=sh
# OpenSSH selects the authentication agent at each use through the managed SSH include.
GIT_SSH_COMMAND=/usr/bin/ssh
export GIT_SSH_COMMAND
onepassword_agent="${HOME}/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"
if [ -n "${DOTFILES_SSH_AUTH_SOCK:-}" ] && [ -S "${DOTFILES_SSH_AUTH_SOCK}" ]; then
	SSH_AUTH_SOCK="${DOTFILES_SSH_AUTH_SOCK}"
	export SSH_AUTH_SOCK
elif [ -z "${SSH_CONNECTION:-}" ]; then
	SSH_AUTH_SOCK="${onepassword_agent}"
	export SSH_AUTH_SOCK
fi
unset onepassword_agent
