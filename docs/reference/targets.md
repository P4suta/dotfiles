# Profile targets

`just gen` writes this page from `home/.chezmoidata/`.
A target missing from the first table reaches every profile.

| Target | Set | Profiles |
| --- | --- | --- |
| `.agents/skills/orbstack` | mac | mac |
| `.bashrc` | unix | mac, linux, wsl |
| `.chezmoiscripts/unix/**` | unix | mac, linux, wsl |
| `.chezmoiscripts/windows/**` | windows | windows |
| `.claude/bashenv.sh` | unix | mac, linux, wsl |
| `.claude/shims` | unix | mac, linux, wsl |
| `.claude/shims/.keep` | unix | mac, linux, wsl |
| `.claude/skills/orbstack` | mac | mac |
| `.config/atuin` | unix | mac, linux, wsl |
| `.config/atuin/config.toml` | unix | mac, linux, wsl |
| `.config/bat` | mac | mac |
| `.config/bat/config` | mac | mac |
| `.config/dev-reaper` | unix | mac, linux, wsl |
| `.config/dev-reaper/config` | unix | mac, linux, wsl |
| `.config/dotfiles-nix` | linux | linux |
| `.config/fcitx5` | linux | linux |
| `.config/fcitx5/config` | linux | linux |
| `.config/fcitx5/profile` | linux | linux |
| `.config/fzf` | mac | mac |
| `.config/fzf/fzfrc` | mac | mac |
| `.config/gh` | unix | mac, linux, wsl |
| `.config/gh/config.yml` | unix | mac, linux, wsl |
| `.config/ghostty` | mac | mac |
| `.config/ghostty/config` | mac | mac |
| `.config/herdr` | desktop | mac, linux |
| `.config/herdr-agent` | mac | mac |
| `.config/herdr-agent/keys` | mac | mac |
| `.config/herdr/config.toml` | desktop | mac, linux |
| `.config/homebrew` | mac | mac |
| `.config/homebrew/Brewfile` | mac | mac |
| `.config/jj` | unix | mac, linux, wsl |
| `.config/jj/config.toml` | unix | mac, linux, wsl |
| `.config/kitty` | linux | linux |
| `.config/kitty/kitty.conf` | linux | linux |
| `.config/kitty/tokyo-night.conf` | linux | linux |
| `.config/lazygit` | mac | mac |
| `.config/lazygit/config.yml` | mac | mac |
| `.config/mise/tasks` | unix | mac, linux, wsl |
| `.config/mise/tasks/git-audit` | unix | mac, linux, wsl |
| `.config/nushell` | desktop | mac, linux |
| `.config/nushell/config.nu` | desktop | mac, linux |
| `.config/nushell/env.nu` | desktop | mac, linux |
| `.config/nushell/login.nu` | mac | mac |
| `.config/session-reaper` | unix | mac, linux, wsl |
| `.config/session-reaper/config` | unix | mac, linux, wsl |
| `.config/shell` | desktop | mac, linux |
| `.config/shell/ssh-agent.sh` | desktop | mac, linux |
| `.config/storage-scout` | native | mac, linux, windows |
| `.config/storage-scout/auto.toml` | native | mac, linux, windows |
| `.config/systemd` | systemd | linux, wsl |
| `.config/systemd/user/dev-cleanup.service` | systemd | linux, wsl |
| `.config/systemd/user/dev-cleanup.timer` | systemd | linux, wsl |
| `.config/systemd/user/dev-reaper.service` | systemd | linux, wsl |
| `.config/systemd/user/dev-reaper.timer` | systemd | linux, wsl |
| `.config/systemd/user/dotfiles-desktop-refresh.service` | linux | linux |
| `.config/systemd/user/dotfiles-desktop-refresh.timer` | linux | linux |
| `.config/systemd/user/session-reaper.service` | systemd | linux, wsl |
| `.config/systemd/user/session-reaper.timer` | systemd | linux, wsl |
| `.config/systemd/user/storage-scout.service` | linux | linux |
| `.config/systemd/user/tools-upgrade.service` | systemd | linux, wsl |
| `.config/systemd/user/tools-upgrade.timer` | systemd | linux, wsl |
| `.config/wezterm` | windows | windows |
| `.config/wezterm/wezterm.lua` | windows | windows |
| `.config/xdg-terminals.list` | linux | linux |
| `.local/bin/dev-reaper` | systemd | linux, wsl |
| `.local/bin/dotfiles-doctor` | unix | mac, linux, wsl |
| `.local/bin/dotfiles-doctor.ps1` | windows | windows |
| `.local/bin/dotfiles-refresh-desktop` | desktop | mac, linux |
| `.local/bin/dotfiles-refresh-integrations` | desktop | mac, linux |
| `.local/bin/dotfiles-refresh-integrations.ps1` | windows | windows |
| `.local/bin/dotfiles-retire-tmux` | linux | linux |
| `.local/bin/dotfiles-touchid` | mac | mac |
| `.local/bin/dotfiles-wsl-ssh-keygen` | wsl | wsl |
| `.local/bin/ghostty-herdr` | mac | mac |
| `.local/bin/ghostty-nu` | mac | mac |
| `.local/bin/git` | unix | mac, linux, wsl |
| `.local/bin/kitty-herdr` | linux | linux |
| `.local/bin/kitty-nu` | linux | linux |
| `.local/bin/renovate-local` | linux | linux |
| `.local/bin/session-reaper` | systemd | linux, wsl |
| `.local/share` | linux | linux |
| `.local/share/applications/kitty-herdr.desktop` | linux | linux |
| `.local/share/applications/kitty-nu.desktop` | linux | linux |
| `.profile` | linux | linux |
| `.zprofile` | mac | mac |
| `.zshrc` | mac | mac |
| `AppData` | windows | windows |
| `AppData/Roaming/herdr/config.toml` | windows | windows |
| `Documents` | windows | windows |
| `Documents/PowerShell/Microsoft.PowerShell_profile.ps1` | windows | windows |
| `Library` | mac | mac |
| `Library/Application Support/lazygit` | mac | mac |
| `Library/Application Support/nushell` | mac | mac |
| `Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist` | mac | mac |
| `Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist` | mac | mac |
| `Library/LaunchAgents/dev.dotfiles.dev-reaper.plist` | mac | mac |
| `Library/LaunchAgents/dev.dotfiles.session-reaper.plist` | mac | mac |
| `Library/LaunchAgents/dev.dotfiles.storage-scout.plist` | mac | mac |
| `Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist` | mac | mac |

## Setup launchers

`just gen` writes each launcher from its setup step, and only the profiles of its set run it.

| Target | Set | Profiles |
| --- | --- | --- |
| `.chezmoiscripts/unix/03-nix.sh` | linux | linux |
| `.chezmoiscripts/unix/04-desktop.sh` | desktop | mac, linux |
| `.chezmoiscripts/unix/07-fcitx5.sh` | linux | linux |
| `.chezmoiscripts/unix/08-macos-defaults.sh` | mac | mac |
| `.chezmoiscripts/unix/09-forge-keys.sh` | mac | mac |
| `.chezmoiscripts/unix/11-ocaml.sh` | desktop | mac, linux |
| `.chezmoiscripts/unix/12-haskell.sh` | mac | mac |
| `.chezmoiscripts/unix/13-racket.sh` | mac | mac |
| `.chezmoiscripts/unix/14-guard.sh` | mac | mac |
| `.chezmoiscripts/unix/17-fleet.sh` | mac | mac |
| `.chezmoiscripts/unix/18-retire-tmux.sh` | linux | linux |

## Retired targets

Each profile in the set removes the target when it applies.

| Target | Set | Profiles |
| --- | --- | --- |
| `.config/aerospace` | mac | mac |
| `.config/borders` | mac | mac |
| `.config/git/hooks/dotguard-delegate.sh` | unix | mac, linux, wsl |
| `.config/git/hooks/lefthook-delegate.sh` | unix | mac, linux, wsl |
| `.config/git/hooks/renovate-freshness.sh` | unix | mac, linux, wsl |
| `.config/git/hooks/storage-scout-delegate.sh` | unix | mac, linux, wsl |
| `.config/git/hooks/strip-agent-trailers.sh` | mac | mac |
| `.config/git/template` | windows | windows |
| `.config/nushell/autoload` | linux | linux |
| `.config/nushell/autoload/atuin.nu` | mac | mac |
| `.config/nushell/autoload/carapace.nu` | mac | mac |
| `.config/nushell/autoload/starship.nu` | mac | mac |
| `.config/nushell/autoload/zoxide.nu` | mac | mac |
| `.config/nushell/login.nu` | linux | linux |
| `.config/nushell/mise.nu` | desktop | mac, linux |
| `.config/opencode/plugins/prose.ts` | unix | mac, linux, wsl |
| `.config/shell/github-token.sh` | unix | mac, linux, wsl |
| `.config/sketchybar` | mac | mac |
| `.config/systemd/user/docker-prune.service` | linux | linux |
| `.config/systemd/user/docker-prune.timer` | linux | linux |
| `.config/systemd/user/mise-upgrade.service` | systemd | linux, wsl |
| `.config/systemd/user/mise-upgrade.timer` | systemd | linux, wsl |
| `.gitconfig` | unix | mac, linux, wsl |
| `.local/bin/dotctl.exe` | windows | windows |
| `.local/bin/dotfiles-wm-defaults` | mac | mac |
| `.local/bin/dotfiles-xtask` | unix | mac, linux, wsl |
| `.local/bin/dotfiles-xtask.exe` | windows | windows |
| `.local/bin/renovate-local` | mac | mac |
| `AppData/Roaming/alacritty/alacritty.toml` | windows | windows |
| `Documents/PowerShell/modules/00-readline.ps1` | windows | windows |
| `Documents/PowerShell/modules/05-mise.ps1` | windows | windows |
| `Documents/PowerShell/modules/10-tools.ps1` | windows | windows |
| `Documents/PowerShell/modules/20-herdr.ps1` | windows | windows |
| `Documents/PowerShell/modules/99-prompt.ps1` | windows | windows |
| `Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist` | mac | mac |
