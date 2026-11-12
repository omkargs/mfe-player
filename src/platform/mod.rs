//! Platform-specific implementations behind small, stable interfaces.
//!
//! The player supports Linux, macOS and Windows from one codebase. Anything
//! that talks to the operating system lives here instead of being scattered
//! behind ad-hoc `cfg` blocks in the UI.

pub mod fs_util;
pub mod media;

#[cfg(target_os = "macos")]
pub mod macos_nowplaying;

#[cfg(windows)]
pub mod windows_smtc;
