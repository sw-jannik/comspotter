use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crate::chaseplane::{DEFAULT_URL as DEFAULT_CHASEPLANE_URL, DEFAULT_VIEW_THEME};
use crate::trackaudio::DEFAULT_URL as DEFAULT_TRACKAUDIO_URL;

const FILE_NAME: &str = "comspotter.options.toml";

#[derive(Clone, Debug)]
pub struct Options {
    pub chaseplane: ChaseplaneOptions,
    pub trackaudio: TrackAudioOptions,
    pub tracking: TrackingOptions,
}

#[derive(Clone, Debug)]
pub struct ChaseplaneOptions {
    pub url: String,
    pub view_switching: bool,
    pub force_view_switch: bool,
    pub view_profile_theme: String,
}

#[derive(Clone, Debug)]
pub struct TrackAudioOptions {
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct TrackingOptions {
    pub track_threshold: Duration,
    pub auto_spot_threshold: Duration,
    pub scene_change_threshold: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            chaseplane: ChaseplaneOptions {
                url: DEFAULT_CHASEPLANE_URL.to_string(),
                view_switching: false,
                force_view_switch: false,
                view_profile_theme: DEFAULT_VIEW_THEME.to_string(),
            },
            trackaudio: TrackAudioOptions {
                url: DEFAULT_TRACKAUDIO_URL.to_string(),
            },
            tracking: TrackingOptions {
                track_threshold: Duration::from_millis(500),
                auto_spot_threshold: Duration::from_millis(10_000),
                scene_change_threshold: Duration::from_millis(5_000),
            },
        }
    }
}

fn options_path() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| io::Error::other("executable has no parent directory"))?;
    Ok(dir.join(FILE_NAME))
}

fn default_contents() -> String {
    let d = Options::default();
    let ms = |d: Duration| d.as_millis();
    format!(
        r#"# ComSpotter options. All durations are in milliseconds.

[chaseplane]
# WebSocket URL of the ChasePlane API.
url = "{chaseplane_url}"

# After tracking a station, also switch to the saved ChasePlane view (active airport) closest to the aircraft.
view_switching = {view_switching}

# Switch to the closest view even if it is already the current one.
force_view_switch = {force_view_switch}

# Only ChasePlane views with this profile_theme are used for view switching.
view_profile_theme = "{view_profile_theme}"

[trackaudio]
# WebSocket URL of the TrackAudio instance (host, host:port or full URL).
url = "{trackaudio_url}"

[tracking]
# Minimum continuous transmission time before a station is tracked in ChasePlane.
track_threshold_ms = {track}

# Idle time with no known traffic transmitting before ChasePlane's auto-spot is enabled.
auto_spot_threshold_ms = {auto_spot}

# Minimum time between scene change events.
scene_change_threshold_ms = {scene_change}
"#,
        chaseplane_url = d.chaseplane.url,
        view_switching = d.chaseplane.view_switching,
        force_view_switch = d.chaseplane.force_view_switch,
        view_profile_theme = d.chaseplane.view_profile_theme,
        trackaudio_url = d.trackaudio.url,
        track = ms(d.tracking.track_threshold),
        auto_spot = ms(d.tracking.auto_spot_threshold),
        scene_change = ms(d.tracking.scene_change_threshold),
    )
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

fn section<'a>(values: &'a toml::Table, name: &str) -> Option<&'a toml::Table> {
    values.get(name).and_then(|v| v.as_table())
}

fn string_or(section: Option<&toml::Table>, key: &str, default: &str) -> String {
    section
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_string()
}

fn bool_or(section: Option<&toml::Table>, key: &str, default: bool) -> bool {
    section
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn duration_ms_or(section: Option<&toml::Table>, key: &str, default: Duration) -> Duration {
    section
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_integer())
        .and_then(|v| u64::try_from(v).ok())
        .map(Duration::from_millis)
        .unwrap_or(default)
}

fn from_table(values: &toml::Table) -> Options {
    let d = Options::default();
    let chaseplane = section(values, "chaseplane");
    let trackaudio = section(values, "trackaudio");
    let tracking = section(values, "tracking");

    Options {
        chaseplane: ChaseplaneOptions {
            url: string_or(chaseplane, "url", &d.chaseplane.url),
            view_switching: bool_or(chaseplane, "view_switching", d.chaseplane.view_switching),
            force_view_switch: bool_or(
                chaseplane,
                "force_view_switch",
                d.chaseplane.force_view_switch,
            ),
            view_profile_theme: string_or(
                chaseplane,
                "view_profile_theme",
                &d.chaseplane.view_profile_theme,
            ),
        },
        trackaudio: TrackAudioOptions {
            url: string_or(trackaudio, "url", &d.trackaudio.url),
        },
        tracking: TrackingOptions {
            track_threshold: duration_ms_or(
                tracking,
                "track_threshold_ms",
                d.tracking.track_threshold,
            ),
            auto_spot_threshold: duration_ms_or(
                tracking,
                "auto_spot_threshold_ms",
                d.tracking.auto_spot_threshold,
            ),
            scene_change_threshold: duration_ms_or(
                tracking,
                "scene_change_threshold_ms",
                d.tracking.scene_change_threshold,
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_keys_missing() {
        let o = from_table(&parse(""));
        assert!(!o.chaseplane.view_switching);
        assert_eq!(o.chaseplane.view_profile_theme, "WORLD_TOWER");
        assert_eq!(o.chaseplane.url, "ws://127.0.0.1:8652/");
        assert_eq!(o.trackaudio.url, "ws://127.0.0.1:49080/ws");
        assert_eq!(o.tracking.track_threshold, Duration::from_millis(500));
    }

    #[test]
    fn reads_values_from_sections() {
        let o = from_table(&parse(
            r#"
[chaseplane]
url = "ws://192.168.1.5:8652/"
view_switching = true
view_profile_theme = "CUSTOM"

[trackaudio]
url = "192.168.1.6"

[tracking]
track_threshold_ms = 100
"#,
        ));
        assert_eq!(o.chaseplane.url, "ws://192.168.1.5:8652/");
        assert!(o.chaseplane.view_switching);
        assert_eq!(o.chaseplane.view_profile_theme, "CUSTOM");
        assert_eq!(o.trackaudio.url, "192.168.1.6");
        assert_eq!(o.tracking.track_threshold, Duration::from_millis(100));
        assert_eq!(
            o.tracking.auto_spot_threshold,
            Duration::from_millis(10_000)
        );
    }

    #[test]
    fn keys_outside_their_section_are_ignored() {
        let o = from_table(&parse("view_switching = true\n[tracking]\nurl = \"x\""));
        assert!(!o.chaseplane.view_switching);
    }

    #[test]
    fn default_file_round_trips() {
        let d = from_table(&parse(&default_contents()));
        let expected = Options::default();
        assert_eq!(format!("{d:?}"), format!("{expected:?}"));
    }
}
