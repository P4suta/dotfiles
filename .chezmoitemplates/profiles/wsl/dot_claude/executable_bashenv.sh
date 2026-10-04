#!/usr/bin/env bash
# Claude Code 専用の bash init。
# 非対話シェルが BASH_ENV 経由で source する。
# 目的: Claude Code が Bash ツールで投げるコマンドから rg/fd/tokei/hyperfine 等
#       mise 管理のモダン CLI を *本名で* 呼べるようにする。
# 副作用: ユーザ自身の対話シェル(~/.bashrc)には影響しない。

# mise activate — 全ツールの shim を PATH に載せる
if [ -x "$HOME/.local/bin/mise" ]; then
  eval "$("$HOME/.local/bin/mise" activate bash)"
fi

# user-installed binaries を mise shim より優先するため、mise activate の **後** で先頭に追加する。
# ~/.local/bin/git wrapper (--no-verify 等を拒否) を Claude Code Bash tool から確実に効かせるためにここが必要。
# 対話シェルでは ~/.bashrc が同じ export を持つ。
export PATH="$HOME/.local/bin:$PATH"

# Claude Code 専用の shim dir(ユーザの対話シェルには載らない)は、空でも PATH に混ぜておく。
# 必要になったら ~/.claude/shims/<name> にスクリプトを置く。
if [ -d "$HOME/.claude/shims" ]; then
  export PATH="$HOME/.claude/shims:$PATH"
fi
