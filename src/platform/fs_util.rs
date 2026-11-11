//! Small filesystem helpers that behave the same on every platform.

use std::io;
use std::path::Path;

/// Replace `to` with `from`.
///
/// Unix `rename` atomically replaces the destination. Windows fails when the
/// destination exists, so fall back to removing it first. State and taste
/// writes go through here so they work on all three platforms.
pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        match std::fs::rename(from, to) {
            Ok(()) => Ok(()),
            Err(_) if to.exists() => {
                let _ = std::fs::remove_file(to);
                std::fs::rename(from, to)
            }
            Err(err) => Err(err),
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to)
    }
}
