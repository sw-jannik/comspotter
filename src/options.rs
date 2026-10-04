use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crate::tracking::Options;

const FILE_NAME: &str = "comspotter.options.toml";

struct Entry {
    key: &'static str,
    default_ms: u64,
    description: &'static str,
}

const ENTRIES: [Entry; 3] = [
    Entry {
        key: "track_threshold_ms",
        default_ms: 500,
        description: "Minimum continuous transmission time before a station is tracked in ChasePlane.",
    },
    Entry {
        key: "auto_spot_threshold_ms",
        default_ms: 10_000,
        description: "Idle time with no known traffic transmitting before ChasePlane's auto-spot is enabled.",
    },
    Entry {
        key: "scene_change_threshold_ms",
        default_ms: 5_000,
        description: "Minimum time between scene change events.",
    },
];

fn options_path() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| io::Error::other("executable has no parent directory"))?;
    Ok(dir.join(FILE_NAME))
}

fn default_contents() -> String {
    let mut out = String::from("# ComSpotter options. All durations are in milliseconds.\n");
    for e in &ENTRIES {
        out.push_str(&format!("\n# {}\n{} = {}\n", e.description, e.key, e.default_ms));
    }
    out
}

fn parse(contents: &str) -> HashMap<String, u64> {
    let table = match contents.parse::<toml::Table>() {
        Ok(table) => table,
        Err(e) => {
            eprintln!("Invalid options file, using defaults: {e}");
            return HashMap::new();
        }
    };
    table
        .into_iter()
        .filter_map(|(key, value)| {
            let ms = value.as_integer().and_then(|v| u64::try_from(v).ok())?;
            Some((key, ms))
        })
        .collect()
}

/// Loads options from the file next to the executable, creating it with defaults if missing.
/// Missing or invalid values fall back to their defaults.
pub fn load() -> Options {
    let values = match options_path() {
        Ok(path) => match fs::read_to_string(&path) {
            Ok(contents) => parse(&contents),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if let Err(e) = fs::write(&path, default_contents()) {
                    eprintln!("Could not create options file {}: {e}", path.display());
                }
                HashMap::new()
            }
            Err(e) => {
                eprintln!("Could not read options file {}: {e}", path.display());
                HashMap::new()
            }
        },
        Err(e) => {
            eprintln!("Could not locate options file: {e}");
            HashMap::new()
        }
    };

    let get = |index: usize| {
        let e = &ENTRIES[index];
        Duration::from_millis(values.get(e.key).copied().unwrap_or(e.default_ms))
    };

    Options {
        track_threshold: get(0),
        auto_spot_threshold: get(1),
        scene_change_threshold: get(2),
    }
}
