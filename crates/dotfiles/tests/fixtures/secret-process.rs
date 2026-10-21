use std::process::Command;

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|value| value == "--consumer") {
        std::fs::write(
            std::env::var_os("FAKE_CONSUMER_LOG").unwrap(),
            b"consumer started",
        )
        .unwrap();
        assert_eq!(std::env::var("API_KEY").unwrap(), "fixture-only");
        assert!(std::env::var_os("DOPPLER_TOKEN").is_none());
        std::process::exit(37);
    }
    let [scope_flag, scope_directory, arguments @ ..] = arguments.as_slice() else {
        panic!("missing authentication scope")
    };
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap();
    assert_eq!(scope_flag, "--scope");
    assert_eq!(scope_directory, &home);
    assert!(std::path::Path::new(&home).is_absolute());
    assert!(!std::env::current_dir().unwrap().starts_with(&home));
    let prefix = [
        "run",
        "--project",
        "fixture",
        "--config",
        "fixture",
        "--only-secrets",
        "API_KEY",
        "--no-fallback",
        "--",
    ]
    .map(std::ffi::OsString::from);
    let command = arguments
        .strip_prefix(prefix.as_slice())
        .expect("unexpected secret selection");
    let (program, forwarded) = command.split_first().expect("missing consumer command");
    std::fs::write(
        std::env::var_os("FAKE_DOPPLER_LOG").unwrap(),
        b"scope and no-fallback verified",
    )
    .unwrap();
    let value = std::env::var_os("FAKE_SECRET").unwrap();
    let status = Command::new(program)
        .args(forwarded)
        .env("API_KEY", value)
        .env("DOPPLER_TOKEN", "fixture-authentication")
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}
