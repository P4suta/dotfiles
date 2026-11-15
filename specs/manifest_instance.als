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
// P30 is .config/dotfiles
// P31 is .config/dotfiles-nix
// P32 is .config/dotfiles/signing.toml
// P33 is .config/dotfiles/ssh-agent.conf
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
// P103 is .config/systemd/user/herdr-agent.service
// P104 is .config/systemd/user/mise-upgrade.service
// P105 is .config/systemd/user/mise-upgrade.timer
// P106 is .config/systemd/user/session-reaper.service
// P107 is .config/systemd/user/session-reaper.timer
// P108 is .config/systemd/user/storage-scout.service
// P109 is .config/systemd/user/tools-upgrade.service
// P110 is .config/systemd/user/tools-upgrade.timer
// P111 is .config/wezterm
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
// P136 is .local/bin/wezterm.cmd
// P137 is .local/share
// P138 is .local/share/applications
// P139 is .local/share/applications/kitty-herdr.desktop
// P140 is .local/share/applications/kitty-nu.desktop
// P141 is .profile
// P142 is .ssh
// P143 is .ssh/config
// P144 is .zprofile
// P145 is .zshrc
// P146 is AppData
// P147 is AppData/Local
// P148 is AppData/Local/Programs
// P149 is AppData/Local/Programs/WezTerm-nightly
// P150 is AppData/Local/lazygit
// P151 is AppData/Local/lazygit/config.yml
// P152 is AppData/Roaming
// P153 is AppData/Roaming/Microsoft
// P154 is AppData/Roaming/Microsoft/Windows
// P155 is AppData/Roaming/Microsoft/Windows/Start Menu
// P156 is AppData/Roaming/Microsoft/Windows/Start Menu/Programs
// P157 is AppData/Roaming/Microsoft/Windows/Start Menu/Programs/WezTerm Nightly.lnk
// P158 is AppData/Roaming/alacritty
// P159 is AppData/Roaming/alacritty/alacritty.toml
// P160 is AppData/Roaming/herdr
// P161 is AppData/Roaming/herdr/config.toml
// P162 is Documents
// P163 is Documents/PowerShell
// P164 is Documents/PowerShell/Microsoft.PowerShell_profile.ps1
// P165 is Documents/PowerShell/modules
// P166 is Documents/PowerShell/modules/00-readline.ps1
// P167 is Documents/PowerShell/modules/05-mise.ps1
// P168 is Documents/PowerShell/modules/10-tools.ps1
// P169 is Documents/PowerShell/modules/20-herdr.ps1
// P170 is Documents/PowerShell/modules/99-prompt.ps1
// P171 is Library
// P172 is Library/Application Support
// P173 is Library/Application Support/lazygit
// P174 is Library/Application Support/nushell
// P175 is Library/LaunchAgents
// P176 is Library/LaunchAgents/dev.dotfiles.desktop-refresh.plist
// P177 is Library/LaunchAgents/dev.dotfiles.dev-cleanup.plist
// P178 is Library/LaunchAgents/dev.dotfiles.dev-reaper.plist
// P179 is Library/LaunchAgents/dev.dotfiles.herdr-agent.plist
// P180 is Library/LaunchAgents/dev.dotfiles.mise-upgrade.plist
// P181 is Library/LaunchAgents/dev.dotfiles.session-reaper.plist
// P182 is Library/LaunchAgents/dev.dotfiles.storage-scout.plist
// P183 is Library/LaunchAgents/dev.dotfiles.tools-upgrade.plist
one sig P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15, P16, P17, P18, P19, P20, P21, P22, P23, P24, P25, P26, P27, P28, P29, P30, P31, P32, P33, P34, P35, P36, P37, P38, P39, P40, P41, P42, P43, P44, P45, P46, P47, P48, P49, P50, P51, P52, P53, P54, P55, P56, P57, P58, P59, P60, P61, P62, P63, P64, P65, P66, P67, P68, P69, P70, P71, P72, P73, P74, P75, P76, P77, P78, P79, P80, P81, P82, P83, P84, P85, P86, P87, P88, P89, P90, P91, P92, P93, P94, P95, P96, P97, P98, P99, P100, P101, P102, P103, P104, P105, P106, P107, P108, P109, P110, P111, P112, P113, P114, P115, P116, P117, P118, P119, P120, P121, P122, P123, P124, P125, P126, P127, P128, P129, P130, P131, P132, P133, P134, P135, P136, P137, P138, P139, P140, P141, P142, P143, P144, P145, P146, P147, P148, P149, P150, P151, P152, P153, P154, P155, P156, P157, P158, P159, P160, P161, P162, P163, P164, P165, P166, P167, P168, P169, P170, P171, P172, P173, P174, P175, P176, P177, P178, P179, P180, P181, P182, P183 extends Path {}
one sig T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108, T109, T110, T111, T112, T113, T114, T115, T116, T117 extends Target {}
one sig R0, R1, R2, R3, R4, R5, R6, R7, R8, R9, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24, R25, R26, R27, R28, R29, R30, R31, R32, R33, R34, R35, R36, R37 extends Retired {}
one sig H0, H1, H2, H3, H4, H5, H6 extends Hook {}

fun parents: Path -> Path { P1 -> P0 + P2 -> P1 + P5 -> P4 + P6 -> P5 + P7 -> P5 + P8 -> P5 + P9 -> P5 + P10 -> P5 + P11 -> P5 + P12 -> P5 + P13 -> P5 + P14 -> P5 + P15 -> P5 + P16 -> P5 + P18 -> P17 + P19 -> P17 + P20 -> P19 + P21 -> P17 + P22 -> P21 + P24 -> P23 + P25 -> P23 + P26 -> P25 + P27 -> P23 + P28 -> P23 + P29 -> P28 + P30 -> P23 + P31 -> P23 + P32 -> P30 + P33 -> P30 + P34 -> P23 + P35 -> P34 + P36 -> P34 + P37 -> P23 + P38 -> P37 + P39 -> P23 + P40 -> P39 + P41 -> P23 + P42 -> P41 + P43 -> P42 + P44 -> P42 + P45 -> P42 + P46 -> P42 + P47 -> P42 + P48 -> P42 + P49 -> P42 + P50 -> P42 + P51 -> P42 + P52 -> P42 + P53 -> P42 + P54 -> P42 + P55 -> P41 + P56 -> P23 + P57 -> P23 + P58 -> P57 + P59 -> P56 + P60 -> P23 + P61 -> P60 + P62 -> P23 + P63 -> P62 + P64 -> P23 + P65 -> P64 + P66 -> P64 + P67 -> P23 + P68 -> P67 + P69 -> P23 + P70 -> P69 + P71 -> P70 + P72 -> P23 + P73 -> P72 + P74 -> P73 + P75 -> P73 + P76 -> P73 + P77 -> P73 + P78 -> P72 + P79 -> P72 + P80 -> P72 + P81 -> P72 + P82 -> P23 + P83 -> P82 + P84 -> P83 + P85 -> P23 + P86 -> P85 + P87 -> P23 + P88 -> P87 + P89 -> P87 + P90 -> P23 + P91 -> P23 + P92 -> P91 + P93 -> P23 + P94 -> P93 + P95 -> P94 + P96 -> P94 + P97 -> P94 + P98 -> P94 + P99 -> P94 + P100 -> P94 + P101 -> P94 + P102 -> P94 + P103 -> P94 + P104 -> P94 + P105 -> P94 + P106 -> P94 + P107 -> P94 + P108 -> P94 + P109 -> P94 + P110 -> P94 + P111 -> P23 + P112 -> P23 + P115 -> P114 + P116 -> P115 + P117 -> P115 + P118 -> P115 + P119 -> P115 + P120 -> P115 + P121 -> P115 + P122 -> P115 + P123 -> P115 + P124 -> P115 + P125 -> P115 + P126 -> P115 + P127 -> P115 + P128 -> P115 + P129 -> P115 + P130 -> P115 + P131 -> P115 + P132 -> P115 + P133 -> P115 + P134 -> P115 + P135 -> P115 + P136 -> P115 + P137 -> P114 + P138 -> P137 + P139 -> P138 + P140 -> P138 + P143 -> P142 + P147 -> P146 + P148 -> P147 + P149 -> P148 + P150 -> P147 + P151 -> P150 + P152 -> P146 + P153 -> P152 + P154 -> P153 + P155 -> P154 + P156 -> P155 + P157 -> P156 + P158 -> P152 + P159 -> P158 + P160 -> P152 + P161 -> P160 + P163 -> P162 + P164 -> P163 + P165 -> P163 + P166 -> P165 + P167 -> P165 + P168 -> P165 + P169 -> P165 + P170 -> P165 + P172 -> P171 + P173 -> P172 + P174 -> P172 + P175 -> P171 + P176 -> P175 + P177 -> P175 + P178 -> P175 + P179 -> P175 + P180 -> P175 + P181 -> P175 + P182 -> P175 + P183 -> P175 }
fun paths: Target -> Path { T0 -> P2 + T1 -> P3 + T2 -> P6 + T3 -> P7 + T4 -> P8 + T5 -> P9 + T6 -> P10 + T7 -> P11 + T8 -> P12 + T9 -> P13 + T10 -> P14 + T11 -> P15 + T12 -> P16 + T13 -> P18 + T14 -> P19 + T15 -> P20 + T16 -> P22 + T17 -> P25 + T18 -> P26 + T19 -> P28 + T20 -> P29 + T21 -> P31 + T22 -> P32 + T23 -> P33 + T24 -> P34 + T25 -> P35 + T26 -> P36 + T27 -> P37 + T28 -> P38 + T29 -> P39 + T30 -> P40 + T31 -> P56 + T32 -> P57 + T33 -> P58 + T34 -> P59 + T35 -> P60 + T36 -> P61 + T37 -> P62 + T38 -> P63 + T39 -> P64 + T40 -> P65 + T41 -> P66 + T42 -> P67 + T43 -> P68 + T44 -> P70 + T45 -> P71 + T46 -> P72 + T47 -> P78 + T48 -> P79 + T49 -> P80 + T50 -> P85 + T51 -> P86 + T52 -> P87 + T53 -> P89 + T54 -> P91 + T55 -> P92 + T56 -> P93 + T57 -> P95 + T58 -> P96 + T59 -> P97 + T60 -> P98 + T61 -> P101 + T62 -> P102 + T63 -> P103 + T64 -> P106 + T65 -> P107 + T66 -> P108 + T67 -> P109 + T68 -> P110 + T69 -> P112 + T70 -> P116 + T71 -> P118 + T72 -> P119 + T73 -> P120 + T74 -> P121 + T75 -> P122 + T76 -> P123 + T77 -> P124 + T78 -> P126 + T79 -> P129 + T80 -> P130 + T81 -> P131 + T82 -> P132 + T83 -> P133 + T84 -> P134 + T85 -> P135 + T86 -> P137 + T87 -> P139 + T88 -> P140 + T89 -> P141 + T90 -> P142 + T91 -> P143 + T92 -> P144 + T93 -> P145 + T94 -> P146 + T95 -> P151 + T96 -> P158 + T97 -> P159 + T98 -> P161 + T99 -> P162 + T100 -> P164 + T101 -> P171 + T102 -> P173 + T103 -> P174 + T104 -> P176 + T105 -> P177 + T106 -> P178 + T107 -> P179 + T108 -> P181 + T109 -> P182 + T110 -> P183 + T111 -> P43 + T112 -> P46 + T113 -> P47 + T114 -> P48 + T115 -> P49 + T116 -> P50 + T117 -> P51 }
fun realReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac + linux) + T33 -> (mac + linux) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux + wsl) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (linux) + T67 -> (linux + wsl) + T68 -> (linux + wsl) + T69 -> (linux) + T70 -> (linux + wsl) + T71 -> (mac + linux + wsl) + T72 -> (windows) + T73 -> (mac + linux) + T74 -> (mac + linux) + T75 -> (windows) + T76 -> (linux) + T77 -> (mac) + T78 -> (wsl) + T79 -> (mac) + T80 -> (mac) + T81 -> (mac + linux + wsl) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux) + T85 -> (linux + wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (mac + linux) + T91 -> (mac + linux) + T92 -> (mac) + T93 -> (mac) + T94 -> (windows) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) }
fun realFiles: Hook -> Target { H0 -> T111 + H1 -> T112 + H2 -> T113 + H3 -> T114 + H4 -> T115 + H5 -> T116 + H6 -> T117 }
fun realAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P99 + R19 -> P100 + R20 -> P104 + R21 -> P105 + R22 -> P111 + R23 -> P113 + R24 -> P117 + R25 -> P125 + R26 -> P127 + R27 -> P128 + R28 -> P134 + R29 -> P136 + R30 -> P149 + R31 -> P157 + R32 -> P166 + R33 -> P167 + R34 -> P168 + R35 -> P169 + R36 -> P170 + R37 -> P180 }
fun realRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (windows) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (mac + linux + wsl) + R27 -> (windows) + R28 -> (mac) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (windows) + R35 -> (windows) + R36 -> (windows) + R37 -> (mac) }
pred real { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first retired entry moved onto the first target, which a validator refuses.
fun plantedOverlapReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac + linux) + T33 -> (mac + linux) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux + wsl) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (linux) + T67 -> (linux + wsl) + T68 -> (linux + wsl) + T69 -> (linux) + T70 -> (linux + wsl) + T71 -> (mac + linux + wsl) + T72 -> (windows) + T73 -> (mac + linux) + T74 -> (mac + linux) + T75 -> (windows) + T76 -> (linux) + T77 -> (mac) + T78 -> (wsl) + T79 -> (mac) + T80 -> (mac) + T81 -> (mac + linux + wsl) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux) + T85 -> (linux + wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (mac + linux) + T91 -> (mac + linux) + T92 -> (mac) + T93 -> (mac) + T94 -> (windows) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) }
fun plantedOverlapFiles: Hook -> Target { H0 -> T111 + H1 -> T112 + H2 -> T113 + H3 -> T114 + H4 -> T115 + H5 -> T116 + H6 -> T117 }
fun plantedOverlapAt: Retired -> Path { R0 -> P2 + R1 -> P27 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P99 + R19 -> P100 + R20 -> P104 + R21 -> P105 + R22 -> P111 + R23 -> P113 + R24 -> P117 + R25 -> P125 + R26 -> P127 + R27 -> P128 + R28 -> P134 + R29 -> P136 + R30 -> P149 + R31 -> P157 + R32 -> P166 + R33 -> P167 + R34 -> P168 + R35 -> P169 + R36 -> P170 + R37 -> P180 }
fun plantedOverlapRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (windows) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (mac + linux + wsl) + R27 -> (windows) + R28 -> (mac) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (windows) + R35 -> (windows) + R36 -> (windows) + R37 -> (mac) }
pred plantedOverlap { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the second hook run from the first hook's file, which a validator refuses.
fun plantedHookReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac + linux) + T33 -> (mac + linux) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux + wsl) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (linux) + T67 -> (linux + wsl) + T68 -> (linux + wsl) + T69 -> (linux) + T70 -> (linux + wsl) + T71 -> (mac + linux + wsl) + T72 -> (windows) + T73 -> (mac + linux) + T74 -> (mac + linux) + T75 -> (windows) + T76 -> (linux) + T77 -> (mac) + T78 -> (wsl) + T79 -> (mac) + T80 -> (mac) + T81 -> (mac + linux + wsl) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux) + T85 -> (linux + wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (mac + linux) + T91 -> (mac + linux) + T92 -> (mac) + T93 -> (mac) + T94 -> (windows) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) }
fun plantedHookFiles: Hook -> Target { H0 -> T111 + H1 -> T111 + H2 -> T113 + H3 -> T114 + H4 -> T115 + H5 -> T116 + H6 -> T117 }
fun plantedHookAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P99 + R19 -> P100 + R20 -> P104 + R21 -> P105 + R22 -> P111 + R23 -> P113 + R24 -> P117 + R25 -> P125 + R26 -> P127 + R27 -> P128 + R28 -> P134 + R29 -> P136 + R30 -> P149 + R31 -> P157 + R32 -> P166 + R33 -> P167 + R34 -> P168 + R35 -> P169 + R36 -> P170 + R37 -> P180 }
fun plantedHookRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (windows) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (mac + linux + wsl) + R27 -> (windows) + R28 -> (mac) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (windows) + R35 -> (windows) + R36 -> (windows) + R37 -> (mac) }
pred plantedHook { checks/tree[parents] and checks/unique[paths] }

// The real manifest with the first hook's file received on the Mac alone, which a validator refuses.
fun plantedMaskReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (mac + linux + wsl) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac + linux) + T33 -> (mac + linux) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux + wsl) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (linux) + T67 -> (linux + wsl) + T68 -> (linux + wsl) + T69 -> (linux) + T70 -> (linux + wsl) + T71 -> (mac + linux + wsl) + T72 -> (windows) + T73 -> (mac + linux) + T74 -> (mac + linux) + T75 -> (windows) + T76 -> (linux) + T77 -> (mac) + T78 -> (wsl) + T79 -> (mac) + T80 -> (mac) + T81 -> (mac + linux + wsl) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux) + T85 -> (linux + wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (mac + linux) + T91 -> (mac + linux) + T92 -> (mac) + T93 -> (mac) + T94 -> (windows) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) }
fun plantedMaskFiles: Hook -> Target { H0 -> T111 + H1 -> T112 + H2 -> T113 + H3 -> T114 + H4 -> T115 + H5 -> T116 + H6 -> T117 }
fun plantedMaskAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P99 + R19 -> P100 + R20 -> P104 + R21 -> P105 + R22 -> P111 + R23 -> P113 + R24 -> P117 + R25 -> P125 + R26 -> P127 + R27 -> P128 + R28 -> P134 + R29 -> P136 + R30 -> P149 + R31 -> P157 + R32 -> P166 + R33 -> P167 + R34 -> P168 + R35 -> P169 + R36 -> P170 + R37 -> P180 }
fun plantedMaskRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (windows) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (mac + linux + wsl) + R27 -> (windows) + R28 -> (mac) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (windows) + R35 -> (windows) + R36 -> (windows) + R37 -> (mac) }
pred plantedMask { checks/tree[parents] and checks/unique[paths] }

// The real manifest with a restricted directory given to every profile, which a validator refuses.
fun plantedEmptyReceives: Target -> Profile { T0 -> (mac) + T1 -> (mac + linux + wsl) + T2 -> (linux) + T3 -> (mac + linux) + T4 -> (linux) + T5 -> (mac) + T6 -> (mac) + T7 -> (mac + linux) + T8 -> (mac) + T9 -> (mac) + T10 -> (mac) + T11 -> (mac) + T12 -> (linux) + T13 -> (mac + linux + wsl) + T14 -> (Profile) + T15 -> (mac + linux + wsl) + T16 -> (mac) + T17 -> (mac + linux + wsl) + T18 -> (mac + linux + wsl) + T19 -> (mac + linux + wsl) + T20 -> (mac + linux + wsl) + T21 -> (linux) + T22 -> (mac + linux) + T23 -> (mac + linux) + T24 -> (linux) + T25 -> (linux) + T26 -> (linux) + T27 -> (mac + linux + wsl) + T28 -> (mac + linux + wsl) + T29 -> (mac) + T30 -> (mac) + T31 -> (mac + linux) + T32 -> (mac + linux) + T33 -> (mac + linux) + T34 -> (mac + linux) + T35 -> (mac) + T36 -> (mac) + T37 -> (mac + linux + wsl) + T38 -> (mac + linux + wsl) + T39 -> (linux) + T40 -> (linux) + T41 -> (linux) + T42 -> (mac + linux + wsl) + T43 -> (mac + linux + wsl) + T44 -> (mac + linux + wsl) + T45 -> (mac + linux + wsl) + T46 -> (mac + linux) + T47 -> (mac + linux) + T48 -> (mac + linux) + T49 -> (mac) + T50 -> (mac + linux + wsl) + T51 -> (mac + linux + wsl) + T52 -> (mac + linux) + T53 -> (mac + linux) + T54 -> (mac + linux + windows) + T55 -> (mac + linux + windows) + T56 -> (linux + wsl) + T57 -> (linux + wsl) + T58 -> (linux + wsl) + T59 -> (linux + wsl) + T60 -> (linux + wsl) + T61 -> (linux) + T62 -> (linux) + T63 -> (linux) + T64 -> (linux + wsl) + T65 -> (linux + wsl) + T66 -> (linux) + T67 -> (linux + wsl) + T68 -> (linux + wsl) + T69 -> (linux) + T70 -> (linux + wsl) + T71 -> (mac + linux + wsl) + T72 -> (windows) + T73 -> (mac + linux) + T74 -> (mac + linux) + T75 -> (windows) + T76 -> (linux) + T77 -> (mac) + T78 -> (wsl) + T79 -> (mac) + T80 -> (mac) + T81 -> (mac + linux + wsl) + T82 -> (linux) + T83 -> (linux) + T84 -> (linux) + T85 -> (linux + wsl) + T86 -> (linux) + T87 -> (linux) + T88 -> (linux) + T89 -> (linux) + T90 -> (mac + linux) + T91 -> (mac + linux) + T92 -> (mac) + T93 -> (mac) + T94 -> (windows) + T95 -> (windows) + T96 -> (windows) + T97 -> (windows) + T98 -> (windows) + T99 -> (windows) + T100 -> (windows) + T101 -> (mac) + T102 -> (mac) + T103 -> (mac) + T104 -> (mac) + T105 -> (mac) + T106 -> (mac) + T107 -> (mac) + T108 -> (mac) + T109 -> (mac) + T110 -> (mac) + T111 -> (mac + linux + windows + wsl) + T112 -> (mac + linux + windows + wsl) + T113 -> (mac + linux + windows + wsl) + T114 -> (mac + linux + windows + wsl) + T115 -> (mac + linux + windows + wsl) + T116 -> (mac + linux + windows + wsl) + T117 -> (mac + linux + windows + wsl) }
fun plantedEmptyFiles: Hook -> Target { H0 -> T111 + H1 -> T112 + H2 -> T113 + H3 -> T114 + H4 -> T115 + H5 -> T116 + H6 -> T117 }
fun plantedEmptyAt: Retired -> Path { R0 -> P24 + R1 -> P27 + R2 -> P44 + R3 -> P45 + R4 -> P52 + R5 -> P53 + R6 -> P54 + R7 -> P55 + R8 -> P73 + R9 -> P74 + R10 -> P75 + R11 -> P76 + R12 -> P77 + R13 -> P80 + R14 -> P81 + R15 -> P84 + R16 -> P88 + R17 -> P90 + R18 -> P99 + R19 -> P100 + R20 -> P104 + R21 -> P105 + R22 -> P111 + R23 -> P113 + R24 -> P117 + R25 -> P125 + R26 -> P127 + R27 -> P128 + R28 -> P134 + R29 -> P136 + R30 -> P149 + R31 -> P157 + R32 -> P166 + R33 -> P167 + R34 -> P168 + R35 -> P169 + R36 -> P170 + R37 -> P180 }
fun plantedEmptyRemoves: Retired -> Profile { R0 -> (mac) + R1 -> (mac) + R2 -> (mac + linux + wsl) + R3 -> (mac + linux + wsl) + R4 -> (mac + linux + wsl) + R5 -> (mac + linux + wsl) + R6 -> (mac) + R7 -> (windows) + R8 -> (linux) + R9 -> (mac) + R10 -> (mac) + R11 -> (mac) + R12 -> (mac) + R13 -> (linux) + R14 -> (mac + linux) + R15 -> (mac + linux + wsl) + R16 -> (mac + linux + wsl) + R17 -> (mac) + R18 -> (linux) + R19 -> (linux) + R20 -> (linux + wsl) + R21 -> (linux + wsl) + R22 -> (windows) + R23 -> (mac + linux + wsl) + R24 -> (windows) + R25 -> (mac) + R26 -> (mac + linux + wsl) + R27 -> (windows) + R28 -> (mac) + R29 -> (windows) + R30 -> (windows) + R31 -> (windows) + R32 -> (windows) + R33 -> (windows) + R34 -> (windows) + R35 -> (windows) + R36 -> (windows) + R37 -> (mac) }
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
