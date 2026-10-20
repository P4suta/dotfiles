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
// P27 is .config/bat
// P28 is .config/bat/config
// P29 is .config/borders
// P30 is .config/dev-reaper
// P31 is .config/dev-reaper/config
// P32 is .config/dotfiles-nix
// P33 is .config/fcitx5
// P34 is .config/fcitx5/config
// P35 is .config/fcitx5/profile
// P36 is .config/fzf
// P37 is .config/fzf/fzfrc
// P38 is .config/gh
// P39 is .config/gh/config.yml
// P40 is .config/ghostty
// P41 is .config/ghostty/config
// P42 is .config/git
// P43 is .config/git/hooks
// P44 is .config/git/hooks/commit-msg
// P45 is .config/git/hooks/dotguard-delegate.sh
// P46 is .config/git/hooks/lefthook-delegate.sh
// P47 is .config/git/hooks/post-checkout
// P48 is .config/git/hooks/post-commit
// P49 is .config/git/hooks/post-merge
// P50 is .config/git/hooks/pre-commit
// P51 is .config/git/hooks/pre-push
// P52 is .config/git/hooks/reference-transaction
// P53 is .config/git/hooks/renovate-freshness.sh
// P54 is .config/git/hooks/storage-scout-delegate.sh
// P55 is .config/git/hooks/strip-agent-trailers.sh
// P56 is .config/git/template
// P57 is .config/herdr
// P58 is .config/herdr-agent
// P59 is .config/herdr-agent/keys
// P60 is .config/herdr/config.toml
// P61 is .config/homebrew
// P62 is .config/homebrew/Brewfile
// P63 is .config/jj
// P64 is .config/jj/config.toml
// P65 is .config/kitty
// P66 is .config/kitty/kitty.conf
// P67 is .config/kitty/tokyo-night.conf
// P68 is .config/lazygit
// P69 is .config/lazygit/config.yml
// P70 is .config/mise
// P71 is .config/mise/tasks
// P72 is .config/mise/tasks/git-audit
// P73 is .config/nushell
// P74 is .config/nushell/autoload
// P75 is .config/nushell/autoload/atuin.nu
// P76 is .config/nushell/autoload/carapace.nu
// P77 is .config/nushell/autoload/starship.nu
// P78 is .config/nushell/autoload/zoxide.nu
// P79 is .config/nushell/config.nu
// P80 is .config/nushell/env.nu
// P81 is .config/nushell/login.nu
// P82 is .config/nushell/mise.nu
// P83 is .config/opencode
// P84 is .config/opencode/plugins
// P85 is .config/opencode/plugins/prose.ts
// P86 is .config/session-reaper
// P87 is .config/session-reaper/config
// P88 is .config/shell
// P89 is .config/shell/github-token.sh
// P90 is .config/shell/ssh-agent.sh
// P91 is .config/sketchybar
// P92 is .config/storage-scout
// P93 is .config/storage-scout/auto.toml
// P94 is .config/systemd
// P95 is .config/systemd/user
// P96 is .config/systemd/user/dev-cleanup.service
// P97 is .config/systemd/user/dev-cleanup.timer
// P98 is .config/systemd/user/dev-reaper.service
// P99 is .config/systemd/user/dev-reaper.timer
// P100 is .config/systemd/user/docker-prune.service
// P101 is .config/systemd/user/docker-prune.timer
// P102 is .config/systemd/user/dotfiles-desktop-refresh.service
// P103 is .config/systemd/user/dotfiles-desktop-refresh.timer
// P104 is .config/systemd/user/mise-upgrade.service
// P105 is .config/systemd/user/mise-upgrade.timer
// P106 is .config/systemd/user/session-reaper.service
// P107 is .config/systemd/user/session-reaper.timer
// P108 is .config/systemd/user/storage-scout.service
// P109 is .config/systemd/user/tools-upgrade.service
// P110 is .config/systemd/user/tools-upgrade.timer
// P111 is .config/wezterm
// P112 is .config/wezterm/wezterm.lua
// P113 is .config/xdg-terminals.list
// P114 is .gitconfig
// P115 is .local
// P116 is .local/bin
// P117 is .local/bin/dev-reaper
// P118 is .local/bin/dotctl.exe
// P119 is .local/bin/dotfiles-doctor
// P120 is .local/bin/dotfiles-doctor.ps1
// P121 is .local/bin/dotfiles-refresh-desktop
// P122 is .local/bin/dotfiles-refresh-integrations
// P123 is .local/bin/dotfiles-refresh-integrations.ps1
// P124 is .local/bin/dotfiles-retire-tmux
// P125 is .local/bin/dotfiles-touchid
// P126 is .local/bin/dotfiles-wm-defaults
// P127 is .local/bin/dotfiles-wsl-ssh-keygen
// P128 is .local/bin/dotfiles-xtask
// P129 is .local/bin/dotfiles-xtask.exe
// P130 is .local/bin/ghostty-herdr
// P131 is .local/bin/ghostty-nu
// P132 is .local/bin/git
// P133 is .local/bin/kitty-herdr
// P134 is .local/bin/kitty-nu
// P135 is .local/bin/renovate-local
// P136 is .local/bin/session-reaper
// P137 is .local/share
// P138 is .local/share/applications
// P139 is .local/share/applications/kitty-herdr.desktop
// P140 is .local/share/applications/kitty-nu.desktop
// P141 is .profile
// P142 is .zprofile
// P143 is .zshrc
// P144 is AppData
// P145 is AppData/Roaming
// P146 is AppData/Roaming/alacritty
// P147 is AppData/Roaming/alacritty/alacritty.toml
// P148 is AppData/Roaming/herdr
// P149 is AppData/Roaming/herdr/config.toml
// P150 is Documents
// P151 is Documents/PowerShell
// P152 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P153 is Documents/PowerShell/modules
// P154 is Documents/PowerShell/modules/00-readline.ps1
// P155 is Documents/PowerShell/modules/05-mise.ps1
// P156 is Documents/PowerShell/modules/10-tools.ps1
// P157 is Documents/PowerShell/modules/20-herdr.ps1
// P158 is Documents/PowerShell/modules/99-prompt.ps1
// P159 is Library
// P160 is Library/Application Support
// P161 is Library/Application Support/lazygit
// P162 is Library/Application Support/nushell
// P163 is Library/LaunchAgents
// P164 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P165 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P166 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P167 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P168 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P169 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P170 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166, P167, P168, P169, P170 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110, T111, T112, T113, T114 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32, R33, R34 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P18 -> P17 + P19 -> P17 + P20 -> P19 + P21 -> P17 + P22 -> P21 + P24 -> P23 + P25 -> P23 + P26 -> P25 + P27 -> P23 + P28 -> P27 + P29 -> P23 + P30 -> P23 + P31 -> P30 + P32 -> P23 + P33 -> P23 + P34 -> P33 + P35 -> P33 + P36 -> P23 + P37 -> P36 + P38 -> P23 + P39 -> P38 + P40 -> P23 + P41 -> P40 + P42 -> P23 + P43 -> P42 + P44 -> P43 + P45 -> P43 + P46 -> P43 + P47 -> P43 + P48 -> P43 + P49 -> P43 + P50 -> P43 + P51 -> P43 + P52 -> P43 + P53 -> P43 + P54 -> P43 + P55 -> P43 + P56 -> P42 + P57 -> P23 + P58 -> P23 + P59 -> P58 + P60 -> P57 + P61 -> P23 + P62 -> P61 + P63 -> P23 + P64 -> P63 + P65 -> P23 + P66 -> P65 + P67 -> P65 + P68 -> P23 + P69 -> P68 + P70 -> P23 + P71 -> P70 + P72 -> P71 + P73 -> P23 + P74 -> P73 + P75 -> P74 + P76 -> P74 + P77 -> P74 + P78 -> P74 + P79 -> P73 + P80 -> P73 + P81 -> P73 + P82 -> P73 + P83 -> P23 + P84 -> P83 + P85 -> P84 + P86 -> P23 + P87 -> P86 + P88 -> P23 + P89 -> P88 + P90 -> P88 + P91 -> P23 + P92 -> P23 + P93 -> P92 + P94 -> P23 + P95 -> P94 + P96 -> P95 + P97 -> P95 + P98 -> P95 + P99 -> P95 + P100 -> P95 + P101 -> P95 + P102 -> P95 + P103 -> P95 + P104 -> P95 + P105 -> P95 + P106 -> P95 + P107 -> P95 + P108 -> P95 + P109 -> P95 + P110 -> P95 + P111 -> P23 + P112 -> P111 + P113 -> P23 + P116 -> P115 + P117 -> P116 + P118 -> P116 + P119 -> P116 + P120 -> P116 + P121 -> P116 + P122 -> P116 + P123 -> P116 + P124 -> P116 + P125 -> P116 + P126 -> P116 + P127 -> P116 + P128 -> P116 + P129 -> P116 + P130 -> P116 + P131 -> P116 + P132 -> P116 + P133 -> P116 + P134 -> P116 + P135 -> P116 + P136 -> P116 + P137 -> P115 + P138 -> P137 + P139 -> P138 + P140 -> P138 + P145 -> P144 + P146 -> P145 + P147 -> P146 + P148 -> P145 + P149 -> P148 + P151 -> P150 + P152 -> P151 + P153 -> P151 + P154 -> P153 + P155 -> P153 + P156 -> P153 + P157 -> P153 + P158 -> P153 + P160 -> P159 + P161 -> P160 + P162 -> P160 + P163 -> P159 + P164 -> P163 + P165 -> P163 + P166 -> P163 + P167 -> P163 + P168 -> P163 + P169 -> P163 + P170 -> P163 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P18 + T14 -> P19 + T15 -> P20 + T16 -> P22 + T17 -> P25 + T18 -> P26 + T19 -> P27 + T20 -> P28 + T21 -> P30 + T22 -> P31 + T23 -> P32 + T24 -> P33 + T25 -> P34 + T26 -> P35 + T27 -> P36 + T28 -> P37 + T29 -> P38 + T30 -> P39 + T31 -> P40 + T32 -> P41 + T33 -> P57 + T34 -> P58 + T35 -> P59 + T36 -> P60 + T37 -> P61 + T38 -> P62 + T39 -> P63 + T40 -> P64 + T41 -> P65 + T42 -> P66 + T43 -> P67 + T44 -> P68 + T45 -> P69 + T46 -> P71 + T47 -> P72 + T48 -> P73 + T49 -> P79 + T50 -> P80 + T51 -> P81 + T52 -> P86 + T53 -> P87 + T54 -> P88 + T55 -> P90 + T56 -> P92 + T57 -> P93 + T58 -> P94 + T59 -> P96 + T60 -> P97 + T61 -> P98 + T62 -> P99 + T63 -> P102 + T64 -> P103 + T65 -> P106 + T66 -> P107 + T67 -> P108 + T68 -> P109 + T69 -> P110 + T70 -> P111 + T71 -> P112 + T72 -> P113 + T73 -> P117 + T74 -> P119 + T75 -> P120 + T76 -> P121 + T77 -> P122 + T78 -> P123 + T79 -> P124 + T80 -> P125 + T81 -> P127 + T82 -> P130 + T83 -> P131 + T84 -> P132 + T85 -> P133 + T86 -> P134 + T87 -> P135 + T88 -> P136 + T89 -> P137 + T90 -> P139 + T91 -> P140 + T92 -> P141 + T93 -> P142 + T94 -> P143 + T95 -> P144 + T96 -> P149 + T97 -> P150 + T98 -> P152 + T99 -> P159 + T100 -> P161 + T101 -> P162 + T102 -> P164 + T103 -> P165 + T104 -> P166 + T105 -> P168 + T106 -> P169 + T107 -> P170 + T108 -> P44 + T109 -> P47 + T110 -> P48 + T111 -> P49 + T112 -> P50 + T113 -> P51 + T114 -> P52 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac) + T21 -> (mac + linux + wsl) + T22 -> (mac + linux + wsl) + T23 -> (linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux + wsl) + T30 -> (mac + linux + wsl) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux + wsl) + T40 -> (mac + linux + wsl) + T41 -> (linux) + T42 -> (linux) + T43 -> (linux) + T44 -> (mac) + T45 -> (mac) + T46 -> (mac + linux + wsl) + T47 -> (mac + linux + wsl) + T48 -> (mac + linux) + T49 -> (mac + linux) + T50 -> (mac + linux) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux + windows) + T57 -> (mac + linux + windows) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux + wsl) + T66 -> (linux + wsl) + T67 -> (linux) + T68 -> (linux + wsl) + T69 -> (linux + wsl) + T70 -> (windows) + T71 -> (windows) + T72 -> (linux) + T73 -> (linux + wsl) + T74 -> (mac + linux + wsl) + T75 -> (windows) + T76 -> (mac + linux) + T77 -> (mac + linux) + T78 -> (windows) + T79 -> (linux) + T80 -> (mac) + T81 -> (wsl) + T82 -> (mac) + T83 -> (mac) + T84 -> (mac + linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux + wsl) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (mac) + T94 -> (mac) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T108 + H1 -> T109 + H2 -> T110 + H3 -> T111 + H4 -> T112 + H5 -> T113 + H6 -> T114 }
fun realAt: Retired -> Path { R0 -> P24 + R1 -> P29 + R2 -> P45 + R3 -> P46 + R4 -> P53 + R5 -> P54 + R6 -> P55 + R7 -> P56 + R8 -> P74 + R9 -> P75 + R10 -> P76 + R11 -> P77 + R12 -> P78 + R13 -> P81 + R14 -> P82 + R15 -> P85 + R16 -> P89 + R17 -> P91 + R18 -> P100 + R19 -> P101 + R20 -> P104 + R21 -> P105 + R22 -> P114 + R23 -> P118 + R24 -> P126 + R25 -> P128 + R26 -> P129 + R27 -> P135 + R28 -> P147 + R29 -> P154 + R30 -> P155 + R31 -> P156 + R32 -> P157 + R33 -> P158 + R34 -> P167 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac) + T21 -> (mac + linux + wsl) + T22 -> (mac + linux + wsl) + T23 -> (linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux + wsl) + T30 -> (mac + linux + wsl) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux + wsl) + T40 -> (mac + linux + wsl) + T41 -> (linux) + T42 -> (linux) + T43 -> (linux) + T44 -> (mac) + T45 -> (mac) + T46 -> (mac + linux + wsl) + T47 -> (mac + linux + wsl) + T48 -> (mac + linux) + T49 -> (mac + linux) + T50 -> (mac + linux) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux + windows) + T57 -> (mac + linux + windows) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux + wsl) + T66 -> (linux + wsl) + T67 -> (linux) + T68 -> (linux + wsl) + T69 -> (linux + wsl) + T70 -> (windows) + T71 -> (windows) + T72 -> (linux) + T73 -> (linux + wsl) + T74 -> (mac + linux + wsl) + T75 -> (windows) + T76 -> (mac + linux) + T77 -> (mac + linux) + T78 -> (windows) + T79 -> (linux) + T80 -> (mac) + T81 -> (wsl) + T82 -> (mac) + T83 -> (mac) + T84 -> (mac + linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux + wsl) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (mac) + T94 -> (mac) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T108 + H1 -> T109 + H2 -> T110 + H3 -> T111 + H4 -> T112 + H5 -> T113 + H6 -> T114 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P29 + R2 -> P45 + R3 -> P46 + R4 -> P53 + R5 -> P54 + R6 -> P55 + R7 -> P56 + R8 -> P74 + R9 -> P75 + R10 -> P76 + R11 -> P77 + R12 -> P78 + R13 -> P81 + R14 -> P82 + R15 -> P85 + R16 -> P89 + R17 -> P91 + R18 -> P100 + R19 -> P101 + R20 -> P104 + R21 -> P105 + R22 -> P114 + R23 -> P118 + R24 -> P126 + R25 -> P128 + R26 -> P129 + R27 -> P135 + R28 -> P147 + R29 -> P154 + R30 -> P155 + R31 -> P156 + R32 -> P157 + R33 -> P158 + R34 -> P167 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac) + T21 -> (mac + linux + wsl) + T22 -> (mac + linux + wsl) + T23 -> (linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux + wsl) + T30 -> (mac + linux + wsl) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux + wsl) + T40 -> (mac + linux + wsl) + T41 -> (linux) + T42 -> (linux) + T43 -> (linux) + T44 -> (mac) + T45 -> (mac) + T46 -> (mac + linux + wsl) + T47 -> (mac + linux + wsl) + T48 -> (mac + linux) + T49 -> (mac + linux) + T50 -> (mac + linux) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux + windows) + T57 -> (mac + linux + windows) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux + wsl) + T66 -> (linux + wsl) + T67 -> (linux) + T68 -> (linux + wsl) + T69 -> (linux + wsl) + T70 -> (windows) + T71 -> (windows) + T72 -> (linux) + T73 -> (linux + wsl) + T74 -> (mac + linux + wsl) + T75 -> (windows) + T76 -> (mac + linux) + T77 -> (mac + linux) + T78 -> (windows) + T79 -> (linux) + T80 -> (mac) + T81 -> (wsl) + T82 -> (mac) + T83 -> (mac) + T84 -> (mac + linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux + wsl) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (mac) + T94 -> (mac) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T108 + H1 -> T108 + H2 -> T110 + H3 -> T111 + H4 -> T112 + H5 -> T113 + H6 -> T114 }
fun plantedHookAt: Retired -> Path { R0 -> P24 + R1 -> P29 + R2 -> P45 + R3 -> P46 + R4 -> P53 + R5 -> P54 + R6 -> P55 + R7 -> P56 + R8 -> P74 + R9 -> P75 + R10 -> P76 + R11 -> P77 + R12 -> P78 + R13 -> P81 + R14 -> P82 + R15 -> P85 + R16 -> P89 + R17 -> P91 + R18 -> P100 + R19 -> P101 + R20 -> P104 + R21 -> P105 + R22 -> P114 + R23 -> P118 + R24 -> P126 + R25 -> P128 + R26 -> P129 + R27 -> P135 + R28 -> P147 + R29 -> P154 + R30 -> P155 + R31 -> P156 + R32 -> P157 + R33 -> P158 + R34 -> P167 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac) + T21 -> (mac + linux + wsl) + T22 -> (mac + linux + wsl) + T23 -> (linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux + wsl) + T30 -> (mac + linux + wsl) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux + wsl) + T40 -> (mac + linux + wsl) + T41 -> (linux) + T42 -> (linux) + T43 -> (linux) + T44 -> (mac) + T45 -> (mac) + T46 -> (mac + linux + wsl) + T47 -> (mac + linux + wsl) + T48 -> (mac + linux) + T49 -> (mac + linux) + T50 -> (mac + linux) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux + windows) + T57 -> (mac + linux + windows) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux + wsl) + T66 -> (linux + wsl) + T67 -> (linux) + T68 -> (linux + wsl) + T69 -> (linux + wsl) + T70 -> (windows) + T71 -> (windows) + T72 -> (linux) + T73 -> (linux + wsl) + T74 -> (mac + linux + wsl) + T75 -> (windows) + T76 -> (mac + linux) + T77 -> (mac + linux) + T78 -> (windows) + T79 -> (linux) + T80 -> (mac) + T81 -> (wsl) + T82 -> (mac) + T83 -> (mac) + T84 -> (mac + linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux + wsl) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (mac) + T94 -> (mac) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T108 + H1 -> T109 + H2 -> T110 + H3 -> T111 + H4 -> T112 + H5 -> T113 + H6 -> T114 }
fun plantedMaskAt: Retired -> Path { R0 -> P24 + R1 -> P29 + R2 -> P45 + R3 -> P46 + R4 -> P53 + R5 -> P54 + R6 -> P55 + R7 -> P56 + R8 -> P74 + R9 -> P75 + R10 -> P76 + R11 -> P77 + R12 -> P78 + R13 -> P81 + R14 -> P82 + R15 -> P85 + R16 -> P89 + R17 -> P91 + R18 -> P100 + R19 -> P101 + R20 -> P104 + R21 -> P105 + R22 -> P114 + R23 -> P118 + R24 -> P126 + R25 -> P128 + R26 -> P129 + R27 -> P135 + R28 -> P147 + R29 -> P154 + R30 -> P155 + R31 -> P156 + R32 -> P157 + R33 -> P158 + R34 -> P167 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (mac + linux + wsl) + R23 -> (windows) + R24 -> (mac) + R25 -> (mac + linux + wsl) + R26 -> (windows) + R27 -> (mac) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (Profile) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac) + T20 -> (mac) + T21 -> (mac + linux + wsl) + T22 -> (mac + linux + wsl) + T23 -> (linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac) + T28 -> (mac) + T29 -> (mac + linux + wsl) + T30 -> (mac + linux + wsl) + T31 -> (mac) + T32 -> (mac) + T33 -> (mac + linux) + T34 -> (mac) + T35 -> (mac) + T36 -> (mac + linux) + T37 -> (mac) + T38 -> (mac) + T39 -> (mac + linux + wsl) + T40 -> (mac + linux + wsl) + T41 -> (linux) + T42 -> (linux) + T43 -> (linux) + T44 -> (mac) + T45 -> (mac) + T46 -> (mac + linux + wsl) + T47 -> (mac + linux + wsl) + T48 -> (mac + linux) + T49 -> (mac + linux) + T50 -> (mac + linux) + T51 -> (mac) + T52 -> (mac + linux + wsl) + T53 -> (mac + linux + wsl) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux + windows) + T57 -> (mac + linux + windows) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux + wsl) + T62 -> (linux + wsl) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux + wsl) + T66 -> (linux + wsl) + T67 -> (linux) + T68 -> (linux + wsl) + T69 -> (linux + wsl) + T70 -> (windows) + T71 -> (windows) + T72 -> (linux) + T73 -> (linux + wsl) + T74 -> (mac + linux + wsl) + T75 -> (windows) + T76 -> (mac + linux) + T77 -> (mac + linux) + T78 -> (windows) + T79 -> (linux) + T80 -> (mac) + T81 -> (wsl) + T82 -> (mac) + T83 -> (mac) + T84 -> (mac + linux + wsl) + T85 -> (linux) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux + wsl) + T89 -> (linux) + T90 -> (linux) + T91 -> (linux) + T92 -> (linux) + T93 -> (mac) + T94 -> (mac) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (mac) + T100 -> (mac) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac + linux + windows + wsl) + T109 -> (mac + linux + windows + wsl) + T110 -> (mac + linux + windows + wsl) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T108 + H1 -> T109 + H2 -> T110 + H3 -> T111 + H4 -> T112 + H5 -> T113 + H6 -> T114 }
fun plantedEmptyAt: Retired -> Path { R0 -> P24 + R1 -> P29 + R2 -> P45 + R3 -> P46 + R4 -> P53 + R5 -> P54 + R6 -> P55 + R7 -> P56 + R8 -> P74 + R9 -> P75 + R10 -> P76 + R11 -> P77 + R12 -> P78 + R13 -> P81 + R14 -> P82 + R15 -> P85 + R16 -> P89 + R17 -> P91 + R18 -> P100 + R19 -> P101 + R20 -> P104 + R21 -> P105 + R22 -> P114 + R23 -> P118 + R24 -> P126 + R25 -> P128 + R26 -> P129 + R27 -> P135 + R28 -> P147 + R29 -> P154 + R30 -> P155 + R31 -> P156 + R32 -> P157 + R33 -> P158 + R34 -> P167 }
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
