# The Tokyo Night theme

The palette in `home/.chezmoidata/shared.toml` is the one source every themed target renders from.
A rendered color literal outside it fails the profile gate, and every pair a theme draws text with keeps WCAG contrast of 4.5:1.

## The shell startup cache

The integrations setup step caches each tool's initialization under `~/.cache/zsh`, as it does for PowerShell, so starting a shell runs no other program.

## パレットの意味

パレットは Tokyo Night の色を名前で持ち、すべてのテーマはその名前だけを参照する。
背景・前景・コメント・差分の色は、それぞれ `bg`・`fg`・`comment`・`diff_add` が担う。
端末の ANSI スロットは `bright_black` から `fg` までの十六色で埋める。

## The wrapper's argument policy

The wrapper inspects the argument vector before it delegates to the real git, and refuses a force push that rewrites published history.
A denial names the category, the reason, and the command that would pass the gate.

## ギリシャ文字の走査

The language gate reports a run of Greek letters, such as αβγδε, because no word of the managed languages carries them.
The scan counts the run before it reports the first offending character.

## The rehearsal

The rehearsal applies the profile to an owned home, counts the programs each interactive shell starts, and holds them to the baselines.
No gate reads wall time, so a slow host and a fast one agree on every count.

## 判定の記録

判定は `policy/baselines.toml` に厳密な値として記録され、改善も悪化も同じく失敗する。
新しい値を記録するまで、ゲートは通らない。
