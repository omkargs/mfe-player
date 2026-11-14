//! One tiny transport abstraction so the rest of the player does not care
//! whether mpv is listening on a Unix socket or a Windows named pipe.
//!
//! mpv speaks the same JSON IPC over both; only the thing you open differs.

use std::io;
use std::path::Path;

#[cfg(unix)]
pub type Connection = std::os::unix::net::UnixStream;

#[cfg(windows)]
pub type Connection = std::fs::File;

/// Open a connection to mpv's IPC endpoint. On Windows the "path" is a named
/// pipe such as `\\.\pipe\mfe-player`, which `std::fs` can open directly.
pub fn connect(path: &Path) -> io::Result<Connection> {
    #[cfg(unix)]
    {
        Connection::connect(path)
    }
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
    }
}

/// Apply read/write timeouts where the platform supports them. Windows named
/// pipes opened through `std::fs` do not expose timeouts; the reader falls back
/// to blocking, which is fine because mpv always answers.
#[cfg(unix)]
pub fn set_timeouts(connection: &Connection, timeout: std::time::Duration) {
    let _ = connection.set_read_timeout(Some(timeout));
    let _ = connection.set_write_timeout(Some(timeout));
}

#[cfg(windows)]
pub fn set_timeouts(_connection: &Connection, _timeout: std::time::Duration) {}

#[cfg(unix)]
pub fn clear_timeouts(connection: &Connection) {
    let _ = connection.set_read_timeout(None);
    let _ = connection.set_write_timeout(None);
}

#[cfg(windows)]
pub fn clear_timeouts(_connection: &Connection) {}

/// Spawn helper flags: detach from the terminal on Unix, use a detached
/// process group on Windows.
#[cfg(unix)]
pub fn detach(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // Put the child in its own session so closing the terminal does not
    // SIGHUP the audio process.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(windows)]
pub fn detach(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Spawn a child that must keep working standard handles. `DETACHED_PROCESS`
/// can break stdio inheritance, so children like mpv get a hidden console and
/// their own process group instead. On Unix this is the same as `detach`.
pub fn hide_window(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        detach(command);
    }
}

/// Ask the kernel to kill this child if the player process dies. Used for
/// helpers whose work only makes sense while the UI is alive (cava, plugin
/// runs, downloads) — a crashed or SIGKILLed TUI must not leave cava, yt-dlp
/// and ffmpeg running in the background until the next reboot.
///
/// Not applied to mpv: playback intentionally outlives the UI.
#[cfg(target_os = "linux")]
pub fn die_with_parent(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            // Safety: async-signal-safe syscall in the child between fork and
            // exec; the error path only builds an io::Error.
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
pub fn die_with_parent(_command: &mut std::process::Command) {}

/// Is `pid` alive *and* an mfe-player process?
///
/// Pid files (`session.pid`) outlive crashes, and the OS recycles pids, so
/// "pid is alive" is not the same as "a player owns the session". Trusting a
/// recycled pid used to convince every new launch that another session owned
/// mpv: the TUI spawned a private player, the daemon waited ten seconds for a
/// release that never came, and playback ended up in an unrunnable orphan.
pub fn is_player_pid(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() || !pid_alive(pid) {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        // comm is the executable name (truncated to 15 chars), which is
        // "mfe-player" for both the TUI and the daemon.
        return std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|comm| comm.trim().contains("mfe"))
            .unwrap_or(false);
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output();
        return output
            .map(|out| String::from_utf8_lossy(&out.stdout).contains("mfe"))
            .unwrap_or(false);
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        return output
            .map(|out| {
                let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
                text.contains("mfe-player") && !text.contains("no tasks")
            })
            .unwrap_or(false);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        true
    }
}

/// Hand freed heap pages back to the OS after a big transient allocation
/// (cover decode, library scan, a large search payload).
///
/// glibc keeps freed memory in its arenas by default, so a player that decodes
/// a 20 MB cover once can sit at that RSS for the rest of the session. This is
/// a no-op on platforms that already return pages eagerly.
pub fn release_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // Safety: malloc_trim only walks the allocator's own bookkeeping and
        // is safe to call from the main thread at any time.
        unsafe {
            libc::malloc_trim(0);
        }
    }
}

/// Is this process still alive? Unix gets the real answer from `kill(pid, 0)`;
/// Windows asks `tasklist`. The daemon only calls this at startup, so a process
/// spawn there is acceptable.
#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output();
    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()),
        Err(_) => false,
    }
}

/// Ask a process to stop. Unix sends SIGTERM; Windows uses `taskkill`
/// without `/F` first so the daemon can flush its state.
pub fn terminate(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .output();
    }
}

/// Last-resort stop.
pub fn kill(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
}
