use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config;

/// Drives a `cava` subprocess and exposes its raw bar values to the renderer.
/// Cava is optional: when it is missing or cannot reach an audio sink the
/// visualiser simply reports that instead of taking the player down with it.
pub struct Cava {
    values: Arc<Mutex<Vec<f64>>>,
    alive: Arc<Mutex<bool>>,
    child: Option<Child>,
    error: Option<String>,
}

impl Cava {
    pub fn spawn(bars: u16) -> Self {
        let values = Arc::new(Mutex::new(vec![0.0; bars as usize]));
        let alive = Arc::new(Mutex::new(false));
        let config_path = config::runtime_dir().join("cava.conf");
        if let Some(parent) = config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let bars = bars.clamp(4, 96);
        let raw = format!(
            "[general]\nframerate = 30\nbars = {bars}\n\n\
             [input]\nmethod = pipewire\nsource = auto\n\n\
             [output]\nmethod = raw\nraw_target = /dev/stdout\n\
             data_format = ascii\nascii_max_range = 1000\n\
             bar_delimiter = 59\nframe_delimiter = 10\n"
        );
        if let Err(err) = fs::write(&config_path, raw) {
            return Self {
                values,
                alive,
                child: None,
                error: Some(format!("could not write cava config: {err}")),
            };
        }

        let mut command = Command::new("cava");
        command
            .arg("-p")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // cava only exists to animate the UI: if the player dies, it dies.
        crate::ipc::die_with_parent(&mut command);
        let child = command.spawn();

        let mut child = match child {
            Ok(child) => child,
            Err(err) => {
                return Self {
                    values,
                    alive,
                    child: None,
                    error: Some(if err.kind() == std::io::ErrorKind::NotFound {
                        "install cava for the spectrum visualiser".to_string()
                    } else {
                        format!("cava failed to start: {err}")
                    }),
                };
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let values = Arc::clone(&values);
            let alive = Arc::clone(&alive);
            *alive.lock().unwrap() = true;
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
                        if let Ok(mut current) = values.lock() {
                            if current.len() != parsed.len() {
                                *current = parsed;
                            } else {
                                // Light smoothing keeps the bars from jittering.
                                for (slot, value) in current.iter_mut().zip(parsed) {
                                    *slot = *slot * 0.55 + value * 0.45;
                                }
                            }
                        }
                    }
                    if let Ok(mut alive) = alive.lock() {
                        *alive = false;
                    }
                })
                .ok();
        }

        let mut cava = Self {
            values,
            alive,
            child: Some(child),
            error: None,
        };
        // Give cava a moment to prove it can open an audio source. It exits
        // quickly when no sink exists; report that instead of drawing a dead
        // panel forever.
        thread::sleep(std::time::Duration::from_millis(120));
        if let Some(child) = cava.child.as_mut() {
            if matches!(child.try_wait(), Ok(Some(_))) {
                cava.error = Some("cava could not open an audio source".to_string());
                cava.child = None;
            }
        }
        cava
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

impl Drop for Cava {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
