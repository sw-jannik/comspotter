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

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = if state.tracking_paused {
                    "Resume tracking"
                } else {
                    "Pause tracking"
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

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                actions.open_details = ui.button("Details…").clicked();
                actions.open_options = ui.button("Options…").clicked();
            });
        });
    });

    actions
}

fn tracking_card(ui: &mut egui::Ui, state: &AppState) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.strong("Tracking");

        let Some(chaseplane) = &state.chaseplane else {
            ui.colored_label(GRAY, "Waiting for ChasePlane");
            return;
        };

        let camera = chaseplane.camera_state();
        let target = camera
            .as_ref()
            .and_then(|c| c.ai_traffic_tracking_id)
            .filter(|id| *id != 0);
        let auto_spot = camera
            .as_ref()
            .is_some_and(|c| c.ai_traffic_auto_target_enabled);

        match target.and_then(|id| chaseplane.get_traffic(id)) {
            Some(traffic) => {
                ui.label(egui::RichText::new(&traffic.callsign).size(22.0).strong());
                ui.label(format!("{} — {}", traffic.display_name, traffic.sim_title));
                ui.label(format!(
                    "{} · {:.0} ft · {:.0} kt · hdg {:.0}°",
                    if traffic.on_ground {
                        "On ground"
                    } else {
                        &traffic.phase
                    },
                    traffic.alt,
                    traffic.speed_knots,
                    traffic.heading
                ));
            }
            None => {
                ui.label(egui::RichText::new("Nothing tracked").size(18.0));
            }
        }

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

        if let Some((callsign, at)) = &state.last_tracked {
            ui.label(format!(
                "Last tracked by ComSpotter: {callsign}, {}",
                ago(*at)
            ));
        }
        if state.tracking_paused {
            ui.colored_label(RED, "Tracking is paused");
        }
        if state.auto_spot_paused {
            ui.colored_label(RED, "Auto-spot is paused");
        }
    });
}

fn radio_card(ui: &mut egui::Ui, state: &AppState) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.strong("Radio");

        if state.transmitting.is_empty() {
            ui.colored_label(GRAY, "Silent");
        } else {
            for tx in &state.transmitting {
                ui.horizontal(|ui| {
                    ui.colored_label(GREEN, "●");
                    ui.label(egui::RichText::new(&tx.callsign).size(18.0).strong());
                    ui.label(format!(
                        "on {} for {}",
                        tx.frequency,
                        duration_text(tx.since.elapsed())
                    ));
                });
            }
        }

        match &state.last_transmission {
            Some(last) => {
                let kind = if last.known_traffic {
                    "known aircraft"
                } else {
                    "not in ChasePlane traffic"
                };
                let when = match last.ended {
                    Some(ended) => format!(
                        "{}, lasted {}",
                        ago(ended),
                        duration_text(ended.duration_since(last.started))
                    ),
                    None => "in progress".to_string(),
                };
                ui.label(format!(
                    "Last: {} on {} ({kind}) — {when}",
                    last.callsign, last.frequency
                ));
            }
            None => {
                ui.colored_label(GRAY, "No transmissions yet");
            }
        }
    });
}

fn camera_card(ui: &mut egui::Ui, state: &AppState) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.strong("Camera view");

        let Some(chaseplane) = &state.chaseplane else {
            ui.colored_label(GRAY, "Waiting for ChasePlane");
            return;
        };

        let views = chaseplane.views();
        let current = chaseplane.current_view().map(|guid| {
            views
                .iter()
                .find(|v| v.guid == guid)
                .map_or(guid, |v| v.name.clone())
        });
        match current {
            Some(name) => ui.label(egui::RichText::new(name).size(18.0)),
            None => ui.colored_label(GRAY, "No view selected by ComSpotter"),
        };

        ui.label(format!(
            "Airport: {} · {} view(s) available",
            chaseplane.active_icao().unwrap_or_else(|| "unknown".into()),
            views.len()
        ));
        if !state.options.chaseplane.view_switching {
            ui.colored_label(GRAY, "View switching is disabled in the options");
        }
    });
}
