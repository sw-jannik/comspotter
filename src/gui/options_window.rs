use std::time::Duration;

use eframe::egui;

use super::{gray, green, mono_grid, red, show_window};
use crate::options::{self, Options};
use crate::state::AppState;

/// Editable copy of the options, applied only when the user presses "Save & apply".
pub struct OptionsForm {
    draft: Options,
    track_ms: u64,
    auto_spot_ms: u64,
    scene_change_ms: u64,
    // (is_error, text) result of the last save attempt.
    message: Option<(bool, String)>,
}

impl OptionsForm {
    pub fn new(options: &Options) -> Self {
        let mut form = Self {
            draft: options.clone(),
            track_ms: 0,
            auto_spot_ms: 0,
            scene_change_ms: 0,
            message: None,
        };
        form.load(options);
        form
    }

    fn load(&mut self, options: &Options) {
        self.draft = options.clone();
        self.track_ms = options.tracking.track_threshold.as_millis() as u64;
        self.auto_spot_ms = options.tracking.auto_spot_threshold.as_millis() as u64;
        self.scene_change_ms = options.tracking.scene_change_threshold.as_millis() as u64;
    }

    /// The edited options with whitespace trimmed, or why they're invalid.
    fn validated(&self) -> Result<Options, String> {
        let mut options = self.draft.clone();
        options.chaseplane.url = options.chaseplane.url.trim().to_string();
        options.chaseplane.view_profile_theme =
            options.chaseplane.view_profile_theme.trim().to_string();
        options.trackaudio.url = options.trackaudio.url.trim().to_string();
        options.tracking.track_threshold = Duration::from_millis(self.track_ms);
        options.tracking.auto_spot_threshold = Duration::from_millis(self.auto_spot_ms);
        options.tracking.scene_change_threshold = Duration::from_millis(self.scene_change_ms);

        if options.chaseplane.url.is_empty() {
            return Err("The ChasePlane URL can't be empty".into());
        }
        if options.trackaudio.url.is_empty() {
            return Err("The TrackAudio URL can't be empty".into());
        }
        if options.chaseplane.view_profile_theme.is_empty() {
            return Err("The view profile theme can't be empty".into());
        }
        Ok(options)
    }
}

/// Shows the options window. Returns the options to apply when the user saves them.
pub fn show(
    ctx: &egui::Context,
    form: &mut OptionsForm,
    state: &AppState,
    open: &mut bool,
) -> Option<Options> {
    let mut to_apply = None;

    show_window(
        ctx,
        egui::ViewportId::from_hash_of("comspotter-options"),
        format!("{} — Options", super::APP_TITLE),
        [560.0, 600.0],
        open,
        |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                fields(ui, form);
                ui.add_space(8.0);
                if let Some(options) = buttons(ui, form, state) {
                    to_apply = Some(options);
                }
            });
        },
    );

    to_apply
}

fn fields(ui: &mut egui::Ui, form: &mut OptionsForm) {
    let draft = &mut form.draft;

    ui.heading("ChasePlane");
    mono_grid(
        ui,
        egui::Grid::new("options-chaseplane")
            .num_columns(2)
            .spacing([12.0, 6.0]),
        |ui| {
            ui.label("URL");
            ui.add(egui::TextEdit::singleline(&mut draft.chaseplane.url).desired_width(300.0))
                .on_hover_text("WebSocket URL of the ChasePlane API. Changing it reconnects.");
            ui.end_row();

            ui.label("View profile theme");
            ui.add(
                egui::TextEdit::singleline(&mut draft.chaseplane.view_profile_theme)
                    .desired_width(300.0),
            )
            .on_hover_text("Only views with this profile theme are used. Changing it reconnects.");
            ui.end_row();
        },
    );
    ui.checkbox(
        &mut draft.chaseplane.view_switching,
        "View switching: also switch to the closest saved view after tracking",
    );
    ui.checkbox(
        &mut draft.chaseplane.force_view_switch,
        "Force view switch: switch even if the closest view is already current",
    );
    ui.checkbox(
        &mut draft.chaseplane.auto_spot_view_switching,
        "Auto-spot view switching: also switch views when auto-spot picks an aircraft",
    );
    ui.checkbox(
        &mut draft.chaseplane.auto_spot_continuous_view_switching,
        "Continuous auto-spot view switching: keep switching as the aircraft moves",
    );

    ui.add_space(8.0);
    ui.heading("TrackAudio");
    mono_grid(
        ui,
        egui::Grid::new("options-trackaudio")
            .num_columns(2)
            .spacing([12.0, 6.0]),
        |ui| {
            ui.label("URL");
            ui.add(egui::TextEdit::singleline(&mut draft.trackaudio.url).desired_width(300.0))
                .on_hover_text(
                    "WebSocket URL of TrackAudio (full URL, host or host:port). Changing it reconnects.",
                );
            ui.end_row();
        },
    );

    ui.add_space(8.0);
    ui.heading("Tracking");
    mono_grid(
        ui,
        egui::Grid::new("options-tracking")
            .num_columns(2)
            .spacing([12.0, 6.0]),
        |ui| {
            ui.label("Track threshold");
            ui.add(
                egui::DragValue::new(&mut form.track_ms)
                    .suffix(" ms")
                    .speed(10),
            )
            .on_hover_text("Continuous transmission time before a station is tracked");
            ui.end_row();

            ui.label("Auto-spot threshold");
            ui.add(
                egui::DragValue::new(&mut form.auto_spot_ms)
                    .suffix(" ms")
                    .speed(50),
            )
            .on_hover_text(
                "Idle time with no known traffic transmitting before auto-spot is enabled",
            );
            ui.end_row();

            ui.label("Scene change threshold");
            ui.add(
                egui::DragValue::new(&mut form.scene_change_ms)
                    .suffix(" ms")
                    .speed(50),
            )
            .on_hover_text("Minimum time between scene changes");
            ui.end_row();
        },
    );
}

fn buttons(ui: &mut egui::Ui, form: &mut OptionsForm, state: &AppState) -> Option<Options> {
    let mut to_apply = None;
    let edited = form.validated().ok();
    let dirty = edited.as_ref() != Some(&state.options);

    ui.horizontal(|ui| {
        if ui
            .add_enabled(dirty, egui::Button::new("Save & apply"))
            .clicked()
        {
            match form.validated() {
                Err(err) => form.message = Some((true, err)),
                Ok(options) => {
                    form.message = Some(match options::save(&options) {
                        Ok(()) => (false, "Saved and applied".to_string()),
                        Err(err) => (
                            true,
                            format!("Applied, but the options file could not be written: {err}"),
                        ),
                    });
                    form.load(&options);
                    to_apply = Some(options);
                }
            }
        }

        if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
            form.load(&state.options);
            form.message = None;
        }

        if ui.button("Reset to defaults").clicked() {
            form.load(&Options::default());
            form.message = None;
        }
    });

    match &form.message {
        Some((true, text)) => ui.colored_label(red(ui), text),
        Some((false, text)) => ui.colored_label(green(ui), text),
        None if dirty => ui.colored_label(gray(ui), "Unsaved changes"),
        None => ui.colored_label(gray(ui), "Options are up to date"),
    };

    if let Ok(path) = options::options_path() {
        ui.add_space(4.0);
        ui.colored_label(gray(ui), format!("File: {}", path.display()));
    }

    to_apply
}
