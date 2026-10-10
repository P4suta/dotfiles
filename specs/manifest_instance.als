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
// P35 is .config/dotfiles-nix
// P36 is .config/fcitx5
// P37 is .config/fcitx5/config
// P38 is .config/fcitx5/profile
// P39 is .config/fzf
// P40 is .config/fzf/fzfrc
// P41 is .config/gh
// P42 is .config/gh/config.yml
// P43 is .config/ghostty
// P44 is .config/ghostty/config
// P45 is .config/git
// P46 is .config/git/hooks
// P47 is .config/git/hooks/commit-msg
// P48 is .config/git/hooks/dotguard-delegate.sh
// P49 is .config/git/hooks/lefthook-delegate.sh
// P50 is .config/git/hooks/post-checkout
// P51 is .config/git/hooks/post-commit
// P52 is .config/git/hooks/post-merge
// P53 is .config/git/hooks/pre-commit
// P54 is .config/git/hooks/pre-push
// P55 is .config/git/hooks/reference-transaction
// P56 is .config/git/hooks/renovate-freshness.sh
// P57 is .config/git/hooks/storage-scout-delegate.sh
// P58 is .config/git/hooks/strip-agent-trailers.sh
// P59 is .config/git/template
// P60 is .config/herdr
// P61 is .config/herdr-agent
// P62 is .config/herdr-agent/keys
// P63 is .config/herdr/config.toml
// P64 is .config/homebrew
// P65 is .config/homebrew/Brewfile
// P66 is .config/jj
// P67 is .config/jj/config.toml
// P68 is .config/kitty
// P69 is .config/kitty/kitty.conf
// P70 is .config/kitty/tokyo-night.conf
// P71 is .config/lazygit
// P72 is .config/lazygit/config.yml
// P73 is .config/lefthook
// P74 is .config/lefthook/global.yml
// P75 is .config/mise
// P76 is .config/mise/tasks
// P77 is .config/mise/tasks/git-audit
// P78 is .config/nushell
// P79 is .config/nushell/autoload
// P80 is .config/nushell/autoload/atuin.nu
// P81 is .config/nushell/autoload/carapace.nu
// P82 is .config/nushell/autoload/starship.nu
// P83 is .config/nushell/autoload/zoxide.nu
// P84 is .config/nushell/config.nu
// P85 is .config/nushell/env.nu
// P86 is .config/nushell/login.nu
// P87 is .config/nushell/mise.nu
// P88 is .config/ocomment
// P89 is .config/ocomment/config.toml
// P90 is .config/opencode
// P91 is .config/opencode/agents
// P92 is .config/opencode/agents/reviewer.md
// P93 is .config/opencode/commands
// P94 is .config/opencode/commands/commit.md
// P95 is .config/opencode/commands/pr.md
// P96 is .config/opencode/commands/review.md
// P97 is .config/opencode/plugins
// P98 is .config/opencode/plugins/prose.ts
// P99 is .config/opencode/themes
// P100 is .config/opencode/themes/tokyonight.json
// P101 is .config/opencode/tui.json
// P102 is .config/session-reaper
// P103 is .config/session-reaper/config
// P104 is .config/shell
// P105 is .config/shell/github-token.sh
// P106 is .config/shell/ssh-agent.sh
// P107 is .config/sketchybar
// P108 is .config/storage-scout
// P109 is .config/storage-scout/auto.toml
// P110 is .config/systemd
// P111 is .config/systemd/user
// P112 is .config/systemd/user/dev-cleanup.service
// P113 is .config/systemd/user/dev-cleanup.timer
// P114 is .config/systemd/user/dev-reaper.service
// P115 is .config/systemd/user/dev-reaper.timer
// P116 is .config/systemd/user/docker-prune.service
// P117 is .config/systemd/user/docker-prune.timer
// P118 is .config/systemd/user/dotfiles-desktop-refresh.service
// P119 is .config/systemd/user/dotfiles-desktop-refresh.timer
// P120 is .config/systemd/user/mise-upgrade.service
// P121 is .config/systemd/user/mise-upgrade.timer
// P122 is .config/systemd/user/session-reaper.service
// P123 is .config/systemd/user/session-reaper.timer
// P124 is .config/systemd/user/storage-scout.service
// P125 is .config/systemd/user/tools-upgrade.service
// P126 is .config/systemd/user/tools-upgrade.timer
// P127 is .config/wezterm
// P128 is .config/wezterm/wezterm.lua
// P129 is .config/xdg-terminals.list
// P130 is .gitconfig
// P131 is .local
// P132 is .local/bin
// P133 is .local/bin/dev-reaper
// P134 is .local/bin/dotctl.exe
// P135 is .local/bin/dotfiles-doctor
// P136 is .local/bin/dotfiles-doctor.ps1
// P137 is .local/bin/dotfiles-refresh-desktop
// P138 is .local/bin/dotfiles-refresh-integrations
// P139 is .local/bin/dotfiles-refresh-integrations.ps1
// P140 is .local/bin/dotfiles-retire-tmux
// P141 is .local/bin/dotfiles-touchid
// P142 is .local/bin/dotfiles-wm-defaults
// P143 is .local/bin/dotfiles-wsl-ssh-keygen
// P144 is .local/bin/dotfiles-xtask
// P145 is .local/bin/dotfiles-xtask.exe
// P146 is .local/bin/ghostty-herdr
// P147 is .local/bin/ghostty-nu
// P148 is .local/bin/git
// P149 is .local/bin/kitty-herdr
// P150 is .local/bin/kitty-nu
// P151 is .local/bin/renovate-local
// P152 is .local/bin/session-reaper
// P153 is .local/share
// P154 is .local/share/applications
// P155 is .local/share/applications/kitty-herdr.desktop
// P156 is .local/share/applications/kitty-nu.desktop
// P157 is .profile
// P158 is .zprofile
// P159 is .zshrc
// P160 is AppData
// P161 is AppData/Roaming
// P162 is AppData/Roaming/alacritty
// P163 is AppData/Roaming/alacritty/alacritty.toml
// P164 is AppData/Roaming/herdr
// P165 is AppData/Roaming/herdr/config.toml
// P166 is Documents
// P167 is Documents/PowerShell
// P168 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P169 is Documents/PowerShell/modules
// P170 is Documents/PowerShell/modules/00-readline.ps1
// P171 is Documents/PowerShell/modules/05-mise.ps1
// P172 is Documents/PowerShell/modules/10-tools.ps1
// P173 is Documents/PowerShell/modules/20-herdr.ps1
// P174 is Documents/PowerShell/modules/99-prompt.ps1
// P175 is Library
// P176 is Library/Application Support
// P177 is Library/Application Support/lazygit
// P178 is Library/Application Support/nushell
// P179 is Library/LaunchAgents
// P180 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P181 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P182 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P183 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P184 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P185 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P186 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166, P167, P168, P169, P170, P171, P172, P173, P174, P175, P176, P177, P178, P179, P180, P181, P182, P183, P184, P185, P186 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119, T120, T121, T122, T123, T124, T125, T126, T127, T128, T129, T130, T131, T132, T133, T134 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32, R33 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P17 -> P5 + P18 -> P5 + P19 -> P5 + P21 -> P20 + P22 -> P20 + P23 -> P22 + P24 -> P20 + P25 -> P24 + P27 -> P26 + P28 -> P26 + P29 -> P28 + P30 -> P26 + P31 -> P30 + P32 -> P26 + P33 -> P26 + P34 -> P33 + P35 -> P26 + P36 -> P26 + P37 -> P36 + P38 -> P36 + P39 -> P26 + P40 -> P39 + P41 -> P26 + P42 -> P41 + P43 -> P26 + P44 -> P43 + P45 -> P26 + P46 -> P45 + P47 -> P46 + P48 -> P46 + P49 -> P46 + P50 -> P46 + P51 -> P46 + P52 -> P46 + P53 -> P46 + P54 -> P46 + P55 -> P46 + P56 -> P46 + P57 -> P46 + P58 -> P46 + P59 -> P45 + P60 -> P26 + P61 -> P26 + P62 -> P61 + P63 -> P60 + P64 -> P26 + P65 -> P64 + P66 -> P26 + P67 -> P66 + P68 -> P26 + P69 -> P68 + P70 -> P68 + P71 -> P26 + P72 -> P71 + P73 -> P26 + P74 -> P73 + P75 -> P26 + P76 -> P75 + P77 -> P76 + P78 -> P26 + P79 -> P78 + P80 -> P79 + P81 -> P79 + P82 -> P79 + P83 -> P79 + P84 -> P78 + P85 -> P78 + P86 -> P78 + P87 -> P78 + P88 -> P26 + P89 -> P88 + P90 -> P26 + P91 -> P90 + P92 -> P91 + P93 -> P90 + P94 -> P93 + P95 -> P93 + P96 -> P93 + P97 -> P90 + P98 -> P97 + P99 -> P90 + P100 -> P99 + P101 -> P90 + P102 -> P26 + P103 -> P102 + P104 -> P26 + P105 -> P104 + P106 -> P104 + P107 -> P26 + P108 -> P26 + P109 -> P108 + P110 -> P26 + P111 -> P110 + P112 -> P111 + P113 -> P111 + P114 -> P111 + P115 -> P111 + P116 -> P111 + P117 -> P111 + P118 -> P111 + P119 -> P111 + P120 -> P111 + P121 -> P111 + P122 -> P111 + P123 -> P111 + P124 -> P111 + P125 -> P111 + P126 -> P111 + P127 -> P26 + P128 -> P127 + P129 -> P26 + P132 -> P131 + P133 -> P132 + P134 -> P132 + P135 -> P132 + P136 -> P132 + P137 -> P132 + P138 -> P132 + P139 -> P132 + P140 -> P132 + P141 -> P132 + P142 -> P132 + P143 -> P132 + P144 -> P132 + P145 -> P132 + P146 -> P132 + P147 -> P132 + P148 -> P132 + P149 -> P132 + P150 -> P132 + P151 -> P132 + P152 -> P132 + P153 -> P131 + P154 -> P153 + P155 -> P154 + P156 -> P154 + P161 -> P160 + P162 -> P161 + P163 -> P162 + P164 -> P161 + P165 -> P164 + P167 -> P166 + P168 -> P167 + P169 -> P167 + P170 -> P169 + P171 -> P169 + P172 -> P169 + P173 -> P169 + P174 -> P169 + P176 -> P175 + P177 -> P176 + P178 -> P176 + P179 -> P175 + P180 -> P179 + P181 -> P179 + P182 -> P179 + P183 -> P179 + P184 -> P179 + P185 -> P179 + P186 -> P179 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P17 + T14 -> P18 + T15 -> P19 + T16 -> P21 + T17 -> P22 + T18 -> P23 + T19 -> P25 + T20 -> P28 + T21 -> P29 + T22 -> P30 + T23 -> P31 + T24 -> P33 + T25 -> P34 + T26 -> P35 + T27 -> P36 + T28 -> P37 + T29 -> P38 + T30 -> P39 + T31 -> P40 + T32 -> P41 + T33 -> P42 + T34 -> P43 + T35 -> P44 + T36 -> P60 + T37 -> P61 + T38 -> P62 + T39 -> P63 + T40 -> P64 + T41 -> P65 + T42 -> P66 + T43 -> P67 + T44 -> P68 + T45 -> P69 + T46 -> P70 + T47 -> P71 + T48 -> P72 + T49 -> P73 + T50 -> P74 + T51 -> P76 + T52 -> P77 + T53 -> P78 + T54 -> P84 + T55 -> P85 + T56 -> P86 + T57 -> P88 + T58 -> P89 + T59 -> P91 + T60 -> P92 + T61 -> P93 + T62 -> P94 + T63 -> P95 + T64 -> P96 + T65 -> P99 + T66 -> P100 + T67 -> P101 + T68 -> P102 + T69 -> P103 + T70 -> P104 + T71 -> P106 + T72 -> P108 + T73 -> P109 + T74 -> P110 + T75 -> P112 + T76 -> P113 + T77 -> P114 + T78 -> P115 + T79 -> P116 + T80 -> P117 + T81 -> P118 + T82 -> P119 + T83 -> P120 + T84 -> P121 + T85 -> P122 + T86 -> P123 + T87 -> P124 + T88 -> P125 + T89 -> P126 + T90 -> P127 + T91 -> P128 + T92 -> P129 + T93 -> P133 + T94 -> P135 + T95 -> P136 + T96 -> P137 + T97 -> P138 + T98 -> P139 + T99 -> P140 + T100 -> P141 + T101 -> P143 + T102 -> P146 + T103 -> P147 + T104 -> P148 + T105 -> P149 + T106 -> P150 + T107 -> P151 + T108 -> P152 + T109 -> P153 + T110 -> P155 + T111 -> P156 + T112 -> P157 + T113 -> P158 + T114 -> P159 + T115 -> P160 + T116 -> P165 + T117 -> P166 + T118 -> P168 + T119 -> P175 + T120 -> P177 + T121 -> P178 + T122 -> P180 + T123 -> P181 + T124 -> P182 + T125 -> P184 + T126 -> P185 + T127 -> P186 + T128 -> P47 + T129 -> P50 + T130 -> P51 + T131 -> P52 + T132 -> P53 + T133 -> P54 + T134 -> P55 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun realAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P79 + R9 -> P80 + R10 -> P81 + R11 -> P82 + R12 -> P83 + R13 -> P86 + R14 -> P87 + R15 -> P98 + R16 -> P105 + R17 -> P107 + R18 -> P120 + R19 -> P121 + R20 -> P130 + R21 -> P134 + R22 -> P135 + R23 -> P142 + R24 -> P144 + R25 -> P145 + R26 -> P151 + R27 -> P163 + R28 -> P170 + R29 -> P171 + R30 -> P172 + R31 -> P173 + R32 -> P174 + R33 -> P183 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P32 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P79 + R9 -> P80 + R10 -> P81 + R11 -> P82 + R12 -> P83 + R13 -> P86 + R14 -> P87 + R15 -> P98 + R16 -> P105 + R17 -> P107 + R18 -> P120 + R19 -> P121 + R20 -> P130 + R21 -> P134 + R22 -> P135 + R23 -> P142 + R24 -> P144 + R25 -> P145 + R26 -> P151 + R27 -> P163 + R28 -> P170 + R29 -> P171 + R30 -> P172 + R31 -> P173 + R32 -> P174 + R33 -> P183 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T128 + H1 -> T128 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedHookAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P79 + R9 -> P80 + R10 -> P81 + R11 -> P82 + R12 -> P83 + R13 -> P86 + R14 -> P87 + R15 -> P98 + R16 -> P105 + R17 -> P107 + R18 -> P120 + R19 -> P121 + R20 -> P130 + R21 -> P134 + R22 -> P135 + R23 -> P142 + R24 -> P144 + R25 -> P145 + R26 -> P151 + R27 -> P163 + R28 -> P170 + R29 -> P171 + R30 -> P172 + R31 -> P173 + R32 -> P174 + R33 -> P183 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedMaskAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P79 + R9 -> P80 + R10 -> P81 + R11 -> P82 + R12 -> P83 + R13 -> P86 + R14 -> P87 + R15 -> P98 + R16 -> P105 + R17 -> P107 + R18 -> P120 + R19 -> P121 + R20 -> P130 + R21 -> P134 + R22 -> P135 + R23 -> P142 + R24 -> P144 + R25 -> P145 + R26 -> P151 + R27 -> P163 + R28 -> P170 + R29 -> P171 + R30 -> P172 + R31 -> P173 + R32 -> P174 + R33 -> P183 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac) + R24 -> (mac + linux + wsl) + R25 -> (windows) + R26 -> (mac) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac) + T14 -> (mac) + T15 -> (linux) + T16 -> (mac + linux + wsl) + T17 -> (Profile) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac + linux + wsl) + T21 -> (mac + linux + wsl) + T22 -> (mac) + T23 -> (mac) + T24 -> (mac + linux) + T25 -> (mac + linux) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux + wsl) + T33 -> (mac + linux + wsl) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (linux) + T45 -> (linux) + T46 -> (linux) + T47 -> (mac) + T48 -> (mac) + T49 -> (mac) + T50 -> (mac) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac) + T57 -> (mac + linux + windows) + T58 -> (mac + linux + windows) + T59 -> (mac) + T60 -> (mac) + T61 -> (mac) + T62 -> (mac) + T63 -> (mac) + T64 -> (mac) + T65 -> (mac) + T66 -> (mac) + T67 -> (mac) + T68 -> (mac + linux) + T69 -> (mac + linux) + T70 -> (mac + linux) + T71 -> (mac + linux) + T72 -> (mac + linux + windows) + T73 -> (mac + linux + windows) + T74 -> (linux + wsl) + T75 -> (linux) + T76 -> (linux) + T77 -> (linux) + T78 -> (linux) + T79 -> (linux) + T80 -> (linux) + T81 -> (linux) + T82 -> (linux) + T83 -> (wsl) + T84 -> (wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (windows) + T91 -> (windows) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (windows) + T96 -> (mac + linux) + T97 -> (mac + linux) + T98 -> (windows) + T99 -> (linux) + T100 -> (mac) + T101 -> (wsl) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + wsl) + T105 -> (linux) + T106 -> (linux) + T107 -> (linux) + T108 -> (linux) + T109 -> (linux) + T110 -> (linux) + T111 -> (linux) + T112 -> (linux) + T113 -> (mac) + T114 -> (mac) + T115 -> (windows) + T116 -> (windows) + T117 -> (windows) + T118 -> (windows) + T119 -> (mac) + T120 -> (mac) + T121 -> (mac) + T122 -> (mac) + T123 -> (mac) + T124 -> (mac) + T125 -> (mac) + T126 -> (mac) + T127 -> (mac) + T128 -> (mac + linux + windows + wsl) + T129 -> (mac + linux + windows + wsl) + T130 -> (mac + linux + windows + wsl) + T131 -> (mac + linux + windows + wsl) + T132 -> (mac + linux + windows + wsl) + T133 -> (mac + linux + windows + wsl) + T134 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T128 + H1 -> T129 + H2 -> T130 + H3 -> T131 + H4 -> T132 + H5 -> T133 + H6 -> T134 }
fun plantedEmptyAt: Retired -> Path { R0 -> P27 + R1 -> P32 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P79 + R9 -> P80 + R10 -> P81 + R11 -> P82 + R12 -> P83 + R13 -> P86 + R14 -> P87 + R15 -> P98 + R16 -> P105 + R17 -> P107 + R18 -> P120 + R19 -> P121 + R20 -> P130 + R21 -> P134 + R22 -> P135 + R23 -> P142 + R24 -> P144 + R25 -> P145 + R26 -> P151 + R27 -> P163 + R28 -> P170 + R29 -> P171 + R30 -> P172 + R31 -> P173 + R32 -> P174 + R33 -> P183 }
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
