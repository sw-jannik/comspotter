use eframe::egui;

use super::{GRAY, GREEN, RED, ago, duration_text, status_row};
use crate::state::AppState;

#[derive(Default)]
pub struct Actions {
    pub set_tracking_paused: Option<bool>,
    pub set_auto_spot_paused: Option<bool>,
    pub open_details: bool,
    pub open_options: bool,
}

pub fn show(ui: &mut egui::Ui, state: &AppState) -> Actions {
    let mut actions = Actions::default();

    egui::Panel::bottom("main_buttons").show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let label = if state.tracking_paused {
                "Resume radio tracking"
            } else {
                "Pause radio tracking"
            };
            if ui
                .add(egui::Button::new(label).selected(state.tracking_paused))
                .on_hover_text("Don't track aircraft when they transmit")
                .clicked()
            {
                actions.set_tracking_paused = Some(!state.tracking_paused);
            }

            let label = if state.auto_spot_paused {
                "Resume auto-spot"
            } else {
                "Pause auto-spot"
            };
            if ui
                .add(egui::Button::new(label).selected(state.auto_spot_paused))
                .on_hover_text(
                    "Turn ChasePlane's auto-spot off and don't enable it again while paused",
                )
                .clicked()
            {
                actions.set_auto_spot_paused = Some(!state.auto_spot_paused);
            }
        });

        ui.horizontal(|ui| {
            actions.open_details = ui.button("Details…").clicked();
            actions.open_options = ui.button("Options…").clicked();
        });
        ui.add_space(4.0);
    });

    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("ComSpotter");
            ui.add_space(4.0);

            status_row(ui, "ChasePlane", &state.chaseplane_status);
            status_row(ui, "TrackAudio", &state.trackaudio_status);
            ui.add_space(8.0);

            tracking_card(ui, state);
            radio_card(ui, state);
            camera_card(ui, state);
        });
    });

    actions
}

/// Maximum number of simultaneous transmissions that get their own row.
const TX_ROWS: usize = 3;
const BIG_ROW_HEIGHT: f32 = 26.0;

/// A single-line label that never wraps, so its row height stays constant.
fn line(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    ui.add(egui::Label::new(text).truncate());
}

/// A line that keeps its space when there is nothing to show.
fn optional_line(ui: &mut egui::Ui, text: Option<(String, egui::Color32)>) {
    match text {
        Some((text, color)) => line(ui, egui::RichText::new(text).color(color)),
        None => line(ui, " "),
    }
}

fn big_row(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(BIG_ROW_HEIGHT);
        add_contents(ui);
    });
}

fn tracking_card(ui: &mut egui::Ui, state: &AppState) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.strong("Tracking");

        let chaseplane = state.chaseplane.as_ref();
        let camera = chaseplane.and_then(|c| c.camera_state());
        let target = camera
            .as_ref()
            .and_then(|c| c.ai_traffic_tracking_id)
            .filter(|id| *id != 0);
        let auto_spot = camera
            .as_ref()
            .is_some_and(|c| c.ai_traffic_auto_target_enabled);
        let traffic = chaseplane.zip(target).and_then(|(c, id)| c.get_traffic(id));

        big_row(ui, |ui| match (&traffic, chaseplane) {
            (Some(traffic), _) => {
                line(
                    ui,
                    egui::RichText::new(&traffic.callsign).size(22.0).strong(),
                );
            }
            (None, Some(_)) => line(ui, egui::RichText::new("Nothing tracked").size(18.0)),
            (None, None) => line(
                ui,
                egui::RichText::new("Waiting for ChasePlane")
                    .size(18.0)
                    .color(GRAY),
            ),
        });

        optional_line(
            ui,
            traffic.as_ref().map(|t| {
                (
                    format!("{} — {}", t.display_name, t.sim_title),
                    ui.visuals().text_color(),
                )
            }),
        );
        optional_line(
            ui,
            traffic.as_ref().map(|t| {
                (
                    format!(
                        "{} · {:.0} ft · {:.0} kt · hdg {:.0}°",
                        if t.on_ground { "On ground" } else { &t.phase },
                        t.alt,
                        t.speed_knots,
                        t.heading
                    ),
                    ui.visuals().text_color(),
                )
            }),
        );

        ui.horizontal(|ui| {
            ui.label("Mode:");
            if auto_spot {
                ui.colored_label(GREEN, "auto-spot");
            } else if target.is_some() {
                ui.label("manual / radio tracking");
            } else {
                ui.colored_label(GRAY, "idle");
            }
        });

        optional_line(
            ui,
            Some(match &state.last_tracked {
                Some((callsign, at)) => (
                    format!("Last tracked by ComSpotter: {callsign}, {}", ago(*at)),
                    ui.visuals().text_color(),
                ),
                None => ("Nothing tracked by ComSpotter yet".to_string(), GRAY),
            }),
        );
        optional_line(
            ui,
            state
                .tracking_paused
                .then(|| ("Tracking is paused".to_string(), RED)),
        );
        optional_line(
            ui,
            state
                .auto_spot_paused
                .then(|| ("Auto-spot is paused".to_string(), RED)),
        );
    });
}

fn radio_card(ui: &mut egui::Ui, state: &AppState) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.strong("Radio");

        let total = state.transmitting.len();
        // The last row turns into a summary when there are more transmitters than rows.
        let shown = if total > TX_ROWS { TX_ROWS - 1 } else { total };

        for slot in 0..TX_ROWS {
            big_row(ui, |ui| {
                if let Some(tx) = state.transmitting.get(slot).filter(|_| slot < shown) {
                    ui.colored_label(GREEN, "●");
                    ui.label(egui::RichText::new(&tx.callsign).size(18.0).strong());
                    line(
                        ui,
                        format!(
                            "on {} for {}",
                            tx.frequency,
                            duration_text(tx.since.elapsed())
                        ),
                    );
                } else if slot == shown && total > TX_ROWS {
                    ui.colored_label(GREEN, format!("+{} more transmitting", total - shown));
                } else if slot == 0 {
                    ui.colored_label(GRAY, "Silent");
                }
            });
        }

        optional_line(
            ui,
            Some(match &state.last_transmission {
                Some(last) => {
                    let kind = if !last.known_traffic {
                        "(no target)"
                    } else {
                        ""
                    };
                    let when = match last.ended {
                        Some(ended) => format!(
                            "{}, lasted {}",
                            ago(ended),
                            duration_text(ended.duration_since(last.started))
                        ),
                        None => "in progress".to_string(),
                    };
                    (
                        format!(
                            "Last: {} on {} — {when} {kind}",
                            last.callsign, last.frequency
                        ),
                        ui.visuals().text_color(),
                    )
                }
                None => ("No transmissions yet".to_string(), GRAY),
            }),
        );
    });
}

fn camera_card(ui: &mut egui::Ui, state: &AppState) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.strong("Camera view");

        let chaseplane = state.chaseplane.as_ref();
        let views = chaseplane.map(|c| c.views()).unwrap_or_default();
        let current_guid = chaseplane.and_then(|c| c.current_view());
        let current_view = current_guid
            .as_ref()
            .and_then(|guid| views.iter().find(|v| &v.guid == guid));
        let distance_km = chaseplane
            .zip(current_view)
            .and_then(|(c, view)| c.view_distance_km(&view.guid));
        let current = current_guid.map(|guid| current_view.map_or(guid, |v| v.name.clone()));

        big_row(ui, |ui| match (current, chaseplane) {
            (Some(name), _) => line(ui, egui::RichText::new(name).size(18.0).strong()),
            (None, Some(_)) => line(
                ui,
                egui::RichText::new("No view selected by ComSpotter").color(GRAY),
            ),
            (None, None) => line(
                ui,
                egui::RichText::new("Waiting for ChasePlane").color(GRAY),
            ),
        });

        optional_line(
            ui,
            distance_km.map(|km| {
                (
                    format!("Distance to target: {km:.2} km"),
                    ui.visuals().text_color(),
                )
            }),
        );
        optional_line(
            ui,
            chaseplane.map(|c| {
                (
                    format!(
                        "Airport: {} · {} view(s) available",
                        c.active_icao().unwrap_or_else(|| "unknown".into()),
                        views.len()
                    ),
                    ui.visuals().text_color(),
                )
            }),
        );
        optional_line(
            ui,
            (!state.options.chaseplane.view_switching).then(|| {
                (
                    "View switching is disabled in the options".to_string(),
                    GRAY,
                )
            }),
        );
    });
}
