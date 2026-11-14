use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;

/// Upper bound on how much of a plugin's stdout/stderr we keep in memory.
/// yt-dlp happily dumps megabytes of JSON and warnings; a rogue or chatty
/// plugin must never be able to balloon the player's RSS.
const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;

/// Handle that can stop an in-flight plugin from another thread. Skipping
/// tracks quickly used to leave one python+yt-dlp tree per press running at
/// the same time; cancelling the previous resolve caps that at one.
#[derive(Clone, Default)]
pub struct CancelHandle {
    cancelled: Arc<AtomicBool>,
    /// Process-group id of the running plugin, or 0 when nothing is running.
    pgid: Arc<AtomicI32>,
}

impl CancelHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop the plugin and everything it spawned. Safe to call when nothing
    /// is running.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        #[cfg(unix)]
        {
            let pgid = self.pgid.load(Ordering::SeqCst);
            if pgid > 0 {
                // Safety: plain syscall. The plugin runs in its own process
                // group (see `run_plugin`), so this never hits the player.
                unsafe {
                    libc::kill(-pgid, libc::SIGKILL);
                }
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn attach(&self, child: &std::process::Child) {
        self.pgid.store(child.id() as i32, Ordering::SeqCst);
    }

    fn detach(&self) {
        self.pgid.store(0, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    Metadata,
    Streaming,
    Lyrics,
}

impl PluginKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Streaming => "streaming",
            Self::Lyrics => "lyrics",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Metadata => "META",
            Self::Streaming => "AUDIO",
            Self::Lyrics => "LYRIC",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    pub description: String,
    pub entry: String,
    pub runtime: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
}

impl Default for PluginManifest {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Unnamed plugin".to_string(),
            version: "0.0.0".to_string(),
            kind: PluginKind::Metadata,
            description: String::new(),
            entry: "main.py".to_string(),
            runtime: None,
            author: None,
            homepage: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BundledPlugin {
    pub manifest: PluginManifest,
    pub script: &'static str,
}

fn manifest(
    id: &str,
    name: &str,
    kind: PluginKind,
    description: &str,
    runtime: Option<&str>,
) -> PluginManifest {
    PluginManifest {
        id: id.to_string(),
        name: name.to_string(),
        version: "1.2.0".to_string(),
        kind,
        description: description.to_string(),
        entry: "main.py".to_string(),
        runtime: runtime.map(ToString::to_string),
        author: Some("mfe-player".to_string()),
        homepage: None,
    }
}

pub fn catalog() -> Vec<BundledPlugin> {
    vec![
        BundledPlugin {
            manifest: manifest(
                "itunes",
                "iTunes Search",
                PluginKind::Metadata,
                "Apple's keyless Search API: tracks, albums, durations and high-res cover art.",
                None,
            ),
            script: include_str!("../plugins/metadata_itunes.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "musicbrainz",
                "MusicBrainz",
                PluginKind::Metadata,
                "Open MusicBrainz recordings with Cover Art Archive art links.",
                None,
            ),
            script: include_str!("../plugins/metadata_musicbrainz.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "youtube",
                "YouTube (yt-dlp)",
                PluginKind::Streaming,
                "Finds the closest YouTube match and extracts a direct audio stream.",
                None,
            ),
            script: include_str!("../plugins/stream_youtube.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "soundcloud",
                "SoundCloud (yt-dlp)",
                PluginKind::Streaming,
                "Searches SoundCloud and extracts its direct HLS audio stream.",
                None,
            ),
            script: include_str!("../plugins/stream_soundcloud.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "lrclib",
                "LRCLIB Lyrics",
                PluginKind::Lyrics,
                "Synced and plain lyrics from LRCLIB, with lyrics.ovh as a fallback.",
                None,
            ),
            script: include_str!("../plugins/lyrics_lrclib.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "internetarchive",
                "Internet Archive Metadata",
                PluginKind::Metadata,
                "Searches the Internet Archive for free music, live sets and public-domain recordings.",
                None,
            ),
            script: include_str!("../plugins/metadata_internetarchive.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "radiobrowser",
                "Radio Browser Metadata",
                PluginKind::Metadata,
                "Searches radio-browser.info for live stations worldwide.",
                None,
            ),
            script: include_str!("../plugins/metadata_radiobrowser.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "stream_internetarchive",
                "Internet Archive Stream",
                PluginKind::Streaming,
                "Resolves Internet Archive tracks into direct download URLs.",
                None,
            ),
            script: include_str!("../plugins/stream_internetarchive.py"),
        },
        BundledPlugin {
            manifest: manifest(
                "stream_radiobrowser",
                "Radio Browser Stream",
                PluginKind::Streaming,
                "Plays live radio stations found via Radio Browser.",
                None,
            ),
            script: include_str!("../plugins/stream_radiobrowser.py"),
        },
    ]
}

pub struct PluginRegistry {
    pub installed: Vec<InstalledPlugin>,
    pub catalog: Vec<BundledPlugin>,
    plugins_dir: PathBuf,
}

impl PluginRegistry {
    pub fn load() -> Self {
        let catalog = catalog();
        let plugins_dir = config::plugins_dir();
        let mut registry = Self {
            installed: Vec::new(),
            catalog,
            plugins_dir,
        };
        registry.refresh();
        registry
    }

    pub fn refresh(&mut self) {
        let mut installed = Vec::new();
        let Ok(entries) = fs::read_dir(&self.plugins_dir) else {
            self.installed = installed;
            return;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let manifest_path = dir.join("plugin.json");
            let Ok(raw) = fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<PluginManifest>(&raw) else {
                continue;
            };
            if !valid_id(&manifest.id) || manifest.entry.trim().is_empty() {
                continue;
            }
            if !dir.join(&manifest.entry).exists() {
                continue;
            }
            installed.push(InstalledPlugin { manifest, dir });
        }
        installed.sort_by(|a, b| {
            plugin_kind_order(a.manifest.kind)
                .cmp(&plugin_kind_order(b.manifest.kind))
                .then_with(|| {
                    a.manifest
                        .name
                        .to_lowercase()
                        .cmp(&b.manifest.name.to_lowercase())
                })
        });
        self.installed = installed;
    }

    /// Make sure one provider of every kind exists, and upgrade any installed
    /// plugin whose bundled copy moved to a newer version (protocol changes,
    /// playback fixes, ...). Returns the ids that were (re)installed.
    pub fn ensure_defaults(&mut self) -> Vec<String> {
        let mut installed = Vec::new();
        for bundled in self.catalog.clone() {
            let id = bundled.manifest.id.clone();
            let needs_install = match self.by_id(&id) {
                Some(existing) => existing.manifest.version != bundled.manifest.version,
                None => true,
            };
            if needs_install && self.install(&id).is_ok() {
                installed.push(id);
            }
        }
        installed
    }

    pub fn install(&mut self, id: &str) -> Result<()> {
        if !valid_id(id) {
            return Err(anyhow!("invalid plugin id"));
        }
        let bundled = self
            .catalog
            .iter()
            .find(|plugin| plugin.manifest.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("plugin '{id}' is not in the store"))?;

        let dir = self.plugins_dir.join(&bundled.manifest.id);
        fs::create_dir_all(&dir)
            .with_context(|| format!("creating plugin directory {}", dir.display()))?;
        let entry_path = dir.join(&bundled.manifest.entry);
        fs::write(&entry_path, bundled.script)
            .with_context(|| format!("writing {}", entry_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&entry_path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&entry_path, permissions)?;
        }

        let manifest_path = dir.join("plugin.json");
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&bundled.manifest)?,
        )
        .with_context(|| format!("writing {}", manifest_path.display()))?;

        self.refresh();
        Ok(())
    }

    pub fn uninstall(&mut self, id: &str) -> Result<()> {
        if !valid_id(id) {
            return Err(anyhow!("invalid plugin id"));
        }
        let dir = self.plugins_dir.join(id);
        if !dir.exists() {
            return Ok(());
        }
        let root = self
            .plugins_dir
            .canonicalize()
            .unwrap_or_else(|_| self.plugins_dir.clone());
        let target = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if !target.starts_with(&root) {
            return Err(anyhow!("refusing to remove path outside plugin directory"));
        }
        fs::remove_dir_all(&target).with_context(|| format!("removing {}", target.display()))?;
        self.refresh();
        Ok(())
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.installed.iter().any(|p| p.manifest.id == id)
    }

    pub fn by_id(&self, id: &str) -> Option<InstalledPlugin> {
        self.installed.iter().find(|p| p.manifest.id == id).cloned()
    }

    pub fn first_of_kind(&self, kind: PluginKind) -> Option<InstalledPlugin> {
        self.installed
            .iter()
            .find(|p| p.manifest.kind == kind)
            .cloned()
    }

    pub fn active(&self, kind: PluginKind, configured: Option<&str>) -> Option<InstalledPlugin> {
        if let Some(id) = configured {
            if let Some(plugin) = self.by_id(id) {
                if plugin.manifest.kind == kind {
                    return Some(plugin);
                }
            }
        }
        self.first_of_kind(kind)
    }

    pub fn bundled(&self, id: &str) -> Option<&BundledPlugin> {
        self.catalog.iter().find(|p| p.manifest.id == id)
    }
}

fn plugin_kind_order(kind: PluginKind) -> usize {
    match kind {
        PluginKind::Metadata => 0,
        PluginKind::Streaming => 1,
        PluginKind::Lyrics => 2,
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn run_plugin(plugin: &InstalledPlugin, args: &[String], timeout: Duration) -> Result<String> {
    run_plugin_cancellable(plugin, args, timeout, None)
}

/// Like [`run_plugin`], but the caller can abort the run from another thread
/// via `cancel`. Used for stream resolves so skipping tracks cannot stack up
/// several python + yt-dlp trees at once.
pub fn run_plugin_cancellable(
    plugin: &InstalledPlugin,
    args: &[String],
    timeout: Duration,
    cancel: Option<&CancelHandle>,
) -> Result<String> {
    let entry = plugin.dir.join(&plugin.manifest.entry);
    if !entry.exists() {
        return Err(anyhow!("plugin entry is missing: {}", entry.display()));
    }

    let mut command = if let Some(runtime) = plugin.manifest.runtime.as_deref() {
        let mut command = Command::new(runtime);
        command.arg(&entry);
        command
    } else {
        #[cfg(windows)]
        {
            // Windows has no shebangs: run the script through Python.
            let mut command = Command::new("python");
            command.arg(&entry);
            command
        }
        #[cfg(not(windows))]
        {
            Command::new(&entry)
        }
    };
    command
        .args(args)
        .current_dir(&plugin.dir)
        .env("MFE_PLAYER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Give the plugin its own process group so a timeout can take down the
    // whole tree (python -> yt-dlp -> ffmpeg) instead of leaving orphans that
    // keep downloading in the background.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    // A crashed player must not leave the plugin tree behind either.
    crate::ipc::die_with_parent(&mut command);

    run_with_timeout(command, timeout, cancel)
        .with_context(|| format!("plugin '{}'", plugin.manifest.name))
}

fn run_with_timeout(
    mut command: Command,
    timeout: Duration,
    cancel: Option<&CancelHandle>,
) -> Result<String> {
    let mut child = command
        .spawn()
        .with_context(|| format!("could not start {:?}", command.get_program()))?;
    if let Some(cancel) = cancel {
        cancel.attach(&child);
    }

    let stdout = child.stdout.take().context("plugin stdout unavailable")?;
    let stderr = child.stderr.take().context("plugin stderr unavailable")?;
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let stderr_reader = thread::spawn(move || read_all(stderr));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Reap first, then forget the pid: a later cancel() must not
                // signal a process group whose id was reused.
                if let Some(cancel) = cancel {
                    cancel.detach();
                }
                break status;
            }
            Ok(None) if Instant::now() < deadline => {
                if cancel.is_some_and(CancelHandle::is_cancelled) {
                    kill_tree(&mut child);
                    if let Some(cancel) = cancel {
                        cancel.detach();
                    }
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(anyhow!("cancelled"));
                }
                thread::sleep(Duration::from_millis(30));
            }
            Ok(None) => {
                kill_tree(&mut child);
                if let Some(cancel) = cancel {
                    cancel.detach();
                }
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(anyhow!("timed out after {}s", timeout.as_secs()));
            }
            Err(err) => {
                kill_tree(&mut child);
                if let Some(cancel) = cancel {
                    cancel.detach();
                }
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(err.into());
            }
        }
    };
    if let Some(cancel) = cancel {
        cancel.detach();
    }

    let out = stdout_reader.join().unwrap_or_default();
    let err = stderr_reader.join().unwrap_or_default();
    let stdout = String::from_utf8_lossy(&out).trim().to_string();
    let stderr = String::from_utf8_lossy(&err).trim().to_string();

    if status.success() {
        if stdout.is_empty() {
            return Err(anyhow!("plugin returned no output"));
        }
        Ok(stdout)
    } else {
        let detail = if stderr.is_empty() {
            format!("plugin exited with {status}")
        } else {
            truncate(&stderr, 500)
        };
        Err(anyhow!(detail))
    }
}

/// Kill a plugin and everything it spawned. The child runs in its own process
/// group (see `run_plugin`), so the negative pid reaches grandchildren such as
/// `yt-dlp` and `ffmpeg` instead of orphaning them.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // Safety: plain syscall, no memory unsafety. The child is its own
        // process-group leader, so -(pid) never matches our own group.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_all(mut reader: impl Read) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(8 * 1024);
    // Read in chunks and stop *storing* at the cap while still draining the
    // pipe: a full pipe would otherwise block the plugin until the timeout.
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                let room = MAX_CAPTURE_BYTES.saturating_sub(buffer.len());
                if room > 0 {
                    buffer.extend_from_slice(&chunk[..count.min(room)]);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    buffer
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}
