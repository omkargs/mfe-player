//! `mfe update`: check GitHub releases and, on Unix, replace the running
//! binary with the newest prebuilt one.
//!
//! It deliberately shells out to `curl` and `tar` instead of adding an HTTP
//! stack: the installer already depends on them, and the player stays small.
//! If any step cannot work out the install method or download safely, it
//! prints the exact one-liner instead of guessing.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};

const REPO: &str = "omkargs/mfe-player";

pub fn run(check_only: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("mfe-player {current}");

    let latest = match latest_version() {
        Ok(latest) => latest,
        Err(err) => {
            println!("could not check for updates: {err}");
            return Ok(());
        }
    };
    if !is_newer(&latest, current) {
        println!("already on the latest version.");
        return Ok(());
    }
    println!("a newer release is available: {latest}");

    if check_only {
        println!("run `mfe update` to install it.");
        return Ok(());
    }

    #[cfg(windows)]
    {
        println!("self-update is not wired up on Windows yet.");
        println!("download it from https://github.com/{REPO}/releases/latest");
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let triple = target_triple().ok_or_else(|| {
            anyhow!("no prebuilt release for this platform — use the install script")
        })?;
        let url = format!(
            "https://github.com/{REPO}/releases/latest/download/mfe-player-{triple}.tar.gz"
        );
        let tmp = env::temp_dir().join(format!("mfe-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).with_context(|| format!("creating {}", tmp.display()))?;

        println!("downloading {triple}");
        if let Err(err) = download(&url, &tmp.join("mfe-player.tar.gz")) {
            let _ = fs::remove_dir_all(&tmp);
            println!("could not download the release: {err}");
            print_fallback();
            return Ok(());
        }

        let archive = tmp.join("mfe-player.tar.gz");
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&tmp)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !matches!(status, Ok(status) if status.success()) {
            let _ = fs::remove_dir_all(&tmp);
            println!("could not unpack the release.");
            print_fallback();
            return Ok(());
        }

        let Some(binary) = find_binary(&tmp) else {
            let _ = fs::remove_dir_all(&tmp);
            println!("the release did not contain an mfe-player binary.");
            print_fallback();
            return Ok(());
        };

        let current_exe = env::current_exe().context("locating the running binary")?;
        let staged = current_exe.with_extension("new");
        let install = (|| -> std::io::Result<()> {
            fs::copy(&binary, &staged)?;
            set_executable(&staged)?;
            fs::rename(&staged, &current_exe)?;
            Ok(())
        })()
        .with_context(|| format!("replacing {}", current_exe.display()));
        let _ = fs::remove_dir_all(&tmp);

        match install {
            Ok(()) => {
                println!("updated to {latest} at {}", current_exe.display());
                println!("restart mfe to use it.");
            }
            Err(err) => {
                let _ = fs::remove_file(&staged);
                println!("could not replace {}: {err}", current_exe.display());
                print_fallback();
            }
        }
    }
    Ok(())
}

fn latest_version() -> Result<String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "20",
            "-H",
            "User-Agent: mfe-player",
            &format!("https://api.github.com/repos/{REPO}/releases/latest"),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .context("running curl")?;
    if !output.status.success() {
        return Err(anyhow!("GitHub did not answer (are you offline?)"));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing the GitHub response")?;
    let tag = value
        .get("tag_name")
        .and_then(|tag| tag.as_str())
        .ok_or_else(|| anyhow!("the release has no tag"))?;
    Ok(tag.trim_start_matches('v').to_string())
}

fn download(url: &str, dest: &Path) -> Result<()> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            "120",
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("running curl")?;
    if !output.status.success() {
        return Err(anyhow!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn find_binary(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_binary(&path) {
                return Some(found);
            }
        } else if path.file_name().and_then(|name| name.to_str()) == Some("mfe-player") {
            return Some(path);
        }
    }
    None
}

#[cfg(unix)]
fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
}

#[cfg(windows)]
fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn target_triple() -> Option<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        _ => None,
    }
}

fn print_fallback() {
    println!("install the new version with:");
    println!("  curl -fsSL https://raw.githubusercontent.com/{REPO}/master/install.sh | bash");
}

/// Numeric semver comparison: 0.10.0 is newer than 0.9.9. Pre-release
/// suffixes are ignored; releases here do not use them.
fn is_newer(latest: &str, current: &str) -> bool {
    parse(latest) > parse(current)
}

fn parse(version: &str) -> (u64, u64, u64) {
    let mut parts = version.trim_start_matches('v').split('.');
    let mut next = || {
        parts
            .next()
            .and_then(|part| {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
            .unwrap_or(0)
    };
    (next(), next(), next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(is_newer("v1.0.0", "0.99.99"));
    }

    #[test]
    fn tolerates_missing_parts_and_suffixes() {
        assert_eq!(parse("1.2"), (1, 2, 0));
        assert_eq!(parse("v0.3.1-beta"), (0, 3, 1));
    }
}
