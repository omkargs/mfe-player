use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config;

/// Drives a `cava` subprocess and exposes its raw bar values to the renderer.
/// Cava is optional: when it is missing or cannot reach an audio sink the
/// visualiser simply reports that instead of taking the player down with it.
///
/// cava is used purely as the audio-capture layer because it already supports
/// every desktop we ship to: PipeWire/PulseAudio on Linux, CoreAudio (with the
/// macOS 14.2+ output tap) on macOS, and WASAPI on Windows. The visual polish
/// (mel re-binning, springs, beat pulse, 1/8-cell bars) lives in
/// [`crate::spectrum`] and runs in-process at 60 fps.
pub struct Cava {
    values: Arc<Mutex<Vec<f64>>>,
    alive: Arc<Mutex<bool>>,
    child: Option<Child>,
    error: Option<String>,
}

/// Input backends to try, in order. Windows needs no `[input]` section at all
/// (cava auto-selects the default output device); macOS defaults to CoreAudio
/// and can tap system output without a loopback device on 14.2+.
fn input_blocks() -> &'static [&'static str] {
    if cfg!(target_os = "linux") {
        &[
            "[input]\nmethod = pipewire\nsource = auto\n",
            "[input]\nmethod = pulse\nsource = auto\n",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "[input]\nmethod = coreaudio\nsource = tap\n",
            "[input]\nmethod = coreaudio\nsource = auto\n",
        ]
    } else {
        // Windows: "Should capture the audio from the default output device
        // automatically. No config needed." — upstream cava README.
        &[""]
    }
}

fn config_text(bars: u16, input: &str) -> String {
    format!(
        "[general]\nframerate = 60\nbars = {bars}\nautosens = 1\n\
         lower_cutoff_freq = 50\nhigher_cutoff_freq = 12000\n\n\
         {input}\
         [smoothing]\nnoise_reduction = 20\n\n\
         [output]\nmethod = raw\n\
         data_format = ascii\nascii_max_range = 1000\n\
         bar_delimiter = 59\nframe_delimiter = 10\n"
    )
}

struct Spawned {
    child: Child,
    values: Arc<Mutex<Vec<f64>>>,
    alive: Arc<Mutex<bool>>,
}

fn spawn_with(config_path: &std::path::Path, bars: u16, input: &str) -> Result<Spawned, String> {
    let raw = config_text(bars, input);
    fs::write(config_path, raw).map_err(|err| format!("could not write cava config: {err}"))?;

    let mut command = Command::new("cava");
    command
        .arg("-p")
        .arg(config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // cava only exists to animate the UI: if the player dies, it dies.
    crate::ipc::die_with_parent(&mut command);
    let mut child = command.spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            "install cava for the spectrum visualiser".to_string()
        } else {
            format!("cava failed to start: {err}")
        }
    })?;

    let values = Arc::new(Mutex::new(vec![0.0; bars as usize]));
    let alive = Arc::new(Mutex::new(true));
    if let Some(stdout) = child.stdout.take() {
        let shared = Arc::clone(&values);
        let alive_reader = Arc::clone(&alive);
        thread::Builder::new()
            .name("cava-reader".to_string())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let parsed: Vec<f64> = line
                        .split(';')
                        .filter(|part| !part.trim().is_empty())
                        .filter_map(|part| part.trim().parse::<f64>().ok())
                        .map(|value| (value / 1000.0).clamp(0.0, 1.0))
                        .collect();
                    if parsed.is_empty() {
                        continue;
                    }
                    if let Ok(mut current) = shared.lock() {
                        // cava already runs at 60 fps; keep only a whisper of
                        // smoothing so the spring engine does the shaping.
                        if current.len() != parsed.len() {
                            *current = parsed;
                        } else {
                            for (slot, value) in current.iter_mut().zip(parsed) {
                                *slot = *slot * 0.25 + value * 0.75;
                            }
                        }
                    }
                }
                if let Ok(mut alive) = alive_reader.lock() {
                    *alive = false;
                }
            })
            .ok();
    }

    Ok(Spawned {
        child,
        values,
        alive,
    })
}

impl Cava {
    pub fn spawn(bars: u16) -> Self {
        let config_path = config::runtime_dir().join("cava.conf");
        if let Some(parent) = config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let bars = bars.clamp(4, 96);
        let mut last_error = None;

        // Each backend gets a moment to prove it can open an audio source.
        // PipeWire machines that only run PulseAudio (and macOS versions
        // before the CoreAudio tap) fall through to the next variant instead
        // of leaving a dead panel on screen.
        for input in input_blocks() {
            let mut spawned = match spawn_with(&config_path, bars, input) {
                Ok(spawned) => spawned,
                Err(err) => {
                    last_error = Some(err);
                    continue;
                }
            };
            thread::sleep(std::time::Duration::from_millis(160));
            match spawned.child.try_wait() {
                Ok(Some(_)) => {
                    last_error = Some("cava could not open an audio source".to_string());
                    continue;
                }
                _ => {
                    return Self {
                        values: spawned.values,
                        alive: spawned.alive,
                        child: Some(spawned.child),
                        error: None,
                    };
                }
            }
        }

        Self {
            values: Arc::new(Mutex::new(vec![0.0; bars as usize])),
            alive: Arc::new(Mutex::new(false)),
            child: None,
            error: Some(
                last_error.unwrap_or_else(|| "cava could not open an audio source".to_string()),
            ),
        }
    }

    pub fn values(&self) -> Vec<f64> {
        self.values
            .lock()
            .map(|values| values.clone())
            .unwrap_or_default()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn running(&self) -> bool {
        self.child.is_some() && self.alive.lock().map(|alive| *alive).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_never_hardcodes_pipewire_on_other_platforms() {
        let inputs = input_blocks();
        assert!(!inputs.is_empty());
        if cfg!(target_os = "linux") {
            assert!(inputs[0].contains("pipewire"));
            assert!(inputs.iter().any(|block| block.contains("pulse")));
        } else if cfg!(target_os = "macos") {
            assert!(inputs[0].contains("coreaudio"));
            assert!(inputs[0].contains("source = tap"));
        } else if cfg!(windows) {
            // Windows needs no input section: cava picks WASAPI itself.
            assert_eq!(inputs, &[""]);
        }
    }

    #[test]
    fn raw_output_does_not_depend_on_dev_stdout() {
        let text = config_text(28, "");
        assert!(text.contains("method = raw"));
        assert!(!text.contains("/dev/stdout"));
        assert!(text.contains("framerate = 60"));
    }
}
