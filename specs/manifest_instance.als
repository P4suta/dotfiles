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
// P16 is .chezmoiscripts/unix/17-fleet.sh
// P17 is .chezmoiscripts/unix/18-retire-tmux.sh
// P18 is .claude
// P19 is .claude/bashenv.sh
// P20 is .claude/shims
// P21 is .claude/shims/.keep
// P22 is .claude/skills
// P23 is .claude/skills/orbstack
// P24 is .config
// P25 is .config/aerospace
// P26 is .config/atuin
// P27 is .config/atuin/config.toml
// P28 is .config/borders
// P29 is .config/dev-reaper
// P30 is .config/dev-reaper/config
// P31 is .config/dotfiles
// P32 is .config/dotfiles-nix
// P33 is .config/dotfiles/notifications
// P34 is .config/fcitx5
// P35 is .config/fcitx5/config
// P36 is .config/fcitx5/profile
// P37 is .config/gh
// P38 is .config/gh/config.yml
// P39 is .config/ghostty
// P40 is .config/ghostty/config
// P41 is .config/git
// P42 is .config/git/hooks
// P43 is .config/git/hooks/commit-msg
// P44 is .config/git/hooks/dotguard-delegate.sh
// P45 is .config/git/hooks/lefthook-delegate.sh
// P46 is .config/git/hooks/post-checkout
// P47 is .config/git/hooks/post-commit
// P48 is .config/git/hooks/post-merge
// P49 is .config/git/hooks/pre-commit
// P50 is .config/git/hooks/pre-push
// P51 is .config/git/hooks/reference-transaction
// P52 is .config/git/hooks/renovate-freshness.sh
// P53 is .config/git/hooks/storage-scout-delegate.sh
// P54 is .config/git/hooks/strip-agent-trailers.sh
// P55 is .config/git/template
// P56 is .config/herdr
// P57 is .config/herdr-agent
// P58 is .config/herdr-agent/keys
// P59 is .config/herdr/config.toml
// P60 is .config/homebrew
// P61 is .config/homebrew/Brewfile
// P62 is .config/jj
// P63 is .config/jj/config.toml
// P64 is .config/kitty
// P65 is .config/kitty/kitty.conf
// P66 is .config/kitty/tokyo-night.conf
// P67 is .config/lazygit
// P68 is .config/lazygit/config.yml
// P69 is .config/mise
// P70 is .config/mise/tasks
// P71 is .config/mise/tasks/git-audit
// P72 is .config/nushell
// P73 is .config/nushell/autoload
// P74 is .config/nushell/autoload/atuin.nu
// P75 is .config/nushell/autoload/carapace.nu
// P76 is .config/nushell/autoload/starship.nu
// P77 is .config/nushell/autoload/zoxide.nu
// P78 is .config/nushell/config.nu
// P79 is .config/nushell/env.nu
// P80 is .config/nushell/login.nu
// P81 is .config/nushell/mise.nu
// P82 is .config/opencode
// P83 is .config/opencode/plugins
// P84 is .config/opencode/plugins/prose.ts
// P85 is .config/session-reaper
// P86 is .config/session-reaper/config
// P87 is .config/shell
// P88 is .config/shell/github-token.sh
// P89 is .config/shell/ssh-agent.sh
// P90 is .config/sketchybar
// P91 is .config/storage-scout
// P92 is .config/storage-scout/auto.toml
// P93 is .config/systemd
// P94 is .config/systemd/user
// P95 is .config/systemd/user/dev-cleanup.service
// P96 is .config/systemd/user/dev-cleanup.timer
// P97 is .config/systemd/user/dev-reaper.service
// P98 is .config/systemd/user/dev-reaper.timer
// P99 is .config/systemd/user/docker-prune.service
// P100 is .config/systemd/user/docker-prune.timer
// P101 is .config/systemd/user/dotfiles-desktop-refresh.service
// P102 is .config/systemd/user/dotfiles-desktop-refresh.timer
// P103 is .config/systemd/user/mise-upgrade.service
// P104 is .config/systemd/user/mise-upgrade.timer
// P105 is .config/systemd/user/session-reaper.service
// P106 is .config/systemd/user/session-reaper.timer
// P107 is .config/systemd/user/storage-scout.service
// P108 is .config/systemd/user/tools-upgrade.service
// P109 is .config/systemd/user/tools-upgrade.timer
// P110 is .config/wezterm
// P111 is .config/wezterm/wezterm.lua
// P112 is .config/xdg-terminals.list
// P113 is .gitconfig
// P114 is .local
// P115 is .local/bin
// P116 is .local/bin/dev-reaper
// P117 is .local/bin/dotctl.exe
// P118 is .local/bin/dotfiles-doctor
// P119 is .local/bin/dotfiles-doctor.ps1
// P120 is .local/bin/dotfiles-refresh-desktop
// P121 is .local/bin/dotfiles-refresh-integrations
// P122 is .local/bin/dotfiles-refresh-integrations.ps1
// P123 is .local/bin/dotfiles-retire-tmux
// P124 is .local/bin/dotfiles-touchid
// P125 is .local/bin/dotfiles-wm-defaults
// P126 is .local/bin/dotfiles-wsl-ssh-keygen
// P127 is .local/bin/dotfiles-xtask
// P128 is .local/bin/dotfiles-xtask.exe
// P129 is .local/bin/ghostty-herdr
// P130 is .local/bin/ghostty-nu
// P131 is .local/bin/git
// P132 is .local/bin/kitty-herdr
// P133 is .local/bin/kitty-nu
// P134 is .local/bin/renovate-local
// P135 is .local/bin/session-reaper
// P136 is .local/share
// P137 is .local/share/applications
// P138 is .local/share/applications/kitty-herdr.desktop
// P139 is .local/share/applications/kitty-nu.desktop
// P140 is .profile
// P141 is .zprofile
// P142 is .zshrc
// P143 is AppData
// P144 is AppData/Roaming
// P145 is AppData/Roaming/alacritty
// P146 is AppData/Roaming/alacritty/alacritty.toml
// P147 is AppData/Roaming/herdr
// P148 is AppData/Roaming/herdr/config.toml
// P149 is Documents
// P150 is Documents/PowerShell
// P151 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P152 is Documents/PowerShell/modules
// P153 is Documents/PowerShell/modules/00-readline.ps1
// P154 is Documents/PowerShell/modules/05-mise.ps1
// P155 is Documents/PowerShell/modules/10-tools.ps1
// P156 is Documents/PowerShell/modules/20-herdr.ps1
// P157 is Documents/PowerShell/modules/99-prompt.ps1
// P158 is Library
// P159 is Library/Application Support
// P160 is Library/Application Support/lazygit
// P161 is Library/Application Support/nushell
// P162 is Library/LaunchAgents
// P163 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P164 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P165 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P166 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P167 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P168 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P169 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166, P167, P168, P169 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110, T111, T112, T113, T114, T115, T116 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P17 -> P5 + P19 -> P18 + P20 -> P18 + P21 -> P20 + P22 -> P18 + P23 -> P22 + P25 -> P24 + P26 -> P24 + P27 -> P26 + P28 -> P24 + P29 -> P24 + P30 -> P29 + P31 -> P24 + P32 -> P24 + P33 -> P31 + P34 -> P24 + P35 -> P34 + P36 -> P34 + P37 -> P24 + P38 -> P37 + P39 -> P24 + P40 -> P39 + P41 -> P24 + P42 -> P41 + P43 -> P42 + P44 -> P42 + P45 -> P42 + P46 -> P42 + P47 -> P42 + P48 -> P42 + P49 -> P42 + P50 -> P42 + P51 -> P42 + P52 -> P42 + P53 -> P42 + P54 -> P42 + P55 -> P41 + P56 -> P24 + P57 -> P24 + P58 -> P57 + P59 -> P56 + P60 -> P24 + P61 -> P60 + P62 -> P24 + P63 -> P62 + P64 -> P24 + P65 -> P64 + P66 -> P64 + P67 -> P24 + P68 -> P67 + P69 -> P24 + P70 -> P69 + P71 -> P70 + P72 -> P24 + P73 -> P72 + P74 -> P73 + P75 -> P73 + P76 -> P73 + P77 -> P73 + P78 -> P72 + P79 -> P72 + P80 -> P72 + P81 -> P72 + P82 -> P24 + P83 -> P82 + P84 -> P83 + P85 -> P24 + P86 -> P85 + P87 -> P24 + P88 -> P87 + P89 -> P87 + P90 -> P24 + P91 -> P24 + P92 -> P91 + P93 -> P24 + P94 -> P93 + P95 -> P94 + P96 -> P94 + P97 -> P94 + P98 -> P94 + P99 -> P94 + P100 -> P94 + P101 -> P94 + P102 -> P94 + P103 -> P94 + P104 -> P94 + P105 -> P94 + P106 -> P94 + P107 -> P94 + P108 -> P94 + P109 -> P94 + P110 -> P24 + P111 -> P110 + P112 -> P24 + P115 -> P114 + P116 -> P115 + P117 -> P115 + P118 -> P115 + P119 -> P115 + P120 -> P115 + P121 -> P115 + P122 -> P115 + P123 -> P115 + P124 -> P115 + P125 -> P115 + P126 -> P115 + P127 -> P115 + P128 -> P115 + P129 -> P115 + P130 -> P115 + P131 -> P115 + P132 -> P115 + P133 -> P115 + P134 -> P115 + P135 -> P115 + P136 -> P114 + P137 -> P136 + P138 -> P137 + P139 -> P137 + P144 -> P143 + P145 -> P144 + P146 -> P145 + P147 -> P144 + P148 -> P147 + P150 -> P149 + P151 -> P150 + P152 -> P150 + P153 -> P152 + P154 -> P152 + P155 -> P152 + P156 -> P152 + P157 -> P152 + P159 -> P158 + P160 -> P159 + P161 -> P159 + P162 -> P158 + P163 -> P162 + P164 -> P162 + P165 -> P162 + P166 -> P162 + P167 -> P162 + P168 -> P162 + P169 -> P162 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P17 + T14 -> P19 + T15 -> P20 + T16 -> P21 + T17 -> P23 + T18 -> P26 + T19 -> P27 + T20 -> P29 + T21 -> P30 + T22 -> P32 + T23 -> P33 + T24 -> P34 + T25 -> P35 + T26 -> P36 + T27 -> P37 + T28 -> P38 + T29 -> P39 + T30 -> P40 + T31 -> P56 + T32 -> P57 + T33 -> P58 + T34 -> P59 + T35 -> P60 + T36 -> P61 + T37 -> P62 + T38 -> P63 + T39 -> P64 + T40 -> P65 + T41 -> P66 + T42 -> P67 + T43 -> P68 + T44 -> P70 + T45 -> P71 + T46 -> P72 + T47 -> P78 + T48 -> P79 + T49 -> P80 + T50 -> P85 + T51 -> P86 + T52 -> P87 + T53 -> P89 + T54 -> P91 + T55 -> P92 + T56 -> P93 + T57 -> P95 + T58 -> P96 + T59 -> P97 + T60 -> P98 + T61 -> P99 + T62 -> P100 + T63 -> P101 + T64 -> P102 + T65 -> P103 + T66 -> P104 + T67 -> P105 + T68 -> P106 + T69 -> P107 + T70 -> P108 + T71 -> P109 + T72 -> P110 + T73 -> P111 + T74 -> P112 + T75 -> P116 + T76 -> P118 + T77 -> P119 + T78 -> P120 + T79 -> P121 + T80 -> P122 + T81 -> P123 + T82 -> P124 + T83 -> P126 + T84 -> P129 + T85 -> P130 + T86 -> P131 + T87 -> P132 + T88 -> P133 + T89 -> P134 + T90 -> P135 + T91 -> P136 + T92 -> P138 + T93 -> P139 + T94 -> P140 + T95 -> P141 + T96 -> P142 + T97 -> P143 + T98 -> P148 + T99 -> P149 + T100 -> P151 + T101 -> P158 + T102 -> P160 + T103 -> P161 + T104 -> P163 + T105 -> P164 + T106 -> P165 + T107 -> P167 + T108 -> P168 + T109 -> P169 + T110 -> P43 + T111 -> P46 + T112 -> P47 + T113 -> P48 + T114 -> P49 + T115 -> P50 + T116 -> P51 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux) + T21 -> (mac + linux) + T22 -> (linux) + T23 -> (mac + linux + windows) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac) + T33 -> (mac) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac) + T43 -> (mac) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux) + T58 -> (linux) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (wsl) + T66 -> (wsl) + T67 -> (linux) + T68 -> (linux) + T69 -> (linux) + T70 -> (linux) + T71 -> (linux) + T72 -> (windows) + T73 -> (windows) + T74 -> (linux) + T75 -> (linux) + T76 -> (mac + linux + wsl) + T77 -> (windows) + T78 -> (mac + linux) + T79 -> (mac + linux) + T80 -> (windows) + T81 -> (linux) + T82 -> (mac) + T83 -> (wsl) + T84 -> (mac) + T85 -> (mac) + T86 -> (mac + linux + wsl) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (mac) + T96 -> (mac) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T110 + H1 -> T111 + H2 -> T112 + H3 -> T113 + H4 -> T114 + H5 -> T115 + H6 -> T116 }
fun realAt: Retired -> Path { R0 -> P25 + R1 -> P28 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P103 + R19 -> P104 + R20 -> P113 + R21 -> P117 + R22 -> P125 + R23 -> P127 + R24 -> P128 + R25 -> P134 + R26 -> P146 + R27 -> P153 + R28 -> P154 + R29 -> P155 + R30 -> P156 + R31 -> P157 + R32 -> P166 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux) + T21 -> (mac + linux) + T22 -> (linux) + T23 -> (mac + linux + windows) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac) + T33 -> (mac) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac) + T43 -> (mac) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux) + T58 -> (linux) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (wsl) + T66 -> (wsl) + T67 -> (linux) + T68 -> (linux) + T69 -> (linux) + T70 -> (linux) + T71 -> (linux) + T72 -> (windows) + T73 -> (windows) + T74 -> (linux) + T75 -> (linux) + T76 -> (mac + linux + wsl) + T77 -> (windows) + T78 -> (mac + linux) + T79 -> (mac + linux) + T80 -> (windows) + T81 -> (linux) + T82 -> (mac) + T83 -> (wsl) + T84 -> (mac) + T85 -> (mac) + T86 -> (mac + linux + wsl) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (mac) + T96 -> (mac) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T110 + H1 -> T111 + H2 -> T112 + H3 -> T113 + H4 -> T114 + H5 -> T115 + H6 -> T116 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P28 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P103 + R19 -> P104 + R20 -> P113 + R21 -> P117 + R22 -> P125 + R23 -> P127 + R24 -> P128 + R25 -> P134 + R26 -> P146 + R27 -> P153 + R28 -> P154 + R29 -> P155 + R30 -> P156 + R31 -> P157 + R32 -> P166 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux) + T21 -> (mac + linux) + T22 -> (linux) + T23 -> (mac + linux + windows) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac) + T33 -> (mac) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac) + T43 -> (mac) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux) + T58 -> (linux) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (wsl) + T66 -> (wsl) + T67 -> (linux) + T68 -> (linux) + T69 -> (linux) + T70 -> (linux) + T71 -> (linux) + T72 -> (windows) + T73 -> (windows) + T74 -> (linux) + T75 -> (linux) + T76 -> (mac + linux + wsl) + T77 -> (windows) + T78 -> (mac + linux) + T79 -> (mac + linux) + T80 -> (windows) + T81 -> (linux) + T82 -> (mac) + T83 -> (wsl) + T84 -> (mac) + T85 -> (mac) + T86 -> (mac + linux + wsl) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (mac) + T96 -> (mac) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T110 + H1 -> T110 + H2 -> T112 + H3 -> T113 + H4 -> T114 + H5 -> T115 + H6 -> T116 }
fun plantedHookAt: Retired -> Path { R0 -> P25 + R1 -> P28 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P103 + R19 -> P104 + R20 -> P113 + R21 -> P117 + R22 -> P125 + R23 -> P127 + R24 -> P128 + R25 -> P134 + R26 -> P146 + R27 -> P153 + R28 -> P154 + R29 -> P155 + R30 -> P156 + R31 -> P157 + R32 -> P166 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux) + T21 -> (mac + linux) + T22 -> (linux) + T23 -> (mac + linux + windows) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac) + T33 -> (mac) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac) + T43 -> (mac) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux) + T58 -> (linux) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (wsl) + T66 -> (wsl) + T67 -> (linux) + T68 -> (linux) + T69 -> (linux) + T70 -> (linux) + T71 -> (linux) + T72 -> (windows) + T73 -> (windows) + T74 -> (linux) + T75 -> (linux) + T76 -> (mac + linux + wsl) + T77 -> (windows) + T78 -> (mac + linux) + T79 -> (mac + linux) + T80 -> (windows) + T81 -> (linux) + T82 -> (mac) + T83 -> (wsl) + T84 -> (mac) + T85 -> (mac) + T86 -> (mac + linux + wsl) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (mac) + T96 -> (mac) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T110 + H1 -> T111 + H2 -> T112 + H3 -> T113 + H4 -> T114 + H5 -> T115 + H6 -> T116 }
fun plantedMaskAt: Retired -> Path { R0 -> P25 + R1 -> P28 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P103 + R19 -> P104 + R20 -> P113 + R21 -> P117 + R22 -> P125 + R23 -> P127 + R24 -> P128 + R25 -> P134 + R26 -> P146 + R27 -> P153 + R28 -> P154 + R29 -> P155 + R30 -> P156 + R31 -> P157 + R32 -> P166 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (Profile) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux) + T21 -> (mac + linux) + T22 -> (linux) + T23 -> (mac + linux + windows) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac) + T33 -> (mac) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac) + T43 -> (mac) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux) + T58 -> (linux) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (wsl) + T66 -> (wsl) + T67 -> (linux) + T68 -> (linux) + T69 -> (linux) + T70 -> (linux) + T71 -> (linux) + T72 -> (windows) + T73 -> (windows) + T74 -> (linux) + T75 -> (linux) + T76 -> (mac + linux + wsl) + T77 -> (windows) + T78 -> (mac + linux) + T79 -> (mac + linux) + T80 -> (windows) + T81 -> (linux) + T82 -> (mac) + T83 -> (wsl) + T84 -> (mac) + T85 -> (mac) + T86 -> (mac + linux + wsl) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (mac) + T96 -> (mac) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T110 + H1 -> T111 + H2 -> T112 + H3 -> T113 + H4 -> T114 + H5 -> T115 + H6 -> T116 }
fun plantedEmptyAt: Retired -> Path { R0 -> P25 + R1 -> P28 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P103 + R19 -> P104 + R20 -> P113 + R21 -> P117 + R22 -> P125 + R23 -> P127 + R24 -> P128 + R25 -> P134 + R26 -> P146 + R27 -> P153 + R28 -> P154 + R29 -> P155 + R30 -> P156 + R31 -> P157 + R32 -> P166 }
fun plantedEmptyRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
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
