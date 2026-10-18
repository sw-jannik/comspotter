use eframe::egui;

use super::{degrees, gray, green, mono_grid, red, show_window, status_row, timestamp_system};
use crate::applog;
use crate::chaseplane::{ChaseplaneClient, TrafficInfo};
use crate::options;
use crate::state::AppState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailsTab {
    Camera,
    Traffic,
    Views,
    Connections,
    Log,
}

impl DetailsTab {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Camera, "Camera"),
        (Self::Traffic, "Traffic"),
        (Self::Views, "Views"),
        (Self::Connections, "Connections & options"),
        (Self::Log, "Log"),
    ];
}

pub fn show(
    ctx: &egui::Context,
    state: &AppState,
    open: &mut bool,
    tab: &mut DetailsTab,
    traffic_filter: &mut String,
) {
    show_window(
        ctx,
        egui::ViewportId::from_hash_of("comspotter-details"),
        format!("{} — Details", super::APP_TITLE),
        [820.0, 560.0],
        open,
        |ui| {
            ui.horizontal(|ui| {
                for (value, label) in DetailsTab::ALL {
                    ui.selectable_value(tab, value, label);
                }
            });
            ui.separator();

            match *tab {
                DetailsTab::Connections => connections_tab(ui, state),
                DetailsTab::Log => log_tab(ui),
                _ => match &state.chaseplane {
                    None => {
                        ui.colored_label(gray(ui), "Not connected to ChasePlane");
                    }
                    Some(chaseplane) => match *tab {
                        DetailsTab::Camera => camera_tab(ui, chaseplane),
                        DetailsTab::Traffic => traffic_tab(ui, state, chaseplane, traffic_filter),
                        _ => views_tab(ui, chaseplane),
                    },
                },
            }
        },
    );
}

fn row(ui: &mut egui::Ui, label: &str, value: impl Into<egui::WidgetText>) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}

fn camera_tab(ui: &mut egui::Ui, chaseplane: &ChaseplaneClient) {
    let Some(camera) = chaseplane.camera_state() else {
        ui.colored_label(gray(ui), "No camera state received yet");
        return;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.strong("Runtime state");
        mono_grid(ui, egui::Grid::new("camera-runtime").striped(true), |ui| {
            row(
                ui,
                "ChasePlane master enabled",
                camera.master_enabled.to_string(),
            );
            row(
                ui,
                "Auto-spot enabled",
                camera.ai_traffic_auto_target_enabled.to_string(),
            );
            let target = camera
                .ai_traffic_tracking_id
                .filter(|id| *id != 0)
                .map(|id| match chaseplane.get_traffic(id) {
                    Some(t) => format!("{} (uId {id})", t.callsign),
                    None => format!("unknown traffic (uId {id})"),
                })
                .unwrap_or_else(|| "none".into());
            row(ui, "Tracked aircraft", target);
        });

        ui.add_space(8.0);
        ui.strong("Camera position");
        match &camera.camera_position {
            None => {
                ui.colored_label(gray(ui), "none");
            }
            Some(p) => {
                mono_grid(ui, egui::Grid::new("camera-position").striped(true), |ui| {
                    row(ui, "Longitude (x)", degrees(p.x));
                    row(ui, "Altitude (y)", format!("{:.2} m", p.y));
                    row(ui, "Latitude (z)", degrees(p.z));
                    row(ui, "Pitch", format!("{:.4} rad", p.pitch));
                    row(ui, "Yaw", format!("{:.4} rad", p.yaw));
                    row(ui, "Roll", format!("{:.4} rad", p.roll));
                    row(ui, "Zoom", format!("{:.4}", p.zoom));
                    row(ui, "Preset zoom", format!("{:.4}", p.preset_zoom));
                    row(ui, "Rendered zoom", format!("{:.4}", p.rendered_zoom));
                    row(ui, "AGL min", format!("{:.2}", p.agl_min));
                });
            }
        }

        ui.add_space(8.0);
        ui.strong("Look-at position");
        match &camera.look_at_position {
            None => {
                ui.colored_label(gray(ui), "none (camera isn't looking at anything)");
            }
            Some(p) => {
                mono_grid(ui, egui::Grid::new("camera-look-at").striped(true), |ui| {
                    row(ui, "Longitude", degrees(p.longitude));
                    row(ui, "Latitude", degrees(p.latitude));
                    row(ui, "Altitude", format!("{:.2} m", p.altitude));
                    row(ui, "AGL min", format!("{:.2}", p.agl_min));
                    row(ui, "Manual", p.manual.to_string());
                });
            }
        }
    });
}

fn traffic_tab(
    ui: &mut egui::Ui,
    state: &AppState,
    chaseplane: &ChaseplaneClient,
    filter: &mut String,
) {
    let mut traffic = chaseplane.traffic();
    traffic.sort_by(|a, b| a.callsign.cmp(&b.callsign));

    let tracked = chaseplane
        .camera_state()
        .and_then(|c| c.ai_traffic_tracking_id)
        .filter(|id| *id != 0);

    ui.horizontal(|ui| {
        ui.label(format!("{} known aircraft", traffic.len()));
        ui.label("Filter:");
        ui.text_edit_singleline(filter);
    });

    let needle = filter.trim().to_lowercase();
    traffic.retain(|t| {
        needle.is_empty()
            || t.callsign.to_lowercase().contains(&needle)
            || t.sim_title.to_lowercase().contains(&needle)
            || t.display_name.to_lowercase().contains(&needle)
    });

    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        mono_grid(ui, egui::Grid::new("traffic").striped(true), |ui| {
            for header in [
                "",
                "Callsign",
                "Name",
                "Type",
                "Phase",
                "Gnd",
                "Lat",
                "Lon",
                "Alt (ft)",
                "Hdg",
                "Kt",
                "uId",
                "Other fields",
            ] {
                ui.strong(header);
            }
            ui.end_row();

            for t in &traffic {
                traffic_row(ui, state, t, tracked == Some(t.uid));
            }
        });
    });
}

fn traffic_row(ui: &mut egui::Ui, state: &AppState, t: &TrafficInfo, tracked: bool) {
    let transmitting = state
        .transmitting
        .iter()
        .any(|tx| tx.callsign.eq_ignore_ascii_case(&t.callsign));

    ui.horizontal(|ui| {
        if tracked {
            ui.colored_label(green(ui), "🎯")
                .on_hover_text("Tracked in ChasePlane");
        }
        if transmitting {
            ui.colored_label(red(ui), "📻")
                .on_hover_text("Transmitting");
        }
    });
    ui.label(&t.callsign);
    ui.label(&t.display_name);
    ui.label(&t.sim_title);
    ui.label(&t.phase);
    ui.label(if t.on_ground { "yes" } else { "no" });
    ui.label(format!("{:.5}", t.lat));
    ui.label(format!("{:.5}", t.lon));
    ui.label(format!("{:.0}", t.alt));
    ui.label(format!("{:.0}", t.heading));
    ui.label(format!("{:.0}", t.speed_knots));
    ui.label(t.uid.to_string());

    let extra = serde_json::Value::Object(t.extra.clone());
    let compact = extra.to_string();
    let shown: String = compact.chars().take(60).collect();
    let response = ui.label(if compact.len() > shown.len() {
        format!("{shown}…")
    } else {
        shown
    });
    response.on_hover_text(serde_json::to_string_pretty(&extra).unwrap_or(compact));
    ui.end_row();
}

fn views_tab(ui: &mut egui::Ui, chaseplane: &ChaseplaneClient) {
    let views = chaseplane.views();
    let current = chaseplane.current_view();
    let tracked = chaseplane.tracked_traffic();
    let closest = tracked
        .as_ref()
        .and_then(|t| chaseplane.closest_view(t.lat, t.lon))
        .map(|v| v.guid);

    ui.label(format!(
        "Airport {} · theme {} · {} view(s)",
        chaseplane.active_icao().unwrap_or_else(|| "unknown".into()),
        chaseplane.view_theme(),
        views.len()
    ));
    if let Some(t) = &tracked {
        ui.label(format!("Distances are measured to {}", t.callsign));
    }

    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        mono_grid(ui, egui::Grid::new("views").striped(true), |ui| {
            for header in ["", "Name", "ICAO", "Lat", "Lon", "Distance", "GUID"] {
                ui.strong(header);
            }
            ui.end_row();

            for v in &views {
                let mut marks = String::new();
                if current.as_deref() == Some(v.guid.as_str()) {
                    marks.push('▶');
                }
                if closest.as_deref() == Some(v.guid.as_str()) {
                    marks.push('◎');
                }
                ui.label(marks)
                    .on_hover_text("▶ current view · ◎ closest to the tracked aircraft");
                ui.label(&v.name);
                ui.label(&v.icao);
                ui.label(format!("{:.5}", v.lat));
                ui.label(format!("{:.5}", v.lon));
                ui.label(
                    chaseplane
                        .view_distance_km(&v.guid)
                        .map(|km| format!("{km:.2} km"))
                        .unwrap_or_default(),
                );
                ui.label(&v.guid);
                ui.end_row();
            }
        });
    });
}

fn connections_tab(ui: &mut egui::Ui, state: &AppState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        status_row(ui, "ChasePlane", &state.chaseplane_status);
        status_row(ui, "TrackAudio", &state.trackaudio_status);
        ui.add_space(8.0);

        ui.strong("Options in effect");
        let o = &state.options;
        mono_grid(
            ui,
            egui::Grid::new("options-in-effect").striped(true),
            |ui| {
                row(ui, "chaseplane.url", o.chaseplane.url.clone());
                row(
                    ui,
                    "chaseplane.view_switching",
                    o.chaseplane.view_switching.to_string(),
                );
                row(
                    ui,
                    "chaseplane.force_view_switch",
                    o.chaseplane.force_view_switch.to_string(),
                );
                row(
                    ui,
                    "chaseplane.auto_spot_view_switching",
                    o.chaseplane.auto_spot_view_switching.to_string(),
                );
                row(
                    ui,
                    "chaseplane.auto_spot_continuous_view_switching",
                    o.chaseplane.auto_spot_continuous_view_switching.to_string(),
                );
                row(
                    ui,
                    "chaseplane.view_profile_theme",
                    o.chaseplane.view_profile_theme.clone(),
                );
                row(ui, "trackaudio.url", o.trackaudio.url.clone());
                row(
                    ui,
                    "tracking.track_threshold",
                    format!("{} ms", o.tracking.track_threshold.as_millis()),
                );
                row(
                    ui,
                    "tracking.auto_spot_threshold",
                    format!("{} ms", o.tracking.auto_spot_threshold.as_millis()),
                );
                row(
                    ui,
                    "tracking.scene_change_threshold",
                    format!("{} ms", o.tracking.scene_change_threshold.as_millis()),
                );
            },
        );

        ui.add_space(8.0);
        let path = options::options_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| format!("unknown ({e})"));
        ui.label(format!("Options file: {path}"));
    });
}

fn log_tab(ui: &mut egui::Ui) {
    let entries = applog::entries();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for entry in &entries {
                let text = format!("{}  {}", timestamp_system(entry.at), entry.text);
                if entry.error {
                    ui.monospace(egui::RichText::new(text).color(red(ui)));
                } else {
                    ui.monospace(text);
                }
            }
        });
}
