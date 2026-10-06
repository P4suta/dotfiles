$env.config = {
  show_banner: false
  edit_mode: vi
  buffer_editor: $env.EDITOR
  use_ansi_coloring: true
  bracketed_paste: true
  footer_mode: 25
  float_precision: 3

  table: {
    mode: rounded
    index_mode: always
    show_empty: true
    trim: { methodology: wrapping, wrapping_try_keep_words: true }
  }

  ls: {
    use_ls_colors: true
    clickable_links: true
  }

  history: {
    max_size: 10_000_000
    sync_on_enter: true
    file_format: sqlite
    isolation: false
  }

  cursor_shape: {
    emacs: line
    vi_insert: line
    vi_normal: block
  }

  color_config: {
    separator: "#565f89"
    leading_trailing_space_bg: { attr: n }
    header: { fg: "#9ece6a", attr: b }
    empty: "#7aa2f7"
    bool: "#bb9af7"
    int: "#ff9e64"
    filesize: "#2ac3de"
    duration: "#e0af68"
    date: "#73daca"
    range: "#7dcfff"
    float: "#ff9e64"
    string: "#9ece6a"
    nothing: "#565f89"
    binary: "#c0caf5"
    cell-path: "#7dcfff"
    row_index: { fg: "#565f89", attr: b }
    record: "#c0caf5"
    list: "#c0caf5"
    block: "#c0caf5"
    hints: "#565f89"
    search_result: { fg: "#1a1b26", bg: "#e0af68" }
    shape_garbage: { fg: "#f7768e", attr: b }
    shape_bool: "#bb9af7"
    shape_int: "#ff9e64"
    shape_float: "#ff9e64"
    shape_range: "#7dcfff"
    shape_internalcall: "#7aa2f7"
    shape_external: "#9ece6a"
    shape_externalarg: "#73daca"
    shape_literal: "#2ac3de"
    shape_operator: "#bb9af7"
    shape_signature: "#9ece6a"
    shape_string: "#9ece6a"
    shape_filepath: "#7dcfff"
    shape_directory: "#7dcfff"
    shape_globpattern: "#2ac3de"
    shape_variable: "#c0caf5"
    shape_flag: "#e0af68"
    shape_custom: "#7aa2f7"
  }

  shell_integration: {
    osc2: true
    osc7: true
    osc8: true
    osc9_9: false
    osc133: true
    osc633: true
    reset_application_mode: true
  }

  keybindings: [
    { name: word_left, modifier: alt, keycode: left, mode: [emacs, vi_insert], event: { edit: movewordleft } }
    { name: word_right, modifier: alt, keycode: right, mode: [emacs, vi_insert], event: { until: [{ send: historyhintwordcomplete } { edit: movewordright }] } }
    { name: word_left_ctrl, modifier: control, keycode: left, mode: [emacs, vi_insert], event: { edit: movewordleft } }
    { name: word_right_ctrl, modifier: control, keycode: right, mode: [emacs, vi_insert], event: { until: [{ send: historyhintwordcomplete } { edit: movewordright }] } }
    { name: word_left_b, modifier: alt, keycode: char_b, mode: [emacs, vi_insert], event: { edit: movewordleft } }
    { name: word_right_f, modifier: alt, keycode: char_f, mode: [emacs, vi_insert], event: { until: [{ send: historyhintwordcomplete } { edit: movewordright }] } }
    { name: kill_word, modifier: alt, keycode: backspace, mode: [emacs, vi_insert], event: { edit: backspaceword } }
    { name: kill_word_ctrl, modifier: control, keycode: char_w, mode: [emacs, vi_insert], event: { edit: backspaceword } }
    { name: kill_line_start, modifier: control, keycode: char_u, mode: [emacs, vi_insert], event: { edit: cutfromlinestart } }
    { name: kill_line_end, modifier: control, keycode: char_k, mode: [emacs, vi_insert], event: { edit: cuttolineend } }
    { name: line_start, modifier: control, keycode: char_a, mode: [emacs, vi_insert], event: { edit: movetolinestart } }
    { name: line_end, modifier: control, keycode: char_e, mode: [emacs, vi_insert], event: { until: [{ send: historyhintcomplete } { edit: movetolineend }] } }
  ]

  completions: {
    case_sensitive: false
    quick: true
    partial: true
    algorithm: fuzzy
    use_ls_colors: true
    external: {
      enable: true
      max_results: 100
      completer: {|spans: list<string>|
        let expansion = (scope aliases | where name == $spans.0 | get --optional 0.expansion)
        let spans = if $expansion == null { $spans } else { $spans | skip 1 | prepend ($expansion | split row " " | first) }
        ^carapace $spans.0 nushell ...$spans | from json
      }
    }
  }
}

$env.TRANSIENT_PROMPT_COMMAND = ""
$env.TRANSIENT_PROMPT_COMMAND_RIGHT = ""
$env.TRANSIENT_PROMPT_MULTILINE_INDICATOR = ""
$env.TRANSIENT_PROMPT_INDICATOR = $"(ansi {fg: "#9ece6a" attr: b})❯(ansi reset) "
$env.TRANSIENT_PROMPT_INDICATOR_VI_INSERT = $env.TRANSIENT_PROMPT_INDICATOR
$env.TRANSIENT_PROMPT_INDICATOR_VI_NORMAL = $"(ansi {fg: "#bb9af7" attr: b})❮(ansi reset) "

use ~/.config/nushell/generated/mise.nu

dotfiles-prepend-path

source ~/.config/nushell/generated/starship.nu
$env.PROMPT_COMMAND_RIGHT = ""
$env.PROMPT_INDICATOR_VI_INSERT = ""
$env.PROMPT_INDICATOR_VI_NORMAL = ""
source ~/.config/nushell/generated/television.nu
source ~/.config/nushell/generated/atuin.nu
source ~/.config/nushell/generated/zoxide.nu
source ~/.config/nushell/generated/ls-colors.nu

alias ll = eza --long --all --git --icons=auto
alias la = eza --all --icons=auto
alias lt = eza --tree --level=2 --icons=auto
alias cat = bat
alias lg = lazygit
alias top = btm

def --env y [...args] {
  let cwd_file = (mktemp --tmpdir yazi-cwd.XXXXXX)
  ^yazi ...$args --cwd-file $cwd_file
  let cwd = (open --raw $cwd_file | str trim)
  if ($cwd | is-not-empty) and $cwd != $env.PWD { cd $cwd }
  rm --force --permanent $cwd_file
}

def --env mkcd [directory: path] {
  mkdir $directory
  cd $directory
}
