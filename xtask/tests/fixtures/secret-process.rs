use std::process::Command;

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|value| value == "--consumer") {
        std::fs::write(std::env::var_os("FAKE_CONSUMER_LOG").unwrap(), b"consumer started").unwrap();
        assert_eq!(std::env::var("API_KEY").unwrap(), "fixture-only");
        assert!(std::env::var_os("DOPPLER_TOKEN").is_none());
        std::process::exit(37);
    }
    assert_eq!(arguments[..9], ["run", "--project", "fixture", "--config", "fixture", "--only-secrets", "API_KEY", "--no-fallback", "--"].map(std::ffi::OsString::from));
    std::fs::write(std::env::var_os("FAKE_DOPPLER_LOG").unwrap(), b"scope and no-fallback verified").unwrap();
    let value = std::env::var_os("FAKE_SECRET").unwrap();
    let status = Command::new(&arguments[9]).args(&arguments[10..]).env("API_KEY", value).env("DOPPLER_TOKEN", "fixture-authentication").status().unwrap();
    std::process::exit(status.code().unwrap_or(1));
}
