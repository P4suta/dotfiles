use std::io::Write as _;

fn main() -> std::io::Result<()> {
    let marker = std::env::var_os("DOTFILES_NOTIFY_FIXTURE_MARKER")
        .ok_or_else(|| std::io::Error::other("missing fixture marker"))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(marker)?;
    let payload = std::env::args().next_back().unwrap_or_default();
    writeln!(file, "{payload}")
}
