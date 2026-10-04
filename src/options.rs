use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crate::chaseplane::{DEFAULT_URL as DEFAULT_CHASEPLANE_URL, DEFAULT_VIEW_THEME};
use crate::trackaudio::DEFAULT_URL as DEFAULT_TRACKAUDIO_URL;
use crate::tracking::Options;

const FILE_NAME: &str = "comspotter.options.toml";
const DEFAULT_VIEW_SWITCHING: bool = false;

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

const VIEW_SWITCHING_KEY: &str = "view_switching";
const VIEW_SWITCHING_DESCRIPTION: &str = "After tracking a station, also switch to the saved ChasePlane view (active airport) closest to the aircraft.";
const VIEW_THEME_KEY: &str = "view_profile_theme";
const VIEW_THEME_DESCRIPTION: &str =
    "Only ChasePlane views with this profile_theme are used for view switching.";

const CHASEPLANE_URL_KEY: &str = "chaseplane_url";
const CHASEPLANE_URL_DESCRIPTION: &str = "WebSocket URL of the ChasePlane API.";
const TRACKAUDIO_URL_KEY: &str = "trackaudio_url";
const TRACKAUDIO_URL_DESCRIPTION: &str =
    "WebSocket URL of the TrackAudio instance (host, host:port or full URL).";

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
        out.push_str(&format!(
            "\n# {}\n{} = {}\n",
            e.description, e.key, e.default_ms
        ));
    }
    out.push_str(&format!(
        "\n# {VIEW_SWITCHING_DESCRIPTION}\n{VIEW_SWITCHING_KEY} = {DEFAULT_VIEW_SWITCHING}\n"
    ));
    out.push_str(&format!(
        "\n# {VIEW_THEME_DESCRIPTION}\n{VIEW_THEME_KEY} = \"{DEFAULT_VIEW_THEME}\"\n"
    ));
    out.push_str(&format!(
        "\n# {CHASEPLANE_URL_DESCRIPTION}\n{CHASEPLANE_URL_KEY} = \"{DEFAULT_CHASEPLANE_URL}\"\n"
    ));
    out.push_str(&format!(
        "\n# {TRACKAUDIO_URL_DESCRIPTION}\n{TRACKAUDIO_URL_KEY} = \"{DEFAULT_TRACKAUDIO_URL}\"\n"
    ));
    out
}

fn parse(contents: &str) -> toml::Table {
    match contents.parse::<toml::Table>() {
        Ok(table) => table,
        Err(e) => {
            eprintln!("Invalid options file, using defaults: {e}");
            toml::Table::new()
        }
    }
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
                toml::Table::new()
            }
            Err(e) => {
                eprintln!("Could not read options file {}: {e}", path.display());
                toml::Table::new()
            }
        },
        Err(e) => {
            eprintln!("Could not locate options file: {e}");
            toml::Table::new()
        }
    };

    from_table(&values)
}

fn string_or(values: &toml::Table, key: &str, default: &str) -> String {
    values
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_string()
}

fn from_table(values: &toml::Table) -> Options {
    let get = |index: usize| {
        let e = &ENTRIES[index];
        let ms = values
            .get(e.key)
            .and_then(|v| v.as_integer())
            .and_then(|v| u64::try_from(v).ok())
            .unwrap_or(e.default_ms);
        Duration::from_millis(ms)
    };

    Options {
        track_threshold: get(0),
        auto_spot_threshold: get(1),
        scene_change_threshold: get(2),
        view_switching: values
            .get(VIEW_SWITCHING_KEY)
            .and_then(|v| v.as_bool())
            .unwrap_or(DEFAULT_VIEW_SWITCHING),
        view_profile_theme: values
            .get(VIEW_THEME_KEY)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| DEFAULT_VIEW_THEME.to_string()),
        chaseplane_url: string_or(values, CHASEPLANE_URL_KEY, DEFAULT_CHASEPLANE_URL),
        trackaudio_url: string_or(values, TRACKAUDIO_URL_KEY, DEFAULT_TRACKAUDIO_URL),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_keys_missing() {
        let o = from_table(&parse(""));
        assert!(!o.view_switching);
        assert_eq!(o.view_profile_theme, "WORLD_TOWER");
        assert_eq!(o.track_threshold, Duration::from_millis(500));
        assert_eq!(o.chaseplane_url, "ws://127.0.0.1:8652/");
        assert_eq!(o.trackaudio_url, "ws://127.0.0.1:49080/ws");
    }

    #[test]
    fn reads_custom_urls() {
        let o = from_table(&parse(
            "chaseplane_url = \"ws://192.168.1.5:8652/\"\ntrackaudio_url = \"192.168.1.6\"",
        ));
        assert_eq!(o.chaseplane_url, "ws://192.168.1.5:8652/");
        assert_eq!(o.trackaudio_url, "192.168.1.6");
    }

    #[test]
    fn reads_values_and_default_file_round_trips() {
        let o = from_table(&parse(
            "view_switching = true\nview_profile_theme = \"CUSTOM\"\ntrack_threshold_ms = 100",
        ));
        assert!(o.view_switching);
        assert_eq!(o.view_profile_theme, "CUSTOM");
        assert_eq!(o.track_threshold, Duration::from_millis(100));

        let d = from_table(&parse(&default_contents()));
        assert!(!d.view_switching);
        assert_eq!(d.view_profile_theme, "WORLD_TOWER");
    }
}
