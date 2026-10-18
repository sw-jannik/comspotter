use std::time::Duration;

use eframe::egui;

use super::{gray, green, red, show_window_panels};
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

    show_window_panels(
        ctx,
        egui::ViewportId::from_hash_of("comspotter-options"),
        format!("{} — Options", super::APP_TITLE),
        [600.0, 720.0],
        open,
        |ui| {
            egui::Panel::bottom("options_footer").show(ui, |ui| {
                ui.add_space(8.0);
                if let Some(options) = footer(ui, form, state) {
                    to_apply = Some(options);
                }
                ui.add_space(8.0);
            });

            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| fields(ui, form));
            });
        },
    );

    to_apply
}

/// Muted explanatory text below a setting.
fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.5).color(gray(ui))).wrap());
}

fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    ui.heading(title);
    ui.add_space(4.0);
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            add_contents(ui);
        });
}

fn text_field(ui: &mut egui::Ui, label: &str, description: &str, value: &mut String) {
    ui.label(egui::RichText::new(label).strong());
    ui.add(
        egui::TextEdit::singleline(value)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(6, 4)),
    );
    hint(ui, description);
}

fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str, description: &str) {
    ui.checkbox(value, egui::RichText::new(label).strong());
    ui.horizontal_wrapped(|ui| {
        ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
        hint(ui, description);
    });
}

fn duration_field(ui: &mut egui::Ui, label: &str, description: &str, ms: &mut u64, speed: u32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::DragValue::new(ms).suffix(" ms").speed(speed));
        });
    });
    hint(ui, description);
}

fn fields(ui: &mut egui::Ui, form: &mut OptionsForm) {
    let draft = &mut form.draft;

    section(ui, "ChasePlane", |ui| {
        text_field(
            ui,
            "URL",
            "WebSocket URL of the ChasePlane API. Changing it reconnects.",
            &mut draft.chaseplane.url,
        );
        ui.add_space(6.0);
        text_field(
            ui,
            "View profile theme",
            "Only views with this profile theme are used. Changing it reconnects.",
            &mut draft.chaseplane.view_profile_theme,
        );
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(2.0);
        toggle(
            ui,
            &mut draft.chaseplane.view_switching,
            "View switching",
            "Also switch to the closest saved view after tracking.",
        );
        ui.add_space(4.0);
        toggle(
            ui,
            &mut draft.chaseplane.force_view_switch,
            "Force view switch",
            "Switch even if the closest view is already the current one.",
        );
        ui.add_space(4.0);
        toggle(
            ui,
            &mut draft.chaseplane.auto_spot_view_switching,
            "Auto-spot view switching",
            "Also switch views when auto-spot picks an aircraft.",
        );
        ui.add_space(4.0);
        toggle(
            ui,
            &mut draft.chaseplane.auto_spot_continuous_view_switching,
            "Continuous auto-spot view switching",
            "Keep switching views as the aircraft moves.",
        );
    });

    section(ui, "TrackAudio", |ui| {
        text_field(
            ui,
            "URL",
            "WebSocket URL of TrackAudio (full URL, host or host:port). Changing it reconnects.",
            &mut draft.trackaudio.url,
        );
    });

    section(ui, "Tracking", |ui| {
        duration_field(
            ui,
            "Track threshold",
            "Continuous transmission time before a station is tracked.",
            &mut form.track_ms,
            10,
        );
        ui.add_space(6.0);
        duration_field(
            ui,
            "Auto-spot threshold",
            "Idle time with no known traffic transmitting before auto-spot is enabled.",
            &mut form.auto_spot_ms,
            50,
        );
        ui.add_space(6.0);
        duration_field(
            ui,
            "Scene change threshold",
            "Minimum time between scene changes.",
            &mut form.scene_change_ms,
            50,
        );
    });

    if let Ok(path) = options::options_path() {
        ui.add_space(10.0);
        hint(ui, &format!("Options file: {}", path.display()));
    }
    ui.add_space(6.0);
}

fn footer(ui: &mut egui::Ui, form: &mut OptionsForm, state: &AppState) -> Option<Options> {
    let mut to_apply = None;
    let edited = form.validated().ok();
    let dirty = edited.as_ref() != Some(&state.options);

    match &form.message {
        Some((true, text)) => ui.colored_label(red(ui), text),
        Some((false, text)) => ui.colored_label(green(ui), text),
        None if dirty => ui.colored_label(gray(ui), "Unsaved changes"),
        None => ui.colored_label(gray(ui), "Options are up to date"),
    };
    ui.add_space(4.0);

    let button_size = egui::vec2(0.0, 28.0);
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new("Reset to defaults").min_size(button_size))
            .clicked()
        {
            form.load(&Options::default());
            form.message = None;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let save = egui::Button::new("Save & apply")
                .min_size(button_size)
                .fill(ui.visuals().selection.bg_fill);
            if ui.add_enabled(dirty, save).clicked() {
                match form.validated() {
                    Err(err) => form.message = Some((true, err)),
                    Ok(options) => {
                        form.message = Some(match options::save(&options) {
                            Ok(()) => (false, "Saved and applied".to_string()),
                            Err(err) => (
                                true,
                                format!(
                                    "Applied, but the options file could not be written: {err}"
                                ),
                            ),
                        });
                        form.load(&options);
                        to_apply = Some(options);
                    }
                }
            }

            if ui
                .add_enabled(dirty, egui::Button::new("Revert").min_size(button_size))
                .clicked()
            {
                form.load(&state.options);
                form.message = None;
            }
        });
    });

    to_apply
}
