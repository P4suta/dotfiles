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
// P11 is .chezmoiscripts/unix/11-ocaml.sh
// P12 is .chezmoiscripts/unix/12-haskell.sh
// P13 is .chezmoiscripts/unix/13-racket.sh
// P14 is .chezmoiscripts/unix/14-guard.sh
// P15 is .chezmoiscripts/unix/17-fleet.sh
// P16 is .chezmoiscripts/unix/18-retire-tmux.sh
// P17 is .claude
// P18 is .claude/bashenv.sh
// P19 is .claude/shims
// P20 is .claude/shims/.keep
// P21 is .claude/skills
// P22 is .claude/skills/orbstack
// P23 is .config
// P24 is .config/aerospace
// P25 is .config/atuin
// P26 is .config/atuin/config.toml
// P27 is .config/borders
// P28 is .config/dev-reaper
// P29 is .config/dev-reaper/config
// P30 is .config/dotfiles-nix
// P31 is .config/fcitx5
// P32 is .config/fcitx5/config
// P33 is .config/fcitx5/profile
// P34 is .config/gh
// P35 is .config/gh/config.yml
// P36 is .config/ghostty
// P37 is .config/ghostty/config
// P38 is .config/git
// P39 is .config/git/hooks
// P40 is .config/git/hooks/commit-msg
// P41 is .config/git/hooks/dotguard-delegate.sh
// P42 is .config/git/hooks/lefthook-delegate.sh
// P43 is .config/git/hooks/post-checkout
// P44 is .config/git/hooks/post-commit
// P45 is .config/git/hooks/post-merge
// P46 is .config/git/hooks/pre-commit
// P47 is .config/git/hooks/pre-push
// P48 is .config/git/hooks/reference-transaction
// P49 is .config/git/hooks/renovate-freshness.sh
// P50 is .config/git/hooks/storage-scout-delegate.sh
// P51 is .config/git/hooks/strip-agent-trailers.sh
// P52 is .config/git/template
// P53 is .config/herdr
// P54 is .config/herdr-agent
// P55 is .config/herdr-agent/keys
// P56 is .config/herdr/config.toml
// P57 is .config/homebrew
// P58 is .config/homebrew/Brewfile
// P59 is .config/jj
// P60 is .config/jj/config.toml
// P61 is .config/kitty
// P62 is .config/kitty/kitty.conf
// P63 is .config/kitty/tokyo-night.conf
// P64 is .config/lazygit
// P65 is .config/lazygit/config.yml
// P66 is .config/mise
// P67 is .config/mise/tasks
// P68 is .config/mise/tasks/git-audit
// P69 is .config/nushell
// P70 is .config/nushell/autoload
// P71 is .config/nushell/autoload/atuin.nu
// P72 is .config/nushell/autoload/carapace.nu
// P73 is .config/nushell/autoload/starship.nu
// P74 is .config/nushell/autoload/zoxide.nu
// P75 is .config/nushell/config.nu
// P76 is .config/nushell/env.nu
// P77 is .config/nushell/login.nu
// P78 is .config/nushell/mise.nu
// P79 is .config/opencode
// P80 is .config/opencode/plugins
// P81 is .config/opencode/plugins/prose.ts
// P82 is .config/session-reaper
// P83 is .config/session-reaper/config
// P84 is .config/shell
// P85 is .config/shell/github-token.sh
// P86 is .config/shell/ssh-agent.sh
// P87 is .config/sketchybar
// P88 is .config/storage-scout
// P89 is .config/storage-scout/auto.toml
// P90 is .config/systemd
// P91 is .config/systemd/user
// P92 is .config/systemd/user/dev-cleanup.service
// P93 is .config/systemd/user/dev-cleanup.timer
// P94 is .config/systemd/user/dev-reaper.service
// P95 is .config/systemd/user/dev-reaper.timer
// P96 is .config/systemd/user/docker-prune.service
// P97 is .config/systemd/user/docker-prune.timer
// P98 is .config/systemd/user/dotfiles-desktop-refresh.service
// P99 is .config/systemd/user/dotfiles-desktop-refresh.timer
// P100 is .config/systemd/user/mise-upgrade.service
// P101 is .config/systemd/user/mise-upgrade.timer
// P102 is .config/systemd/user/session-reaper.service
// P103 is .config/systemd/user/session-reaper.timer
// P104 is .config/systemd/user/storage-scout.service
// P105 is .config/systemd/user/tools-upgrade.service
// P106 is .config/systemd/user/tools-upgrade.timer
// P107 is .config/wezterm
// P108 is .config/wezterm/wezterm.lua
// P109 is .config/xdg-terminals.list
// P110 is .gitconfig
// P111 is .local
// P112 is .local/bin
// P113 is .local/bin/dev-reaper
// P114 is .local/bin/dotctl.exe
// P115 is .local/bin/dotfiles-doctor
// P116 is .local/bin/dotfiles-doctor.ps1
// P117 is .local/bin/dotfiles-refresh-desktop
// P118 is .local/bin/dotfiles-refresh-integrations
// P119 is .local/bin/dotfiles-refresh-integrations.ps1
// P120 is .local/bin/dotfiles-retire-tmux
// P121 is .local/bin/dotfiles-touchid
// P122 is .local/bin/dotfiles-wm-defaults
// P123 is .local/bin/dotfiles-wsl-ssh-keygen
// P124 is .local/bin/dotfiles-xtask
// P125 is .local/bin/dotfiles-xtask.exe
// P126 is .local/bin/ghostty-herdr
// P127 is .local/bin/ghostty-nu
// P128 is .local/bin/git
// P129 is .local/bin/kitty-herdr
// P130 is .local/bin/kitty-nu
// P131 is .local/bin/renovate-local
// P132 is .local/bin/session-reaper
// P133 is .local/share
// P134 is .local/share/applications
// P135 is .local/share/applications/kitty-herdr.desktop
// P136 is .local/share/applications/kitty-nu.desktop
// P137 is .profile
// P138 is .zprofile
// P139 is .zshrc
// P140 is AppData
// P141 is AppData/Roaming
// P142 is AppData/Roaming/alacritty
// P143 is AppData/Roaming/alacritty/alacritty.toml
// P144 is AppData/Roaming/herdr
// P145 is AppData/Roaming/herdr/config.toml
// P146 is Documents
// P147 is Documents/PowerShell
// P148 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P149 is Documents/PowerShell/modules
// P150 is Documents/PowerShell/modules/00-readline.ps1
// P151 is Documents/PowerShell/modules/05-mise.ps1
// P152 is Documents/PowerShell/modules/10-tools.ps1
// P153 is Documents/PowerShell/modules/20-herdr.ps1
// P154 is Documents/PowerShell/modules/99-prompt.ps1
// P155 is Library
// P156 is Library/Application Support
// P157 is Library/Application Support/lazygit
// P158 is Library/Application Support/nushell
// P159 is Library/LaunchAgents
// P160 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P161 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P162 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P163 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P164 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P165 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P166 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32, R33, R34 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P18 -> P17 + P19 -> P17 + P20 -> P19 + P21 -> P17 + P22 -> P21 + P24 -> P23 + P25 -> P23 + P26 -> P25 + P27 -> P23 + P28 -> P23 + P29 -> P28 + P30 -> P23 + P31 -> P23 + P32 -> P31 + P33 -> P31 + P34 -> P23 + P35 -> P34 + P36 -> P23 + P37 -> P36 + P38 -> P23 + P39 -> P38 + P40 -> P39 + P41 -> P39 + P42 -> P39 + P43 -> P39 + P44 -> P39 + P45 -> P39 + P46 -> P39 + P47 -> P39 + P48 -> P39 + P49 -> P39 + P50 -> P39 + P51 -> P39 + P52 -> P38 + P53 -> P23 + P54 -> P23 + P55 -> P54 + P56 -> P53 + P57 -> P23 + P58 -> P57 + P59 -> P23 + P60 -> P59 + P61 -> P23 + P62 -> P61 + P63 -> P61 + P64 -> P23 + P65 -> P64 + P66 -> P23 + P67 -> P66 + P68 -> P67 + P69 -> P23 + P70 -> P69 + P71 -> P70 + P72 -> P70 + P73 -> P70 + P74 -> P70 + P75 -> P69 + P76 -> P69 + P77 -> P69 + P78 -> P69 + P79 -> P23 + P80 -> P79 + P81 -> P80 + P82 -> P23 + P83 -> P82 + P84 -> P23 + P85 -> P84 + P86 -> P84 + P87 -> P23 + P88 -> P23 + P89 -> P88 + P90 -> P23 + P91 -> P90 + P92 -> P91 + P93 -> P91 + P94 -> P91 + P95 -> P91 + P96 -> P91 + P97 -> P91 + P98 -> P91 + P99 -> P91 + P100 -> P91 + P101 -> P91 + P102 -> P91 + P103 -> P91 + P104 -> P91 + P105 -> P91 + P106 -> P91 + P107 -> P23 + P108 -> P107 + P109 -> P23 + P112 -> P111 + P113 -> P112 + P114 -> P112 + P115 -> P112 + P116 -> P112 + P117 -> P112 + P118 -> P112 + P119 -> P112 + P120 -> P112 + P121 -> P112 + P122 -> P112 + P123 -> P112 + P124 -> P112 + P125 -> P112 + P126 -> P112 + P127 -> P112 + P128 -> P112 + P129 -> P112 + P130 -> P112 + P131 -> P112 + P132 -> P112 + P133 -> P111 + P134 -> P133 + P135 -> P134 + P136 -> P134 + P141 -> P140 + P142 -> P141 + P143 -> P142 + P144 -> P141 + P145 -> P144 + P147 -> P146 + P148 -> P147 + P149 -> P147 + P150 -> P149 + P151 -> P149 + P152 -> P149 + P153 -> P149 + P154 -> P149 + P156 -> P155 + P157 -> P156 + P158 -> P156 + P159 -> P155 + P160 -> P159 + P161 -> P159 + P162 -> P159 + P163 -> P159 + P164 -> P159 + P165 -> P159 + P166 -> P159 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P18 + T14 -> P19 + T15 -> P20 + T16 -> P22 + T17 -> P25 + T18 -> P26 + T19 -> P28 + T20 -> P29 + T21 -> P30 + T22 -> P31 + T23 -> P32 + T24 -> P33 + T25 -> P34 + T26 -> P35 + T27 -> P36 + T28 -> P37 + T29 -> P53 + T30 -> P54 + T31 -> P55 + T32 -> P56 + T33 -> P57 + T34 -> P58 + T35 -> P59 + T36 -> P60 + T37 -> P61 + T38 -> P62 + T39 -> P63 + T40 -> P64 + T41 -> P65 + T42 -> P67 + T43 -> P68 + T44 -> P69 + T45 -> P75 + T46 -> P76 + T47 -> P77 + T48 -> P82 + T49 -> P83 + T50 -> P84 + T51 -> P86 + T52 -> P88 + T53 -> P89 + T54 -> P90 + T55 -> P92 + T56 -> P93 + T57 -> P94 + T58 -> P95 + T59 -> P98 + T60 -> P99 + T61 -> P102 + T62 -> P103 + T63 -> P104 + T64 -> P105 + T65 -> P106 + T66 -> P107 + T67 -> P108 + T68 -> P109 + T69 -> P113 + T70 -> P115 + T71 -> P116 + T72 -> P117 + T73 -> P118 + T74 -> P119 + T75 -> P120 + T76 -> P121 + T77 -> P123 + T78 -> P126 + T79 -> P127 + T80 -> P128 + T81 -> P129 + T82 -> P130 + T83 -> P131 + T84 -> P132 + T85 -> P133 + T86 -> P135 + T87 -> P136 + T88 -> P137 + T89 -> P138 + T90 -> P139 + T91 -> P140 + T92 -> P145 + T93 -> P146 + T94 -> P148 + T95 -> P155 + T96 -> P157 + T97 -> P158 + T98 -> P160 + T99 -> P161 + T100 -> P162 + T101 -> P164 + T102 -> P165 + T103 -> P166 + T104 -> P40 + T105 -> P43 + T106 -> P44 + T107 -> P45 + T108 -> P46 + T109 -> P47 + T110 -> P48 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (linux) + T23 -> (linux) + T24 -> (linux) + T25 -> (mac + linux + wsl) + T26 -> (mac + linux + wsl) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux + wsl) + T36 -> (mac + linux + wsl) + T37 -> (linux) + T38 -> (linux) + T39 -> (linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux) + T45 -> (mac + linux) + T46 -> (mac + linux) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux + windows) + T53 -> (mac + linux + windows) + T54 -> (linux + wsl) + T55 -> (linux + wsl) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (windows) + T67 -> (windows) + T68 -> (linux) + T69 -> (linux + wsl) + T70 -> (mac + linux + wsl) + T71 -> (windows) + T72 -> (mac + linux) + T73 -> (mac + linux) + T74 -> (windows) + T75 -> (linux) + T76 -> (mac) + T77 -> (wsl) + T78 -> (mac) + T79 -> (mac) + T80 -> (mac + linux + wsl) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (mac) + T90 -> (mac) + T91 -> (windows) + T92 -> (windows) + T93 -> (windows) + T94 -> (windows) + T95 -> (mac) + T96 -> (mac) + T97 -> (mac) + T98 -> (mac) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + windows + wsl) + T105 -> (mac + linux + windows + wsl) + T106 -> (mac + linux + windows + wsl) + T107 -> (mac + linux + windows + wsl) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T104 + H1 -> T105 + H2 -> T106 + H3 -> T107 + H4 -> T108 + H5 -> T109 + H6 -> T110 }
fun realAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P41 + R3 -> P42 + R4 -> P49 + R5 -> P50 + R6 -> P51 + R7 -> P52 + R8 -> P70 + R9 -> P71 + R10 -> P72 + R11 -> P73 + R12 -> P74 + R13 -> P77 + R14 -> P78 + R15 -> P81 + R16 -> P85 + R17 -> P87 + R18 -> P96 + R19 -> P97 + R20 -> P100 + R21 -> P101 + R22 -> P110 + R23 -> P114 + R24 -> P122 + R25 -> P124 + R26 -> P125 + R27 -> P131 + R28 -> P143 + R29 -> P150 + R30 -> P151 + R31 -> P152 + R32 -> P153 + R33 -> P154 + R34 -> P163 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (linux) + T23 -> (linux) + T24 -> (linux) + T25 -> (mac + linux + wsl) + T26 -> (mac + linux + wsl) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux + wsl) + T36 -> (mac + linux + wsl) + T37 -> (linux) + T38 -> (linux) + T39 -> (linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux) + T45 -> (mac + linux) + T46 -> (mac + linux) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux + windows) + T53 -> (mac + linux + windows) + T54 -> (linux + wsl) + T55 -> (linux + wsl) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (windows) + T67 -> (windows) + T68 -> (linux) + T69 -> (linux + wsl) + T70 -> (mac + linux + wsl) + T71 -> (windows) + T72 -> (mac + linux) + T73 -> (mac + linux) + T74 -> (windows) + T75 -> (linux) + T76 -> (mac) + T77 -> (wsl) + T78 -> (mac) + T79 -> (mac) + T80 -> (mac + linux + wsl) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (mac) + T90 -> (mac) + T91 -> (windows) + T92 -> (windows) + T93 -> (windows) + T94 -> (windows) + T95 -> (mac) + T96 -> (mac) + T97 -> (mac) + T98 -> (mac) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + windows + wsl) + T105 -> (mac + linux + windows + wsl) + T106 -> (mac + linux + windows + wsl) + T107 -> (mac + linux + windows + wsl) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T104 + H1 -> T105 + H2 -> T106 + H3 -> T107 + H4 -> T108 + H5 -> T109 + H6 -> T110 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P27 + R2 -> P41 + R3 -> P42 + R4 -> P49 + R5 -> P50 + R6 -> P51 + R7 -> P52 + R8 -> P70 + R9 -> P71 + R10 -> P72 + R11 -> P73 + R12 -> P74 + R13 -> P77 + R14 -> P78 + R15 -> P81 + R16 -> P85 + R17 -> P87 + R18 -> P96 + R19 -> P97 + R20 -> P100 + R21 -> P101 + R22 -> P110 + R23 -> P114 + R24 -> P122 + R25 -> P124 + R26 -> P125 + R27 -> P131 + R28 -> P143 + R29 -> P150 + R30 -> P151 + R31 -> P152 + R32 -> P153 + R33 -> P154 + R34 -> P163 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (linux) + T23 -> (linux) + T24 -> (linux) + T25 -> (mac + linux + wsl) + T26 -> (mac + linux + wsl) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux + wsl) + T36 -> (mac + linux + wsl) + T37 -> (linux) + T38 -> (linux) + T39 -> (linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux) + T45 -> (mac + linux) + T46 -> (mac + linux) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux + windows) + T53 -> (mac + linux + windows) + T54 -> (linux + wsl) + T55 -> (linux + wsl) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (windows) + T67 -> (windows) + T68 -> (linux) + T69 -> (linux + wsl) + T70 -> (mac + linux + wsl) + T71 -> (windows) + T72 -> (mac + linux) + T73 -> (mac + linux) + T74 -> (windows) + T75 -> (linux) + T76 -> (mac) + T77 -> (wsl) + T78 -> (mac) + T79 -> (mac) + T80 -> (mac + linux + wsl) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (mac) + T90 -> (mac) + T91 -> (windows) + T92 -> (windows) + T93 -> (windows) + T94 -> (windows) + T95 -> (mac) + T96 -> (mac) + T97 -> (mac) + T98 -> (mac) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + windows + wsl) + T105 -> (mac + linux + windows + wsl) + T106 -> (mac + linux + windows + wsl) + T107 -> (mac + linux + windows + wsl) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T104 + H1 -> T104 + H2 -> T106 + H3 -> T107 + H4 -> T108 + H5 -> T109 + H6 -> T110 }
fun plantedHookAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P41 + R3 -> P42 + R4 -> P49 + R5 -> P50 + R6 -> P51 + R7 -> P52 + R8 -> P70 + R9 -> P71 + R10 -> P72 + R11 -> P73 + R12 -> P74 + R13 -> P77 + R14 -> P78 + R15 -> P81 + R16 -> P85 + R17 -> P87 + R18 -> P96 + R19 -> P97 + R20 -> P100 + R21 -> P101 + R22 -> P110 + R23 -> P114 + R24 -> P122 + R25 -> P124 + R26 -> P125 + R27 -> P131 + R28 -> P143 + R29 -> P150 + R30 -> P151 + R31 -> P152 + R32 -> P153 + R33 -> P154 + R34 -> P163 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (linux) + T23 -> (linux) + T24 -> (linux) + T25 -> (mac + linux + wsl) + T26 -> (mac + linux + wsl) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux + wsl) + T36 -> (mac + linux + wsl) + T37 -> (linux) + T38 -> (linux) + T39 -> (linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux) + T45 -> (mac + linux) + T46 -> (mac + linux) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux + windows) + T53 -> (mac + linux + windows) + T54 -> (linux + wsl) + T55 -> (linux + wsl) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (windows) + T67 -> (windows) + T68 -> (linux) + T69 -> (linux + wsl) + T70 -> (mac + linux + wsl) + T71 -> (windows) + T72 -> (mac + linux) + T73 -> (mac + linux) + T74 -> (windows) + T75 -> (linux) + T76 -> (mac) + T77 -> (wsl) + T78 -> (mac) + T79 -> (mac) + T80 -> (mac + linux + wsl) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (mac) + T90 -> (mac) + T91 -> (windows) + T92 -> (windows) + T93 -> (windows) + T94 -> (windows) + T95 -> (mac) + T96 -> (mac) + T97 -> (mac) + T98 -> (mac) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac + linux + windows + wsl) + T106 -> (mac + linux + windows + wsl) + T107 -> (mac + linux + windows + wsl) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T104 + H1 -> T105 + H2 -> T106 + H3 -> T107 + H4 -> T108 + H5 -> T109 + H6 -> T110 }
fun plantedMaskAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P41 + R3 -> P42 + R4 -> P49 + R5 -> P50 + R6 -> P51 + R7 -> P52 + R8 -> P70 + R9 -> P71 + R10 -> P72 + R11 -> P73 + R12 -> P74 + R13 -> P77 + R14 -> P78 + R15 -> P81 + R16 -> P85 + R17 -> P87 + R18 -> P96 + R19 -> P97 + R20 -> P100 + R21 -> P101 + R22 -> P110 + R23 -> P114 + R24 -> P122 + R25 -> P124 + R26 -> P125 + R27 -> P131 + R28 -> P143 + R29 -> P150 + R30 -> P151 + R31 -> P152 + R32 -> P153 + R33 -> P154 + R34 -> P163 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (Profile) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (linux) + T23 -> (linux) + T24 -> (linux) + T25 -> (mac + linux + wsl) + T26 -> (mac + linux + wsl) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux) + T30 -> (mac) + T31 -> (mac) + T32 -> (mac + linux) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux + wsl) + T36 -> (mac + linux + wsl) + T37 -> (linux) + T38 -> (linux) + T39 -> (linux) + T40 -> (mac) + T41 -> (mac) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux) + T45 -> (mac + linux) + T46 -> (mac + linux) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux + windows) + T53 -> (mac + linux + windows) + T54 -> (linux + wsl) + T55 -> (linux + wsl) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux) + T60 -> (linux) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (windows) + T67 -> (windows) + T68 -> (linux) + T69 -> (linux + wsl) + T70 -> (mac + linux + wsl) + T71 -> (windows) + T72 -> (mac + linux) + T73 -> (mac + linux) + T74 -> (windows) + T75 -> (linux) + T76 -> (mac) + T77 -> (wsl) + T78 -> (mac) + T79 -> (mac) + T80 -> (mac + linux + wsl) + T81 -> (linux) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (mac) + T90 -> (mac) + T91 -> (windows) + T92 -> (windows) + T93 -> (windows) + T94 -> (windows) + T95 -> (mac) + T96 -> (mac) + T97 -> (mac) + T98 -> (mac) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac + linux + windows + wsl) + T105 -> (mac + linux + windows + wsl) + T106 -> (mac + linux + windows + wsl) + T107 -> (mac + linux + windows + wsl) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T104 + H1 -> T105 + H2 -> T106 + H3 -> T107 + H4 -> T108 + H5 -> T109 + H6 -> T110 }
fun plantedEmptyAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P41 + R3 -> P42 + R4 -> P49 + R5 -> P50 + R6 -> P51 + R7 -> P52 + R8 -> P70 + R9 -> P71 + R10 -> P72 + R11 -> P73 + R12 -> P74 + R13 -> P77 + R14 -> P78 + R15 -> P81 + R16 -> P85 + R17 -> P87 + R18 -> P96 + R19 -> P97 + R20 -> P100 + R21 -> P101 + R22 -> P110 + R23 -> P114 + R24 -> P122 + R25 -> P124 + R26 -> P125 + R27 -> P131 + R28 -> P143 + R29 -> P150 + R30 -> P151 + R31 -> P152 + R32 -> P153 + R33 -> P154 + R34 -> P163 }
fun plantedEmptyRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
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
