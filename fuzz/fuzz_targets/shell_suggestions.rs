#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (source, tree) = text.split_once("\n---\n").unwrap_or(("jq a", text));
    let _ = dotfiles::legacy::check_shell_spelling_tree(source, tree);
});
