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
// P17 is .chezmoiscripts/unix/16-domyjob.sh
// P18 is .chezmoiscripts/unix/17-fleet.sh
// P19 is .chezmoiscripts/unix/18-retire-tmux.sh
// P20 is .claude
// P21 is .claude/bashenv.sh
// P22 is .claude/shims
// P23 is .claude/shims/.keep
// P24 is .claude/skills
// P25 is .claude/skills/orbstack
// P26 is .config
// P27 is .config/aerospace
// P28 is .config/atuin
// P29 is .config/atuin/config.toml
// P30 is .config/bat
// P31 is .config/bat/config
// P32 is .config/borders
// P33 is .config/dev-reaper
// P34 is .config/dev-reaper/config
// P35 is .config/dotfiles
// P36 is .config/dotfiles-nix
// P37 is .config/dotfiles/notifications
// P38 is .config/fcitx5
// P39 is .config/fcitx5/config
// P40 is .config/fcitx5/profile
// P41 is .config/fzf
// P42 is .config/fzf/fzfrc
// P43 is .config/gh
// P44 is .config/gh/config.yml
// P45 is .config/ghostty
// P46 is .config/ghostty/config
// P47 is .config/git
// P48 is .config/git/hooks
// P49 is .config/git/hooks/commit-msg
// P50 is .config/git/hooks/dotguard-delegate.sh
// P51 is .config/git/hooks/lefthook-delegate.sh
// P52 is .config/git/hooks/post-checkout
// P53 is .config/git/hooks/post-commit
// P54 is .config/git/hooks/post-merge
// P55 is .config/git/hooks/pre-commit
// P56 is .config/git/hooks/pre-push
// P57 is .config/git/hooks/reference-transaction
// P58 is .config/git/hooks/renovate-freshness.sh
// P59 is .config/git/hooks/storage-scout-delegate.sh
// P60 is .config/git/hooks/strip-agent-trailers.sh
// P61 is .config/git/template
// P62 is .config/herdr
// P63 is .config/herdr-agent
// P64 is .config/herdr-agent/keys
// P65 is .config/herdr/config.toml
// P66 is .config/homebrew
// P67 is .config/homebrew/Brewfile
// P68 is .config/jj
// P69 is .config/jj/config.toml
// P70 is .config/kitty
// P71 is .config/kitty/kitty.conf
// P72 is .config/kitty/tokyo-night.conf
// P73 is .config/lazygit
// P74 is .config/lazygit/config.yml
// P75 is .config/lefthook
// P76 is .config/lefthook/global.yml
// P77 is .config/mise
// P78 is .config/mise/tasks
// P79 is .config/mise/tasks/git-audit
// P80 is .config/nushell
// P81 is .config/nushell/autoload
// P82 is .config/nushell/autoload/atuin.nu
// P83 is .config/nushell/autoload/carapace.nu
// P84 is .config/nushell/autoload/starship.nu
// P85 is .config/nushell/autoload/zoxide.nu
// P86 is .config/nushell/config.nu
// P87 is .config/nushell/env.nu
// P88 is .config/nushell/login.nu
// P89 is .config/nushell/mise.nu
// P90 is .config/ocomment
// P91 is .config/ocomment/config.toml
// P92 is .config/opencode
// P93 is .config/opencode/agents
// P94 is .config/opencode/agents/reviewer.md
// P95 is .config/opencode/commands
// P96 is .config/opencode/commands/commit.md
// P97 is .config/opencode/commands/pr.md
// P98 is .config/opencode/commands/review.md
// P99 is .config/opencode/plugins
// P100 is .config/opencode/plugins/prose.ts
// P101 is .config/opencode/themes
// P102 is .config/opencode/themes/tokyonight.json
// P103 is .config/opencode/tui.json
// P104 is .config/session-reaper
// P105 is .config/session-reaper/config
// P106 is .config/shell
// P107 is .config/shell/github-token.sh
// P108 is .config/shell/ssh-agent.sh
// P109 is .config/sketchybar
// P110 is .config/storage-scout
// P111 is .config/storage-scout/auto.toml
// P112 is .config/systemd
// P113 is .config/systemd/user
// P114 is .config/systemd/user/dev-cleanup.service
// P115 is .config/systemd/user/dev-cleanup.timer
// P116 is .config/systemd/user/dev-reaper.service
// P117 is .config/systemd/user/dev-reaper.timer
// P118 is .config/systemd/user/docker-prune.service
// P119 is .config/systemd/user/docker-prune.timer
// P120 is .config/systemd/user/dotfiles-desktop-refresh.service
// P121 is .config/systemd/user/dotfiles-desktop-refresh.timer
// P122 is .config/systemd/user/mise-upgrade.service
// P123 is .config/systemd/user/mise-upgrade.timer
// P124 is .config/systemd/user/session-reaper.service
// P125 is .config/systemd/user/session-reaper.timer
// P126 is .config/systemd/user/storage-scout.service
// P127 is .config/systemd/user/tools-upgrade.service
// P128 is .config/systemd/user/tools-upgrade.timer
// P129 is .config/wezterm
// P130 is .config/wezterm/wezterm.lua
// P131 is .config/xdg-terminals.list
// P132 is .gitconfig
// P133 is .local
// P134 is .local/bin
// P135 is .local/bin/dev-reaper
// P136 is .local/bin/dotctl.exe
// P137 is .local/bin/dotfiles-doctor
// P138 is .local/bin/dotfiles-doctor.ps1
// P139 is .local/bin/dotfiles-refresh-desktop
// P140 is .local/bin/dotfiles-refresh-integrations
// P141 is .local/bin/dotfiles-refresh-integrations.ps1
// P142 is .local/bin/dotfiles-retire-tmux
// P143 is .local/bin/dotfiles-touchid
// P144 is .local/bin/dotfiles-wm-defaults
// P145 is .local/bin/dotfiles-wsl-ssh-keygen
// P146 is .local/bin/dotfiles-xtask
// P147 is .local/bin/dotfiles-xtask.exe
// P148 is .local/bin/ghostty-herdr
// P149 is .local/bin/ghostty-nu
// P150 is .local/bin/git
// P151 is .local/bin/kitty-herdr
// P152 is .local/bin/kitty-nu
// P153 is .local/bin/renovate-local
// P154 is .local/bin/session-reaper
// P155 is .local/share
// P156 is .local/share/applications
// P157 is .local/share/applications/kitty-herdr.desktop
// P158 is .local/share/applications/kitty-nu.desktop
// P159 is .profile
// P160 is .zprofile
// P161 is .zshrc
// P162 is AppData
// P163 is AppData/Roaming
// P164 is AppData/Roaming/alacritty
// P165 is AppData/Roaming/alacritty/alacritty.toml
// P166 is AppData/Roaming/herdr
// P167 is AppData/Roaming/herdr/config.toml
// P168 is Documents
// P169 is Documents/PowerShell
// P170 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P171 is Documents/PowerShell/modules
// P172 is Documents/PowerShell/modules/00-readline.ps1
// P173 is Documents/PowerShell/modules/05-mise.ps1
// P174 is Documents/PowerShell/modules/10-tools.ps1
// P175 is Documents/PowerShell/modules/20-herdr.ps1
// P176 is Documents/PowerShell/modules/99-prompt.ps1
// P177 is Library
// P178 is Library/Application Support
// P179 is Library/Application Support/lazygit
// P180 is Library/Application Support/nushell
// P181 is Library/LaunchAgents
// P182 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P183 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P184 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P185 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P186 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P187 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P188 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166, P167, P168, P169, P170, P171, P172, P173, P174, P175, P176, P177, P178, P179, P180, P181, P182, P183, P184, P185, P186, P187, P188 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119, T120, T121, T122, T123, T124, T125, T126, T127, T128, T129, T130, T131, T132, T133, T134, T135 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32, R33 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P17 -> P5 + P18 -> P5 + P19 -> P5 + P21 -> P20 + P22 -> P20 + P23 -> P22 + P24 -> P20 + P25 -> P24 + P27 -> P26 + P28 -> P26 + P29 -> P28 + P30 -> P26 + P31 -> P30 + P32 -> P26 + P33 -> P26 + P34 -> P33 + P35 -> P26 + P36 -> P26 + P37 -> P35 + P38 -> P26 + P39 -> P38 + P40 -> P38 + P41 -> P26 + P42 -> P41 + P43 -> P26 + P44 -> P43 + P45 -> P26 + P46 -> P45 + P47 -> P26 + P48 -> P47 + P49 -> P48 + P50 -> P48 + P51 -> P48 + P52 -> P48 + P53 -> P48 + P54 -> P48 + P55 -> P48 + P56 -> P48 + P57 -> P48 + P58 -> P48 + P59 -> P48 + P60 -> P48 + P61 -> P47 + P62 -> P26 + P63 -> P26 + P64 -> P63 + P65 -> P62 + P66 -> P26 + P67 -> P66 + P68 -> P26 + P69 -> P68 + P70 -> P26 + P71 -> P70 + P72 -> P70 + P73 -> P26 + P74 -> P73 + P75 -> P26 + P76 -> P75 + P77 -> P26 + P78 -> P77 + P79 -> P78 + P80 -> P26 + P81 -> P80 + P82 -> P81 + P83 -> P81 + P84 -> P81 + P85 -> P81 + P86 -> P80 + P87 -> P80 + P88 -> P80 + P89 -> P80 + P90 -> P26 + P91 -> P90 + P92 -> P26 + P93 -> P92 + P94 -> P93 + P95 -> P92 + P96 -> P95 + P97 -> P95 + P98 -> P95 + P99 -> P92 + P100 -> P99 + P101 -> P92 + P102 -> P101 + P103 -> P92 + P104 -> P26 + P105 -> P104 + P106 -> P26 + P107 -> P106 + P108 -> P106 + P109 -> P26 + P110 -> P26 + P111 -> P110 + P112 -> P26 + P113 -> P112 + P114 -> P113 + P115 -> P113 + P116 -> P113 + P117 -> P113 + P118 -> P113 + P119 -> P113 + P120 -> P113 + P121 -> P113 + P122 -> P113 + P123 -> P113 + P124 -> P113 + P125 -> P113 + P126 -> P113 + P127 -> P113 + P128 -> P113 + P129 -> P26 + P130 -> P129 + P131 -> P26 + P134 -> P133 + P135 -> P134 + P136 -> P134 + P137 -> P134 + P138 -> P134 + P139 -> P134 + P140 -> P134 + P141 -> P134 + P142 -> P134 + P143 -> P134 + P144 -> P134 + P145 -> P134 + P146 -> P134 + P147 -> P134 + P148 -> P134 + P149 -> P134 + P150 -> P134 + P151 -> P134 + P152 -> P134 + P153 -> P134 + P154 -> P134 + P155 -> P133 + P156 -> P155 + P157 -> P156 + P158 -> P156 + P163 -> P162 + P164 -> P163 + P165 -> P164 + P166 -> P163 + P167 -> P166 + P169 -> P168 + P170 -> P169 + P171 -> P169 + P172 -> P171 + P173 -> P171 + P174 -> P171 + P175 -> P171 + P176 -> P171 + P178 -> P177 + P179 -> P178 + P180 -> P178 + P181 -> P177 + P182 -> P181 + P183 -> P181 + P184 -> P181 + P185 -> P181 + P186 -> P181 + P187 -> P181 + P188 -> P181 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P17 + T14 -> P18 + T15 -> P19 + T16 -> P21 + T17 -> P22 + T18 -> P23 + T19 -> P25 + T20 -> P28 + T21 -> P29 + T22 -> P30 + T23 -> P31 + T24 -> P33 + T25 -> P34 + T26 -> P36 + T27 -> P37 + T28 -> P38 + T29 -> P39 + T30 -> P40 + T31 -> P41 + T32 -> P42 + T33 -> P43 + T34 -> P44 + T35 -> P45 + T36 -> P46 + T37 -> P62 + T38 -> P63 + T39 -> P64 + T40 -> P65 + T41 -> P66 + T42 -> P67 + T43 -> P68 + T44 -> P69 + T45 -> P70 + T46 -> P71 + T47 -> P72 + T48 -> P73 + T49 -> P74 + T50 -> P75 + T51 -> P76 + T52 -> P78 + T53 -> P79 + T54 -> P80 + T55 -> P86 + T56 -> P87 + T57 -> P88 + T58 -> P90 + T59 -> P91 + T60 -> P93 + T61 -> P94 + T62 -> P95 + T63 -> P96 + T64 -> P97 + T65 -> P98 + T66 -> P101 + T67 -> P102 + T68 -> P103 + T69 -> P104 + T70 -> P105 + T71 -> P106 + T72 -> P108 + T73 -> P110 + T74 -> P111 + T75 -> P112 + T76 -> P114 + T77 -> P115 + T78 -> P116 + T79 -> P117 + T80 -> P118 + T81 -> P119 + T82 -> P120 + T83 -> P121 + T84 -> P122 + T85 -> P123 + T86 -> P124 + T87 -> P125 + T88 -> P126 + T89 -> P127 + T90 -> P128 + T91 -> P129 + T92 -> P130 + T93 -> P131 + T94 -> P135 + T95 -> P137 + T96 -> P138 + T97 -> P139 + T98 -> P140 + T99 -> P141 + T100 -> P142 + T101 -> P143 + T102 -> P145 + T103 -> P148 + T104 -> P149 + T105 -> P150 + T106 -> P151 + T107 -> P152 + T108 -> P153 + T109 -> P154 + T110 -> P155 + T111 -> P157 + T112 -> P158 + T113 -> P159 + T114 -> P160 + T115 -> P161 + T116 -> P162 + T117 -> P167 + T118 -> P168 + T119 -> P170 + T120 -> P177 + T121 -> P179 + T122 -> P180 + T123 -> P182 + T124 -> P183 + T125 -> P184 + T126 -> P186 + T127 -> P187 + T128 -> P188 + T129 -> P49 + T130 -> P52 + T131 -> P53 + T132 -> P54 + T133 -> P55 + T134 -> P56 + T135 -> P57 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (mac + linux + windows) + T28 -> (linux) + T29 -> (linux) + T30 -> (linux) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux + wsl) + T34 -> (mac + linux + wsl) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux) + T38 -> (mac) + T39 -> (mac) + T40 -> (mac + linux) + T41 -> (mac) + T42 -> (mac) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (linux) + T46 -> (linux) + T47 -> (linux) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux) + T73 -> (mac + linux + windows) + T74 -> (mac + linux + windows) + T75 -> (linux + wsl) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (wsl) + T85 -> (wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (windows) + T92 -> (windows) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (windows) + T97 -> (mac + linux) + T98 -> (mac + linux) + T99 -> (windows) + T100 -> (linux) + T101 -> (mac) + T102 -> (wsl) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac + linux + wsl) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (linux) + T114 -> (mac) + T115 -> (mac) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (windows) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) + T135 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T129 + H1 -> T130 + H2 -> T131 + H3 -> T132 + H4 -> T133 + H5 -> T134 + H6 -> T135 }
fun realAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P50 + R3 -> P51 + R4 -> P58 + R5 -> P59 + R6 -> P60 + R7 -> P61 + R8 -> P81 + R9 -> P82 + R10 -> P83 + R11 -> P84 + R12 -> P85 + R13 -> P88 + R14 -> P89 + R15 -> P100 + R16 -> P107 + R17 -> P109 + R18 -> P122 + R19 -> P123 + R20 -> P132 + R21 -> P136 + R22 -> P137 + R23 -> P144 + R24 -> P146 + R25 -> P147 + R26 -> P153 + R27 -> P165 + R28 -> P172 + R29 -> P173 + R30 -> P174 + R31 -> P175 + R32 -> P176 + R33 -> P185 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (mac + linux + windows) + T28 -> (linux) + T29 -> (linux) + T30 -> (linux) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux + wsl) + T34 -> (mac + linux + wsl) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux) + T38 -> (mac) + T39 -> (mac) + T40 -> (mac + linux) + T41 -> (mac) + T42 -> (mac) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (linux) + T46 -> (linux) + T47 -> (linux) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux) + T73 -> (mac + linux + windows) + T74 -> (mac + linux + windows) + T75 -> (linux + wsl) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (wsl) + T85 -> (wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (windows) + T92 -> (windows) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (windows) + T97 -> (mac + linux) + T98 -> (mac + linux) + T99 -> (windows) + T100 -> (linux) + T101 -> (mac) + T102 -> (wsl) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac + linux + wsl) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (linux) + T114 -> (mac) + T115 -> (mac) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (windows) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) + T135 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T129 + H1 -> T130 + H2 -> T131 + H3 -> T132 + H4 -> T133 + H5 -> T134 + H6 -> T135 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P32 + R2 -> P50 + R3 -> P51 + R4 -> P58 + R5 -> P59 + R6 -> P60 + R7 -> P61 + R8 -> P81 + R9 -> P82 + R10 -> P83 + R11 -> P84 + R12 -> P85 + R13 -> P88 + R14 -> P89 + R15 -> P100 + R16 -> P107 + R17 -> P109 + R18 -> P122 + R19 -> P123 + R20 -> P132 + R21 -> P136 + R22 -> P137 + R23 -> P144 + R24 -> P146 + R25 -> P147 + R26 -> P153 + R27 -> P165 + R28 -> P172 + R29 -> P173 + R30 -> P174 + R31 -> P175 + R32 -> P176 + R33 -> P185 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (mac + linux + windows) + T28 -> (linux) + T29 -> (linux) + T30 -> (linux) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux + wsl) + T34 -> (mac + linux + wsl) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux) + T38 -> (mac) + T39 -> (mac) + T40 -> (mac + linux) + T41 -> (mac) + T42 -> (mac) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (linux) + T46 -> (linux) + T47 -> (linux) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux) + T73 -> (mac + linux + windows) + T74 -> (mac + linux + windows) + T75 -> (linux + wsl) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (wsl) + T85 -> (wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (windows) + T92 -> (windows) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (windows) + T97 -> (mac + linux) + T98 -> (mac + linux) + T99 -> (windows) + T100 -> (linux) + T101 -> (mac) + T102 -> (wsl) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac + linux + wsl) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (linux) + T114 -> (mac) + T115 -> (mac) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (windows) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) + T135 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T129 + H1 -> T129 + H2 -> T131 + H3 -> T132 + H4 -> T133 + H5 -> T134 + H6 -> T135 }
fun plantedHookAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P50 + R3 -> P51 + R4 -> P58 + R5 -> P59 + R6 -> P60 + R7 -> P61 + R8 -> P81 + R9 -> P82 + R10 -> P83 + R11 -> P84 + R12 -> P85 + R13 -> P88 + R14 -> P89 + R15 -> P100 + R16 -> P107 + R17 -> P109 + R18 -> P122 + R19 -> P123 + R20 -> P132 + R21 -> P136 + R22 -> P137 + R23 -> P144 + R24 -> P146 + R25 -> P147 + R26 -> P153 + R27 -> P165 + R28 -> P172 + R29 -> P173 + R30 -> P174 + R31 -> P175 + R32 -> P176 + R33 -> P185 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (mac + linux + windows) + T28 -> (linux) + T29 -> (linux) + T30 -> (linux) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux + wsl) + T34 -> (mac + linux + wsl) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux) + T38 -> (mac) + T39 -> (mac) + T40 -> (mac + linux) + T41 -> (mac) + T42 -> (mac) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (linux) + T46 -> (linux) + T47 -> (linux) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux) + T73 -> (mac + linux + windows) + T74 -> (mac + linux + windows) + T75 -> (linux + wsl) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (wsl) + T85 -> (wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (windows) + T92 -> (windows) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (windows) + T97 -> (mac + linux) + T98 -> (mac + linux) + T99 -> (windows) + T100 -> (linux) + T101 -> (mac) + T102 -> (wsl) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac + linux + wsl) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (linux) + T114 -> (mac) + T115 -> (mac) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (windows) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac) + T129 -> (mac) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) + T135 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T129 + H1 -> T130 + H2 -> T131 + H3 -> T132 + H4 -> T133 + H5 -> T134 + H6 -> T135 }
fun plantedMaskAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P50 + R3 -> P51 + R4 -> P58 + R5 -> P59 + R6 -> P60 + R7 -> P61 + R8 -> P81 + R9 -> P82 + R10 -> P83 + R11 -> P84 + R12 -> P85 + R13 -> P88 + R14 -> P89 + R15 -> P100 + R16 -> P107 + R17 -> P109 + R18 -> P122 + R19 -> P123 + R20 -> P132 + R21 -> P136 + R22 -> P137 + R23 -> P144 + R24 -> P146 + R25 -> P147 + R26 -> P153 + R27 -> P165 + R28 -> P172 + R29 -> P173 + R30 -> P174 + R31 -> P175 + R32 -> P176 + R33 -> P185 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (Profile) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (mac + linux + windows) + T28 -> (linux) + T29 -> (linux) + T30 -> (linux) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux + wsl) + T34 -> (mac + linux + wsl) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux) + T38 -> (mac) + T39 -> (mac) + T40 -> (mac + linux) + T41 -> (mac) + T42 -> (mac) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (linux) + T46 -> (linux) + T47 -> (linux) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux) + T73 -> (mac + linux + windows) + T74 -> (mac + linux + windows) + T75 -> (linux + wsl) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (wsl) + T85 -> (wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (windows) + T92 -> (windows) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (windows) + T97 -> (mac + linux) + T98 -> (mac + linux) + T99 -> (windows) + T100 -> (linux) + T101 -> (mac) + T102 -> (wsl) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac + linux + wsl) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (linux) + T114 -> (mac) + T115 -> (mac) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (windows) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) + T135 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T129 + H1 -> T130 + H2 -> T131 + H3 -> T132 + H4 -> T133 + H5 -> T134 + H6 -> T135 }
fun plantedEmptyAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P50 + R3 -> P51 + R4 -> P58 + R5 -> P59 + R6 -> P60 + R7 -> P61 + R8 -> P81 + R9 -> P82 + R10 -> P83 + R11 -> P84 + R12 -> P85 + R13 -> P88 + R14 -> P89 + R15 -> P100 + R16 -> P107 + R17 -> P109 + R18 -> P122 + R19 -> P123 + R20 -> P132 + R21 -> P136 + R22 -> P137 + R23 -> P144 + R24 -> P146 + R25 -> P147 + R26 -> P153 + R27 -> P165 + R28 -> P172 + R29 -> P173 + R30 -> P174 + R31 -> P175 + R32 -> P176 + R33 -> P185 }
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
