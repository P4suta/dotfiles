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
                "dotfiles-refusal/2 {{\"rule\":\"commit.language\",\"cause\":\"fixture\",\"evidence\":[\"fixture\"],\"next\":\"git status\",\"bypass\":null}}"
            );
            std::process::exit(1);
        }
        // Fails without a record.
        "post-commit" => {
            if let Ok(log) = std::env::var("FIXTURE_HOOK_LOG") {
                let mut events = std::fs::read_to_string(&log).unwrap_or_default();
                events.push_str("dotguard\n");
                std::fs::write(log, events).unwrap();
                return;
            }
            eprintln!("fixture failure");
            std::process::exit(1);
        }
        _ => {}
    }
}
