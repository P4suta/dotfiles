//! The language gate run as installed: a refused scan ends with one complete record.

use dotguard::refusal::Refusal;
use std::process::Command;

#[test]
fn a_contaminated_file_is_refused_with_a_rescan_and_a_recorded_waiver() {
    let scope = std::env::temp_dir().join(format!("dotguard-language-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scope);
    std::fs::create_dir_all(&scope).unwrap();
    let cyrillic = char::from_u32(0x0441).unwrap();
    std::fs::write(scope.join("a.txt"), format!("plain\nmixed {cyrillic}\n")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dotguard"))
        .current_dir(&scope)
        .env("HOME", &scope)
        .env("USERPROFILE", &scope)
        .env_remove("ALLOW_FOREIGN")
        .args(["scan", "a.txt"])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&scope);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let refusal = Refusal::parse(stderr.lines().last().unwrap()).expect("a structured refusal");
    assert!(refusal.is_complete(), "{refusal:?}");
    assert_eq!(refusal.rule, "scan.language");
    assert_eq!(refusal.next, "dotguard scan a.txt");
    assert_eq!(
        refusal.waiver.as_deref(),
        Some("ALLOW_FOREIGN=1 dotguard scan a.txt")
    );
    assert_eq!(
        refusal.evidence,
        ["a.txt:2:7  '\u{441}'  U+0441  [Cyrillic]"]
    );
}
