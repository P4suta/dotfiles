// The real target manifest of `home/.chezmoidata` and the rendered Git hooks, which `just gen` writes from `xtask/src/manifest_model.rs`.
module manifest_instance
open manifest_checks[Profile, Path, Target, Retired, Hook] as checks

abstract sig Profile {}
one sig mac, linux, windows, wsl extends Profile {}
abstract sig Path {}
abstract sig Target {}
abstract sig Retired {}
abstract sig Hook {}

// P0 is .agents
// P1 is .agents/skills
// P2 is .agents/skills/orbstack
// P3 is .bashrc
// P4 is .chezmoiscripts
// P5 is .chezmoiscripts/unix
// P6 is .chezmoiscripts/unix/03-nix.sh
// P7 is .chezmoiscripts/unix/04-desktop.sh
// P8 is .chezmoiscripts/unix/07-fcitx5.sh
// P9 is .chezmoiscripts/unix/08-macos-defaults.sh
// P10 is .chezmoiscripts/unix/09-forge-keys.sh
// P11 is .chezmoiscripts/unix/10-claude.sh
// P12 is .chezmoiscripts/unix/11-ocaml.sh
// P13 is .chezmoiscripts/unix/12-haskell.sh
// P14 is .chezmoiscripts/unix/13-racket.sh
// P15 is .chezmoiscripts/unix/14-guard.sh
// P16 is .chezmoiscripts/unix/15-ocomment.sh
// P17 is .chezmoiscripts/unix/17-fleet.sh
// P18 is .chezmoiscripts/unix/18-retire-tmux.sh
// P19 is .claude
// P20 is .claude/bashenv.sh
// P21 is .claude/shims
// P22 is .claude/shims/.keep
// P23 is .claude/skills
// P24 is .claude/skills/orbstack
// P25 is .config
// P26 is .config/aerospace
// P27 is .config/atuin
// P28 is .config/atuin/config.toml
// P29 is .config/bat
// P30 is .config/bat/config
// P31 is .config/borders
// P32 is .config/dev-reaper
// P33 is .config/dev-reaper/config
// P34 is .config/dotfiles
// P35 is .config/dotfiles-nix
// P36 is .config/dotfiles/notifications
// P37 is .config/fcitx5
// P38 is .config/fcitx5/config
// P39 is .config/fcitx5/profile
// P40 is .config/fzf
// P41 is .config/fzf/fzfrc
// P42 is .config/gh
// P43 is .config/gh/config.yml
// P44 is .config/ghostty
// P45 is .config/ghostty/config
// P46 is .config/git
// P47 is .config/git/hooks
// P48 is .config/git/hooks/commit-msg
// P49 is .config/git/hooks/dotguard-delegate.sh
// P50 is .config/git/hooks/lefthook-delegate.sh
// P51 is .config/git/hooks/post-checkout
// P52 is .config/git/hooks/post-commit
// P53 is .config/git/hooks/post-merge
// P54 is .config/git/hooks/pre-commit
// P55 is .config/git/hooks/pre-push
// P56 is .config/git/hooks/reference-transaction
// P57 is .config/git/hooks/renovate-freshness.sh
// P58 is .config/git/hooks/storage-scout-delegate.sh
// P59 is .config/git/hooks/strip-agent-trailers.sh
// P60 is .config/git/template
// P61 is .config/herdr
// P62 is .config/herdr-agent
// P63 is .config/herdr-agent/keys
// P64 is .config/herdr/config.toml
// P65 is .config/homebrew
// P66 is .config/homebrew/Brewfile
// P67 is .config/jj
// P68 is .config/jj/config.toml
// P69 is .config/kitty
// P70 is .config/kitty/kitty.conf
// P71 is .config/kitty/tokyo-night.conf
// P72 is .config/lazygit
// P73 is .config/lazygit/config.yml
// P74 is .config/lefthook
// P75 is .config/lefthook/global.yml
// P76 is .config/mise
// P77 is .config/mise/tasks
// P78 is .config/mise/tasks/git-audit
// P79 is .config/nushell
// P80 is .config/nushell/autoload
// P81 is .config/nushell/autoload/atuin.nu
// P82 is .config/nushell/autoload/carapace.nu
// P83 is .config/nushell/autoload/starship.nu
// P84 is .config/nushell/autoload/zoxide.nu
// P85 is .config/nushell/config.nu
// P86 is .config/nushell/env.nu
// P87 is .config/nushell/login.nu
// P88 is .config/nushell/mise.nu
// P89 is .config/ocomment
// P90 is .config/ocomment/config.toml
// P91 is .config/opencode
// P92 is .config/opencode/agents
// P93 is .config/opencode/agents/reviewer.md
// P94 is .config/opencode/commands
// P95 is .config/opencode/commands/commit.md
// P96 is .config/opencode/commands/pr.md
// P97 is .config/opencode/commands/review.md
// P98 is .config/opencode/plugins
// P99 is .config/opencode/plugins/prose.ts
// P100 is .config/opencode/themes
// P101 is .config/opencode/themes/tokyonight.json
// P102 is .config/opencode/tui.json
// P103 is .config/session-reaper
// P104 is .config/session-reaper/config
// P105 is .config/shell
// P106 is .config/shell/github-token.sh
// P107 is .config/shell/ssh-agent.sh
// P108 is .config/sketchybar
// P109 is .config/storage-scout
// P110 is .config/storage-scout/auto.toml
// P111 is .config/systemd
// P112 is .config/systemd/user
// P113 is .config/systemd/user/dev-cleanup.service
// P114 is .config/systemd/user/dev-cleanup.timer
// P115 is .config/systemd/user/dev-reaper.service
// P116 is .config/systemd/user/dev-reaper.timer
// P117 is .config/systemd/user/docker-prune.service
// P118 is .config/systemd/user/docker-prune.timer
// P119 is .config/systemd/user/dotfiles-desktop-refresh.service
// P120 is .config/systemd/user/dotfiles-desktop-refresh.timer
// P121 is .config/systemd/user/mise-upgrade.service
// P122 is .config/systemd/user/mise-upgrade.timer
// P123 is .config/systemd/user/session-reaper.service
// P124 is .config/systemd/user/session-reaper.timer
// P125 is .config/systemd/user/storage-scout.service
// P126 is .config/systemd/user/tools-upgrade.service
// P127 is .config/systemd/user/tools-upgrade.timer
// P128 is .config/wezterm
// P129 is .config/wezterm/wezterm.lua
// P130 is .config/xdg-terminals.list
// P131 is .gitconfig
// P132 is .local
// P133 is .local/bin
// P134 is .local/bin/dev-reaper
// P135 is .local/bin/dotctl.exe
// P136 is .local/bin/dotfiles-doctor
// P137 is .local/bin/dotfiles-doctor.ps1
// P138 is .local/bin/dotfiles-refresh-desktop
// P139 is .local/bin/dotfiles-refresh-integrations
// P140 is .local/bin/dotfiles-refresh-integrations.ps1
// P141 is .local/bin/dotfiles-retire-tmux
// P142 is .local/bin/dotfiles-touchid
// P143 is .local/bin/dotfiles-wm-defaults
// P144 is .local/bin/dotfiles-wsl-ssh-keygen
// P145 is .local/bin/dotfiles-xtask
// P146 is .local/bin/dotfiles-xtask.exe
// P147 is .local/bin/ghostty-herdr
// P148 is .local/bin/ghostty-nu
// P149 is .local/bin/git
// P150 is .local/bin/kitty-herdr
// P151 is .local/bin/kitty-nu
// P152 is .local/bin/renovate-local
// P153 is .local/bin/session-reaper
// P154 is .local/share
// P155 is .local/share/applications
// P156 is .local/share/applications/kitty-herdr.desktop
// P157 is .local/share/applications/kitty-nu.desktop
// P158 is .profile
// P159 is .zprofile
// P160 is .zshrc
// P161 is AppData
// P162 is AppData/Roaming
// P163 is AppData/Roaming/alacritty
// P164 is AppData/Roaming/alacritty/alacritty.toml
// P165 is AppData/Roaming/herdr
// P166 is AppData/Roaming/herdr/config.toml
// P167 is Documents
// P168 is Documents/PowerShell
// P169 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P170 is Documents/PowerShell/modules
// P171 is Documents/PowerShell/modules/00-readline.ps1
// P172 is Documents/PowerShell/modules/05-mise.ps1
// P173 is Documents/PowerShell/modules/10-tools.ps1
// P174 is Documents/PowerShell/modules/20-herdr.ps1
// P175 is Documents/PowerShell/modules/99-prompt.ps1
// P176 is Library
// P177 is Library/Application Support
// P178 is Library/Application Support/lazygit
// P179 is Library/Application Support/nushell
// P180 is Library/LaunchAgents
// P181 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P182 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P183 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P184 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P185 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P186 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P187 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166, P167, P168, P169, P170, P171, P172, P173, P174, P175, P176, P177, P178, P179, P180, P181, P182, P183, P184, P185, P186, P187 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119, T120, T121, T122, T123, T124, T125, T126, T127, T128, T129, T130, T131, T132, T133, T134 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32, R33 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P17 -> P5 + P18 -> P5 + P20 -> P19 + P21 -> P19 + P22 -> P21 + P23 -> P19 + P24 -> P23 + P26 -> P25 + P27 -> P25 + P28 -> P27 + P29 -> P25 + P30 -> P29 + P31 -> P25 + P32 -> P25 + P33 -> P32 + P34 -> P25 + P35 -> P25 + P36 -> P34 + P37 -> P25 + P38 -> P37 + P39 -> P37 + P40 -> P25 + P41 -> P40 + P42 -> P25 + P43 -> P42 + P44 -> P25 + P45 -> P44 + P46 -> P25 + P47 -> P46 + P48 -> P47 + P49 -> P47 + P50 -> P47 + P51 -> P47 + P52 -> P47 + P53 -> P47 + P54 -> P47 + P55 -> P47 + P56 -> P47 + P57 -> P47 + P58 -> P47 + P59 -> P47 + P60 -> P46 + P61 -> P25 + P62 -> P25 + P63 -> P62 + P64 -> P61 + P65 -> P25 + P66 -> P65 + P67 -> P25 + P68 -> P67 + P69 -> P25 + P70 -> P69 + P71 -> P69 + P72 -> P25 + P73 -> P72 + P74 -> P25 + P75 -> P74 + P76 -> P25 + P77 -> P76 + P78 -> P77 + P79 -> P25 + P80 -> P79 + P81 -> P80 + P82 -> P80 + P83 -> P80 + P84 -> P80 + P85 -> P79 + P86 -> P79 + P87 -> P79 + P88 -> P79 + P89 -> P25 + P90 -> P89 + P91 -> P25 + P92 -> P91 + P93 -> P92 + P94 -> P91 + P95 -> P94 + P96 -> P94 + P97 -> P94 + P98 -> P91 + P99 -> P98 + P100 -> P91 + P101 -> P100 + P102 -> P91 + P103 -> P25 + P104 -> P103 + P105 -> P25 + P106 -> P105 + P107 -> P105 + P108 -> P25 + P109 -> P25 + P110 -> P109 + P111 -> P25 + P112 -> P111 + P113 -> P112 + P114 -> P112 + P115 -> P112 + P116 -> P112 + P117 -> P112 + P118 -> P112 + P119 -> P112 + P120 -> P112 + P121 -> P112 + P122 -> P112 + P123 -> P112 + P124 -> P112 + P125 -> P112 + P126 -> P112 + P127 -> P112 + P128 -> P25 + P129 -> P128 + P130 -> P25 + P133 -> P132 + P134 -> P133 + P135 -> P133 + P136 -> P133 + P137 -> P133 + P138 -> P133 + P139 -> P133 + P140 -> P133 + P141 -> P133 + P142 -> P133 + P143 -> P133 + P144 -> P133 + P145 -> P133 + P146 -> P133 + P147 -> P133 + P148 -> P133 + P149 -> P133 + P150 -> P133 + P151 -> P133 + P152 -> P133 + P153 -> P133 + P154 -> P132 + P155 -> P154 + P156 -> P155 + P157 -> P155 + P162 -> P161 + P163 -> P162 + P164 -> P163 + P165 -> P162 + P166 -> P165 + P168 -> P167 + P169 -> P168 + P170 -> P168 + P171 -> P170 + P172 -> P170 + P173 -> P170 + P174 -> P170 + P175 -> P170 + P177 -> P176 + P178 -> P177 + P179 -> P177 + P180 -> P176 + P181 -> P180 + P182 -> P180 + P183 -> P180 + P184 -> P180 + P185 -> P180 + P186 -> P180 + P187 -> P180 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P17 + T14 -> P18 + T15 -> P20 + T16 -> P21 + T17 -> P22 + T18 -> P24 + T19 -> P27 + T20 -> P28 + T21 -> P29 + T22 -> P30 + T23 -> P32 + T24 -> P33 + T25 -> P35 + T26 -> P36 + T27 -> P37 + T28 -> P38 + T29 -> P39 + T30 -> P40 + T31 -> P41 + T32 -> P42 + T33 -> P43 + T34 -> P44 + T35 -> P45 + T36 -> P61 + T37 -> P62 + T38 -> P63 + T39 -> P64 + T40 -> P65 + T41 -> P66 + T42 -> P67 + T43 -> P68 + T44 -> P69 + T45 -> P70 + T46 -> P71 + T47 -> P72 + T48 -> P73 + T49 -> P74 + T50 -> P75 + T51 -> P77 + T52 -> P78 + T53 -> P79 + T54 -> P85 + T55 -> P86 + T56 -> P87 + T57 -> P89 + T58 -> P90 + T59 -> P92 + T60 -> P93 + T61 -> P94 + T62 -> P95 + T63 -> P96 + T64 -> P97 + T65 -> P100 + T66 -> P101 + T67 -> P102 + T68 -> P103 + T69 -> P104 + T70 -> P105 + T71 -> P107 + T72 -> P109 + T73 -> P110 + T74 -> P111 + T75 -> P113 + T76 -> P114 + T77 -> P115 + T78 -> P116 + T79 -> P117 + T80 -> P118 + T81 -> P119 + T82 -> P120 + T83 -> P121 + T84 -> P122 + T85 -> P123 + T86 -> P124 + T87 -> P125 + T88 -> P126 + T89 -> P127 + T90 -> P128 + T91 -> P129 + T92 -> P130 + T93 -> P134 + T94 -> P136 + T95 -> P137 + T96 -> P138 + T97 -> P139 + T98 -> P140 + T99 -> P141 + T100 -> P142 + T101 -> P144 + T102 -> P147 + T103 -> P148 + T104 -> P149 + T105 -> P150 + T106 -> P151 + T107 -> P152 + T108 -> P153 + T109 -> P154 + T110 -> P156 + T111 -> P157 + T112 -> P158 + T113 -> P159 + T114 -> P160 + T115 -> P161 + T116 -> P166 + T117 -> P167 + T118 -> P169 + T119 -> P176 + T120 -> P178 + T121 -> P179 + T122 -> P181 + T123 -> P182 + T124 -> P183 + T125 -> P185 + T126 -> P186 + T127 -> P187 + T128 -> P48 + T129 -> P51 + T130 -> P52 + T131 -> P53 + T132 -> P54 + T133 -> P55 + T134 -> P56 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (linux) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (mac) + T22 -> (mac) + T23 -> (mac + linux) + T24 -> (mac + linux) + T25 -> (linux) + T26 -> (mac + linux + windows) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun realAt: Retired -> Path { R0 -> P26 + R1 -> P31 + R2 -> P49 + R3 -> P50 + R4 -> P57 + R5 -> P58 + R6 -> P59 + R7 -> P60 + R8 -> P80 + R9 -> P81 + R10 -> P82 + R11 -> P83 + R12 -> P84 + R13 -> P87 + R14 -> P88 + R15 -> P99 + R16 -> P106 + R17 -> P108 + R18 -> P121 + R19 -> P122 + R20 -> P131 + R21 -> P135 + R22 -> P136 + R23 -> P143 + R24 -> P145 + R25 -> P146 + R26 -> P152 + R27 -> P164 + R28 -> P171 + R29 -> P172 + R30 -> P173 + R31 -> P174 + R32 -> P175 + R33 -> P184 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (linux) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (mac) + T22 -> (mac) + T23 -> (mac + linux) + T24 -> (mac + linux) + T25 -> (linux) + T26 -> (mac + linux + windows) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P31 + R2 -> P49 + R3 -> P50 + R4 -> P57 + R5 -> P58 + R6 -> P59 + R7 -> P60 + R8 -> P80 + R9 -> P81 + R10 -> P82 + R11 -> P83 + R12 -> P84 + R13 -> P87 + R14 -> P88 + R15 -> P99 + R16 -> P106 + R17 -> P108 + R18 -> P121 + R19 -> P122 + R20 -> P131 + R21 -> P135 + R22 -> P136 + R23 -> P143 + R24 -> P145 + R25 -> P146 + R26 -> P152 + R27 -> P164 + R28 -> P171 + R29 -> P172 + R30 -> P173 + R31 -> P174 + R32 -> P175 + R33 -> P184 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (linux) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (mac) + T22 -> (mac) + T23 -> (mac + linux) + T24 -> (mac + linux) + T25 -> (linux) + T26 -> (mac + linux + windows) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T128 + H1 -> T128 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedHookAt: Retired -> Path { R0 -> P26 + R1 -> P31 + R2 -> P49 + R3 -> P50 + R4 -> P57 + R5 -> P58 + R6 -> P59 + R7 -> P60 + R8 -> P80 + R9 -> P81 + R10 -> P82 + R11 -> P83 + R12 -> P84 + R13 -> P87 + R14 -> P88 + R15 -> P99 + R16 -> P106 + R17 -> P108 + R18 -> P121 + R19 -> P122 + R20 -> P131 + R21 -> P135 + R22 -> P136 + R23 -> P143 + R24 -> P145 + R25 -> P146 + R26 -> P152 + R27 -> P164 + R28 -> P171 + R29 -> P172 + R30 -> P173 + R31 -> P174 + R32 -> P175 + R33 -> P184 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (linux) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (mac) + T22 -> (mac) + T23 -> (mac + linux) + T24 -> (mac + linux) + T25 -> (linux) + T26 -> (mac + linux + windows) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedMaskAt: Retired -> Path { R0 -> P26 + R1 -> P31 + R2 -> P49 + R3 -> P50 + R4 -> P57 + R5 -> P58 + R6 -> P59 + R7 -> P60 + R8 -> P80 + R9 -> P81 + R10 -> P82 + R11 -> P83 + R12 -> P84 + R13 -> P87 + R14 -> P88 + R15 -> P99 + R16 -> P106 + R17 -> P108 + R18 -> P121 + R19 -> P122 + R20 -> P131 + R21 -> P135 + R22 -> P136 + R23 -> P143 + R24 -> P145 + R25 -> P146 + R26 -> P152 + R27 -> P164 + R28 -> P171 + R29 -> P172 + R30 -> P173 + R31 -> P174 + R32 -> P175 + R33 -> P184 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (linux) + T15 -> (mac + linux + wsl) + T16 -> (Profile) + T17 -> (mac + linux + wsl) + T18 -> (mac) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (mac) + T22 -> (mac) + T23 -> (mac + linux) + T24 -> (mac + linux) + T25 -> (linux) + T26 -> (mac + linux + windows) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedEmptyAt: Retired -> Path { R0 -> P26 + R1 -> P31 + R2 -> P49 + R3 -> P50 + R4 -> P57 + R5 -> P58 + R6 -> P59 + R7 -> P60 + R8 -> P80 + R9 -> P81 + R10 -> P82 + R11 -> P83 + R12 -> P84 + R13 -> P87 + R14 -> P88 + R15 -> P99 + R16 -> P106 + R17 -> P108 + R18 -> P121 + R19 -> P122 + R20 -> P131 + R21 -> P135 + R22 -> P136 + R23 -> P143 + R24 -> P145 + R25 -> P146 + R26 -> P152 + R27 -> P164 + R28 -> P171 + R29 -> P172 + R30 -> P173 + R31 -> P174 + R32 -> P175 + R33 -> P184 }
fun plantedEmptyRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedEmpty { checks/tree[parents] and checks/unique[paths] }

Real_disjoint: check { real implies checks/disjoint[realReceives, paths, parents, realRemoves, realAt] } for 4
Planted_disjoint: check { plantedOverlap implies checks/disjoint[plantedOverlapReceives, paths, parents, plantedOverlapRemoves, plantedOverlapAt] } for 4
Real_hookBijection: check { real implies checks/hookBijection[realFiles] } for 4
Planted_hookBijection: check { plantedHook implies checks/hookBijection[plantedHookFiles] } for 4
Real_uniformHookMask: check { real implies checks/uniformHookMask[realFiles, realReceives] } for 4
Planted_uniformHookMask: check { plantedMask implies checks/uniformHookMask[plantedMaskFiles, plantedMaskReceives] } for 4
Real_noEmptyDirectory: check { real implies checks/noEmptyDirectory[realReceives, paths, parents] } for 4
Planted_noEmptyDirectory: check { plantedEmpty implies checks/noEmptyDirectory[plantedEmptyReceives, paths, parents] } for 4
RealManifest: run { real } for 4
