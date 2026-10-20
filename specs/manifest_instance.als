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
// P28 is .config/bat
// P29 is .config/bat/config
// P30 is .config/borders
// P31 is .config/dev-reaper
// P32 is .config/dev-reaper/config
// P33 is .config/dotfiles
// P34 is .config/dotfiles-nix
// P35 is .config/dotfiles/notifications
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
// P73 is .config/mise
// P74 is .config/mise/tasks
// P75 is .config/mise/tasks/git-audit
// P76 is .config/nushell
// P77 is .config/nushell/autoload
// P78 is .config/nushell/autoload/atuin.nu
// P79 is .config/nushell/autoload/carapace.nu
// P80 is .config/nushell/autoload/starship.nu
// P81 is .config/nushell/autoload/zoxide.nu
// P82 is .config/nushell/config.nu
// P83 is .config/nushell/env.nu
// P84 is .config/nushell/login.nu
// P85 is .config/nushell/mise.nu
// P86 is .config/opencode
// P87 is .config/opencode/plugins
// P88 is .config/opencode/plugins/prose.ts
// P89 is .config/session-reaper
// P90 is .config/session-reaper/config
// P91 is .config/shell
// P92 is .config/shell/github-token.sh
// P93 is .config/shell/ssh-agent.sh
// P94 is .config/sketchybar
// P95 is .config/storage-scout
// P96 is .config/storage-scout/auto.toml
// P97 is .config/systemd
// P98 is .config/systemd/user
// P99 is .config/systemd/user/dev-cleanup.service
// P100 is .config/systemd/user/dev-cleanup.timer
// P101 is .config/systemd/user/dev-reaper.service
// P102 is .config/systemd/user/dev-reaper.timer
// P103 is .config/systemd/user/docker-prune.service
// P104 is .config/systemd/user/docker-prune.timer
// P105 is .config/systemd/user/dotfiles-desktop-refresh.service
// P106 is .config/systemd/user/dotfiles-desktop-refresh.timer
// P107 is .config/systemd/user/mise-upgrade.service
// P108 is .config/systemd/user/mise-upgrade.timer
// P109 is .config/systemd/user/session-reaper.service
// P110 is .config/systemd/user/session-reaper.timer
// P111 is .config/systemd/user/storage-scout.service
// P112 is .config/systemd/user/tools-upgrade.service
// P113 is .config/systemd/user/tools-upgrade.timer
// P114 is .config/wezterm
// P115 is .config/wezterm/wezterm.lua
// P116 is .config/xdg-terminals.list
// P117 is .gitconfig
// P118 is .local
// P119 is .local/bin
// P120 is .local/bin/dev-reaper
// P121 is .local/bin/dotctl.exe
// P122 is .local/bin/dotfiles-doctor
// P123 is .local/bin/dotfiles-doctor.ps1
// P124 is .local/bin/dotfiles-refresh-desktop
// P125 is .local/bin/dotfiles-refresh-integrations
// P126 is .local/bin/dotfiles-refresh-integrations.ps1
// P127 is .local/bin/dotfiles-retire-tmux
// P128 is .local/bin/dotfiles-touchid
// P129 is .local/bin/dotfiles-wm-defaults
// P130 is .local/bin/dotfiles-wsl-ssh-keygen
// P131 is .local/bin/dotfiles-xtask
// P132 is .local/bin/dotfiles-xtask.exe
// P133 is .local/bin/ghostty-herdr
// P134 is .local/bin/ghostty-nu
// P135 is .local/bin/git
// P136 is .local/bin/kitty-herdr
// P137 is .local/bin/kitty-nu
// P138 is .local/bin/renovate-local
// P139 is .local/bin/session-reaper
// P140 is .local/share
// P141 is .local/share/applications
// P142 is .local/share/applications/kitty-herdr.desktop
// P143 is .local/share/applications/kitty-nu.desktop
// P144 is .profile
// P145 is .zprofile
// P146 is .zshrc
// P147 is AppData
// P148 is AppData/Roaming
// P149 is AppData/Roaming/alacritty
// P150 is AppData/Roaming/alacritty/alacritty.toml
// P151 is AppData/Roaming/herdr
// P152 is AppData/Roaming/herdr/config.toml
// P153 is Documents
// P154 is Documents/PowerShell
// P155 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P156 is Documents/PowerShell/modules
// P157 is Documents/PowerShell/modules/00-readline.ps1
// P158 is Documents/PowerShell/modules/05-mise.ps1
// P159 is Documents/PowerShell/modules/10-tools.ps1
// P160 is Documents/PowerShell/modules/20-herdr.ps1
// P161 is Documents/PowerShell/modules/99-prompt.ps1
// P162 is Library
// P163 is Library/Application Support
// P164 is Library/Application Support/lazygit
// P165 is Library/Application Support/nushell
// P166 is Library/LaunchAgents
// P167 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P168 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P169 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P170 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P171 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P172 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P173 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166, P167, P168, P169, P170, P171, P172, P173 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119, T120 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P17 -> P5 + P19 -> P18 + P20 -> P18 + P21 -> P20 + P22 -> P18 + P23 -> P22 + P25 -> P24 + P26 -> P24 + P27 -> P26 + P28 -> P24 + P29 -> P28 + P30 -> P24 + P31 -> P24 + P32 -> P31 + P33 -> P24 + P34 -> P24 + P35 -> P33 + P36 -> P24 + P37 -> P36 + P38 -> P36 + P39 -> P24 + P40 -> P39 + P41 -> P24 + P42 -> P41 + P43 -> P24 + P44 -> P43 + P45 -> P24 + P46 -> P45 + P47 -> P46 + P48 -> P46 + P49 -> P46 + P50 -> P46 + P51 -> P46 + P52 -> P46 + P53 -> P46 + P54 -> P46 + P55 -> P46 + P56 -> P46 + P57 -> P46 + P58 -> P46 + P59 -> P45 + P60 -> P24 + P61 -> P24 + P62 -> P61 + P63 -> P60 + P64 -> P24 + P65 -> P64 + P66 -> P24 + P67 -> P66 + P68 -> P24 + P69 -> P68 + P70 -> P68 + P71 -> P24 + P72 -> P71 + P73 -> P24 + P74 -> P73 + P75 -> P74 + P76 -> P24 + P77 -> P76 + P78 -> P77 + P79 -> P77 + P80 -> P77 + P81 -> P77 + P82 -> P76 + P83 -> P76 + P84 -> P76 + P85 -> P76 + P86 -> P24 + P87 -> P86 + P88 -> P87 + P89 -> P24 + P90 -> P89 + P91 -> P24 + P92 -> P91 + P93 -> P91 + P94 -> P24 + P95 -> P24 + P96 -> P95 + P97 -> P24 + P98 -> P97 + P99 -> P98 + P100 -> P98 + P101 -> P98 + P102 -> P98 + P103 -> P98 + P104 -> P98 + P105 -> P98 + P106 -> P98 + P107 -> P98 + P108 -> P98 + P109 -> P98 + P110 -> P98 + P111 -> P98 + P112 -> P98 + P113 -> P98 + P114 -> P24 + P115 -> P114 + P116 -> P24 + P119 -> P118 + P120 -> P119 + P121 -> P119 + P122 -> P119 + P123 -> P119 + P124 -> P119 + P125 -> P119 + P126 -> P119 + P127 -> P119 + P128 -> P119 + P129 -> P119 + P130 -> P119 + P131 -> P119 + P132 -> P119 + P133 -> P119 + P134 -> P119 + P135 -> P119 + P136 -> P119 + P137 -> P119 + P138 -> P119 + P139 -> P119 + P140 -> P118 + P141 -> P140 + P142 -> P141 + P143 -> P141 + P148 -> P147 + P149 -> P148 + P150 -> P149 + P151 -> P148 + P152 -> P151 + P154 -> P153 + P155 -> P154 + P156 -> P154 + P157 -> P156 + P158 -> P156 + P159 -> P156 + P160 -> P156 + P161 -> P156 + P163 -> P162 + P164 -> P163 + P165 -> P163 + P166 -> P162 + P167 -> P166 + P168 -> P166 + P169 -> P166 + P170 -> P166 + P171 -> P166 + P172 -> P166 + P173 -> P166 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P17 + T14 -> P19 + T15 -> P20 + T16 -> P21 + T17 -> P23 + T18 -> P26 + T19 -> P27 + T20 -> P28 + T21 -> P29 + T22 -> P31 + T23 -> P32 + T24 -> P34 + T25 -> P35 + T26 -> P36 + T27 -> P37 + T28 -> P38 + T29 -> P39 + T30 -> P40 + T31 -> P41 + T32 -> P42 + T33 -> P43 + T34 -> P44 + T35 -> P60 + T36 -> P61 + T37 -> P62 + T38 -> P63 + T39 -> P64 + T40 -> P65 + T41 -> P66 + T42 -> P67 + T43 -> P68 + T44 -> P69 + T45 -> P70 + T46 -> P71 + T47 -> P72 + T48 -> P74 + T49 -> P75 + T50 -> P76 + T51 -> P82 + T52 -> P83 + T53 -> P84 + T54 -> P89 + T55 -> P90 + T56 -> P91 + T57 -> P93 + T58 -> P95 + T59 -> P96 + T60 -> P97 + T61 -> P99 + T62 -> P100 + T63 -> P101 + T64 -> P102 + T65 -> P103 + T66 -> P104 + T67 -> P105 + T68 -> P106 + T69 -> P107 + T70 -> P108 + T71 -> P109 + T72 -> P110 + T73 -> P111 + T74 -> P112 + T75 -> P113 + T76 -> P114 + T77 -> P115 + T78 -> P116 + T79 -> P120 + T80 -> P122 + T81 -> P123 + T82 -> P124 + T83 -> P125 + T84 -> P126 + T85 -> P127 + T86 -> P128 + T87 -> P130 + T88 -> P133 + T89 -> P134 + T90 -> P135 + T91 -> P136 + T92 -> P137 + T93 -> P138 + T94 -> P139 + T95 -> P140 + T96 -> P142 + T97 -> P143 + T98 -> P144 + T99 -> P145 + T100 -> P146 + T101 -> P147 + T102 -> P152 + T103 -> P153 + T104 -> P155 + T105 -> P162 + T106 -> P164 + T107 -> P165 + T108 -> P167 + T109 -> P168 + T110 -> P169 + T111 -> P171 + T112 -> P172 + T113 -> P173 + T114 -> P47 + T115 -> P50 + T116 -> P51 + T117 -> P52 + T118 -> P53 + T119 -> P54 + T120 -> P55 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac) + T21 -> (mac) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (mac + linux + windows) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux + wsl) + T32 -> (mac + linux + wsl) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux) + T36 -> (mac) + T37 -> (mac) + T38 -> (mac + linux) + T39 -> (mac) + T40 -> (mac) + T41 -> (mac + linux + wsl) + T42 -> (mac + linux + wsl) + T43 -> (linux) + T44 -> (linux) + T45 -> (linux) + T46 -> (mac) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac + linux) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux) + T66 -> (linux) + T67 -> (linux) + T68 -> (linux) + T69 -> (wsl) + T70 -> (wsl) + T71 -> (linux) + T72 -> (linux) + T73 -> (linux) + T74 -> (linux) + T75 -> (linux) + T76 -> (windows) + T77 -> (windows) + T78 -> (linux) + T79 -> (linux) + T80 -> (mac + linux + wsl) + T81 -> (windows) + T82 -> (mac + linux) + T83 -> (mac + linux) + T84 -> (windows) + T85 -> (linux) + T86 -> (mac) + T87 -> (wsl) + T88 -> (mac) + T89 -> (mac) + T90 -> (mac + linux + wsl) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (linux) + T97 -> (linux) + T98 -> (linux) + T99 -> (mac) + T100 -> (mac) + T101 -> (windows) + T102 -> (windows) + T103 -> (windows) + T104 -> (windows) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac) + T112 -> (mac) + T113 -> (mac) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) + T118 -> (mac + linux + windows + wsl) + T119 -> (mac + linux + windows + wsl) + T120 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T114 + H1 -> T115 + H2 -> T116 + H3 -> T117 + H4 -> T118 + H5 -> T119 + H6 -> T120 }
fun realAt: Retired -> Path { R0 -> P25 + R1 -> P30 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P77 + R9 -> P78 + R10 -> P79 + R11 -> P80 + R12 -> P81 + R13 -> P84 + R14 -> P85 + R15 -> P88 + R16 -> P92 + R17 -> P94 + R18 -> P107 + R19 -> P108 + R20 -> P117 + R21 -> P121 + R22 -> P129 + R23 -> P131 + R24 -> P132 + R25 -> P138 + R26 -> P150 + R27 -> P157 + R28 -> P158 + R29 -> P159 + R30 -> P160 + R31 -> P161 + R32 -> P170 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac) + T21 -> (mac) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (mac + linux + windows) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux + wsl) + T32 -> (mac + linux + wsl) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux) + T36 -> (mac) + T37 -> (mac) + T38 -> (mac + linux) + T39 -> (mac) + T40 -> (mac) + T41 -> (mac + linux + wsl) + T42 -> (mac + linux + wsl) + T43 -> (linux) + T44 -> (linux) + T45 -> (linux) + T46 -> (mac) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac + linux) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux) + T66 -> (linux) + T67 -> (linux) + T68 -> (linux) + T69 -> (wsl) + T70 -> (wsl) + T71 -> (linux) + T72 -> (linux) + T73 -> (linux) + T74 -> (linux) + T75 -> (linux) + T76 -> (windows) + T77 -> (windows) + T78 -> (linux) + T79 -> (linux) + T80 -> (mac + linux + wsl) + T81 -> (windows) + T82 -> (mac + linux) + T83 -> (mac + linux) + T84 -> (windows) + T85 -> (linux) + T86 -> (mac) + T87 -> (wsl) + T88 -> (mac) + T89 -> (mac) + T90 -> (mac + linux + wsl) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (linux) + T97 -> (linux) + T98 -> (linux) + T99 -> (mac) + T100 -> (mac) + T101 -> (windows) + T102 -> (windows) + T103 -> (windows) + T104 -> (windows) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac) + T112 -> (mac) + T113 -> (mac) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) + T118 -> (mac + linux + windows + wsl) + T119 -> (mac + linux + windows + wsl) + T120 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T114 + H1 -> T115 + H2 -> T116 + H3 -> T117 + H4 -> T118 + H5 -> T119 + H6 -> T120 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P30 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P77 + R9 -> P78 + R10 -> P79 + R11 -> P80 + R12 -> P81 + R13 -> P84 + R14 -> P85 + R15 -> P88 + R16 -> P92 + R17 -> P94 + R18 -> P107 + R19 -> P108 + R20 -> P117 + R21 -> P121 + R22 -> P129 + R23 -> P131 + R24 -> P132 + R25 -> P138 + R26 -> P150 + R27 -> P157 + R28 -> P158 + R29 -> P159 + R30 -> P160 + R31 -> P161 + R32 -> P170 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac) + T21 -> (mac) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (mac + linux + windows) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux + wsl) + T32 -> (mac + linux + wsl) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux) + T36 -> (mac) + T37 -> (mac) + T38 -> (mac + linux) + T39 -> (mac) + T40 -> (mac) + T41 -> (mac + linux + wsl) + T42 -> (mac + linux + wsl) + T43 -> (linux) + T44 -> (linux) + T45 -> (linux) + T46 -> (mac) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac + linux) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux) + T66 -> (linux) + T67 -> (linux) + T68 -> (linux) + T69 -> (wsl) + T70 -> (wsl) + T71 -> (linux) + T72 -> (linux) + T73 -> (linux) + T74 -> (linux) + T75 -> (linux) + T76 -> (windows) + T77 -> (windows) + T78 -> (linux) + T79 -> (linux) + T80 -> (mac + linux + wsl) + T81 -> (windows) + T82 -> (mac + linux) + T83 -> (mac + linux) + T84 -> (windows) + T85 -> (linux) + T86 -> (mac) + T87 -> (wsl) + T88 -> (mac) + T89 -> (mac) + T90 -> (mac + linux + wsl) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (linux) + T97 -> (linux) + T98 -> (linux) + T99 -> (mac) + T100 -> (mac) + T101 -> (windows) + T102 -> (windows) + T103 -> (windows) + T104 -> (windows) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac) + T112 -> (mac) + T113 -> (mac) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) + T118 -> (mac + linux + windows + wsl) + T119 -> (mac + linux + windows + wsl) + T120 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T114 + H1 -> T114 + H2 -> T116 + H3 -> T117 + H4 -> T118 + H5 -> T119 + H6 -> T120 }
fun plantedHookAt: Retired -> Path { R0 -> P25 + R1 -> P30 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P77 + R9 -> P78 + R10 -> P79 + R11 -> P80 + R12 -> P81 + R13 -> P84 + R14 -> P85 + R15 -> P88 + R16 -> P92 + R17 -> P94 + R18 -> P107 + R19 -> P108 + R20 -> P117 + R21 -> P121 + R22 -> P129 + R23 -> P131 + R24 -> P132 + R25 -> P138 + R26 -> P150 + R27 -> P157 + R28 -> P158 + R29 -> P159 + R30 -> P160 + R31 -> P161 + R32 -> P170 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac) + T21 -> (mac) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (mac + linux + windows) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux + wsl) + T32 -> (mac + linux + wsl) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux) + T36 -> (mac) + T37 -> (mac) + T38 -> (mac + linux) + T39 -> (mac) + T40 -> (mac) + T41 -> (mac + linux + wsl) + T42 -> (mac + linux + wsl) + T43 -> (linux) + T44 -> (linux) + T45 -> (linux) + T46 -> (mac) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac + linux) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux) + T66 -> (linux) + T67 -> (linux) + T68 -> (linux) + T69 -> (wsl) + T70 -> (wsl) + T71 -> (linux) + T72 -> (linux) + T73 -> (linux) + T74 -> (linux) + T75 -> (linux) + T76 -> (windows) + T77 -> (windows) + T78 -> (linux) + T79 -> (linux) + T80 -> (mac + linux + wsl) + T81 -> (windows) + T82 -> (mac + linux) + T83 -> (mac + linux) + T84 -> (windows) + T85 -> (linux) + T86 -> (mac) + T87 -> (wsl) + T88 -> (mac) + T89 -> (mac) + T90 -> (mac + linux + wsl) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (linux) + T97 -> (linux) + T98 -> (linux) + T99 -> (mac) + T100 -> (mac) + T101 -> (windows) + T102 -> (windows) + T103 -> (windows) + T104 -> (windows) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac) + T112 -> (mac) + T113 -> (mac) + T114 -> (mac) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) + T118 -> (mac + linux + windows + wsl) + T119 -> (mac + linux + windows + wsl) + T120 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T114 + H1 -> T115 + H2 -> T116 + H3 -> T117 + H4 -> T118 + H5 -> T119 + H6 -> T120 }
fun plantedMaskAt: Retired -> Path { R0 -> P25 + R1 -> P30 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P77 + R9 -> P78 + R10 -> P79 + R11 -> P80 + R12 -> P81 + R13 -> P84 + R14 -> P85 + R15 -> P88 + R16 -> P92 + R17 -> P94 + R18 -> P107 + R19 -> P108 + R20 -> P117 + R21 -> P121 + R22 -> P129 + R23 -> P131 + R24 -> P132 + R25 -> P138 + R26 -> P150 + R27 -> P157 + R28 -> P158 + R29 -> P159 + R30 -> P160 + R31 -> P161 + R32 -> P170 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (mac + linux + wsl) + R21 -> (windows) + R22 -> (mac) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (windows) + R27 -> (windows) + R28 -> (windows) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac) + T8 -> (mac + linux) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (mac) + T13 -> (linux) + T14 -> (mac + linux + wsl) + T15 -> (Profile) + T16 -> (mac + linux + wsl) + T17 -> (mac) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac) + T21 -> (mac) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (mac + linux + windows) + T26 -> (linux) + T27 -> (linux) + T28 -> (linux) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux + wsl) + T32 -> (mac + linux + wsl) + T33 -> (mac) + T34 -> (mac) + T35 -> (mac + linux) + T36 -> (mac) + T37 -> (mac) + T38 -> (mac + linux) + T39 -> (mac) + T40 -> (mac) + T41 -> (mac + linux + wsl) + T42 -> (mac + linux + wsl) + T43 -> (linux) + T44 -> (linux) + T45 -> (linux) + T46 -> (mac) + T47 -> (mac) + T48 -> (mac + linux + wsl) + T49 -> (mac + linux + wsl) + T50 -> (mac + linux) + T51 -> (mac + linux) + T52 -> (mac + linux) + T53 -> (mac) + T54 -> (mac + linux) + T55 -> (mac + linux) + T56 -> (mac + linux) + T57 -> (mac + linux) + T58 -> (mac + linux + windows) + T59 -> (mac + linux + windows) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux) + T65 -> (linux) + T66 -> (linux) + T67 -> (linux) + T68 -> (linux) + T69 -> (wsl) + T70 -> (wsl) + T71 -> (linux) + T72 -> (linux) + T73 -> (linux) + T74 -> (linux) + T75 -> (linux) + T76 -> (windows) + T77 -> (windows) + T78 -> (linux) + T79 -> (linux) + T80 -> (mac + linux + wsl) + T81 -> (windows) + T82 -> (mac + linux) + T83 -> (mac + linux) + T84 -> (windows) + T85 -> (linux) + T86 -> (mac) + T87 -> (wsl) + T88 -> (mac) + T89 -> (mac) + T90 -> (mac + linux + wsl) + T91 -> (linux) + T92 -> (linux) + T93 -> (linux) + T94 -> (linux) + T95 -> (linux) + T96 -> (linux) + T97 -> (linux) + T98 -> (linux) + T99 -> (mac) + T100 -> (mac) + T101 -> (windows) + T102 -> (windows) + T103 -> (windows) + T104 -> (windows) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac) + T112 -> (mac) + T113 -> (mac) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) + T118 -> (mac + linux + windows + wsl) + T119 -> (mac + linux + windows + wsl) + T120 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T114 + H1 -> T115 + H2 -> T116 + H3 -> T117 + H4 -> T118 + H5 -> T119 + H6 -> T120 }
fun plantedEmptyAt: Retired -> Path { R0 -> P25 + R1 -> P30 + R2 -> P48 + R3 -> P49 + R4 -> P56 + R5 -> P57 + R6 -> P58 + R7 -> P59 + R8 -> P77 + R9 -> P78 + R10 -> P79 + R11 -> P80 + R12 -> P81 + R13 -> P84 + R14 -> P85 + R15 -> P88 + R16 -> P92 + R17 -> P94 + R18 -> P107 + R19 -> P108 + R20 -> P117 + R21 -> P121 + R22 -> P129 + R23 -> P131 + R24 -> P132 + R25 -> P138 + R26 -> P150 + R27 -> P157 + R28 -> P158 + R29 -> P159 + R30 -> P160 + R31 -> P161 + R32 -> P170 }
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
