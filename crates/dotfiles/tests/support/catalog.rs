//! Installs the verification catalogs into a fixture home the way chezmoi installs them on a host.
//! The dotfiles and pr-workflow test suites share it.

use std::fs;
use std::path::Path;

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Copies `home/dot_config/assure` of this checkout to `.config/assure` under `home`.
pub fn install(home: &Path) -> std::io::Result<()> {
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../home/dot_config/assure"),
        &home.join(".config/assure"),
    )
}
