//! A stand-in for dotguard that behaves as one hook gate per hook name.

fn main() {
    match std::env::args().nth(1).unwrap_or_default().as_str() {
        // Stages a change, which the dispatcher must refuse.
        "pre-commit" => {
            std::fs::write("added.txt", "added\n").unwrap();
            let staged = std::process::Command::new("git")
                .args(["add", "added.txt"])
                .status()
                .unwrap();
            assert!(staged.success());
        }
        // Refuses with its own record, which must stay last.
        "commit-msg" => {
            eprintln!(
                "dotfiles-refusal/1 {{\"rule\":\"commit.language\",\"cause\":\"fixture\",\"evidence\":[\"fixture\"],\"next\":\"git status\",\"waiver\":null}}"
            );
            std::process::exit(1);
        }
        // Fails without a record.
        "post-commit" => {
            eprintln!("fixture failure");
            std::process::exit(1);
        }
        _ => {}
    }
}
