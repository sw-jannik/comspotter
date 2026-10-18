//! The egui front end: a main window plus separate details and options windows.

mod details;
mod main_window;
mod options_window;

use std::time::{Duration, SystemTime};

use tokio::time::Instant;

use eframe::egui;
use tokio::sync::mpsc::UnboundedSender;

use crate::applog;
use crate::state::{ConnStatus, Control, StateHandle};

use details::DetailsTab;
use options_window::OptionsForm;

const APP_TITLE: &str = concat!("ComSpotter v", env!("CARGO_PKG_VERSION"));

// Status colours: bright shades on dark backgrounds, darker ones on light backgrounds,
// all chosen for at least 4.5:1 contrast against the default egui panel fill.
fn themed(ui: &egui::Ui, dark: egui::Color32, light: egui::Color32) -> egui::Color32 {
    if ui.visuals().dark_mode { dark } else { light }
}

fn green(ui: &egui::Ui) -> egui::Color32 {
    themed(
        ui,
        egui::Color32::from_rgb(90, 205, 110),
        egui::Color32::from_rgb(15, 115, 40),
    )
}

fn amber(ui: &egui::Ui) -> egui::Color32 {
    themed(
        ui,
        egui::Color32::from_rgb(240, 180, 50),
        egui::Color32::from_rgb(150, 85, 0),
    )
}

fn red(ui: &egui::Ui) -> egui::Color32 {
    themed(
        ui,
        egui::Color32::from_rgb(255, 110, 100),
        egui::Color32::from_rgb(180, 25, 25),
    )
}

/// De-emphasised text that is still comfortably readable.
fn gray(ui: &egui::Ui) -> egui::Color32 {
    themed(
        ui,
        egui::Color32::from_rgb(170, 170, 170),
        egui::Color32::from_rgb(90, 90, 90),
    )
}

pub fn run(state: StateHandle, control: UnboundedSender<Control>) -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_inner_size([440.0, 640.0])
            .with_min_inner_size([360.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        native_options,
        Box::new(move |cc| {
            applog::set_repaint(cc.egui_ctx.clone());
            Ok(Box::new(App::new(state, control)))
        }),
    )
}

struct App {
    state: StateHandle,
    control: UnboundedSender<Control>,
    details_open: bool,
    details_tab: DetailsTab,
    traffic_filter: String,
    options_form: Option<OptionsForm>,
}

impl App {
    fn new(state: StateHandle, control: UnboundedSender<Control>) -> Self {
        Self {
            state,
            control,
            details_open: false,
            details_tab: DetailsTab::Camera,
            traffic_filter: String::new(),
            options_form: None,
        }
    }

    fn send(&self, control: Control) {
        // The engine only stops when the app is shutting down.
        let _ = self.control.send(control);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let snapshot = self.state.snapshot();

        // Camera updates arrive continuously and relative times need refreshing.
        let refresh = if self.details_open { 200 } else { 500 };
        ctx.request_repaint_after(Duration::from_millis(refresh));

        let actions = main_window::show(ui, &snapshot);
        if let Some(paused) = actions.set_tracking_paused {
            self.send(Control::SetTrackingPaused(paused));
        }
        if let Some(paused) = actions.set_auto_spot_paused {
            self.send(Control::SetAutoSpotPaused(paused));
        }
        if actions.open_details {
            self.details_open = true;
        }
        if actions.open_options && self.options_form.is_none() {
            self.options_form = Some(OptionsForm::new(&snapshot.options));
        }

        if self.details_open {
            details::show(
                &ctx,
                &snapshot,
                &mut self.details_open,
                &mut self.details_tab,
                &mut self.traffic_filter,
            );
        }

        if let Some(form) = self.options_form.as_mut() {
            let mut open = true;
            let apply = options_window::show(&ctx, form, &snapshot, &mut open);
            if let Some(options) = apply {
                self.send(Control::ApplyOptions(options));
            }
            if !open {
                self.options_form = None;
            }
        }
    }
}

/// Opens a separate OS window (or an embedded one if the backend can't) and shows `add_contents`.
fn show_window(
    ctx: &egui::Context,
    id: egui::ViewportId,
    title: impl Into<String>,
    size: [f32; 2],
    open: &mut bool,
    mut add_contents: impl FnMut(&mut egui::Ui),
) {
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size([300.0, 200.0]);

    ctx.show_viewport_immediate(id, builder, |ui, _class| {
        egui::CentralPanel::default().show(ui, |ui| add_contents(ui));
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            *open = false;
        }
    });
}

fn status_color(ui: &egui::Ui, status: &ConnStatus) -> egui::Color32 {
    match status {
        ConnStatus::Connected => green(ui),
        ConnStatus::Connecting => amber(ui),
        ConnStatus::Failed(_) => red(ui),
    }
}

fn status_text(status: &ConnStatus) -> String {
    match status {
        ConnStatus::Connected => "Connected".into(),
        ConnStatus::Connecting => "Connecting…".into(),
        ConnStatus::Failed(err) => format!("Not connected: {err}"),
    }
}

/// Coloured dot + name + status, e.g. "● ChasePlane  Connected".
fn status_row(ui: &mut egui::Ui, name: &str, status: &ConnStatus) {
    ui.horizontal_wrapped(|ui| {
        let color = status_color(ui, status);
        ui.colored_label(color, "●");
        ui.strong(name);
        ui.label(status_text(status));
    });
}

fn duration_text(d: Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m {}s", secs / 60, secs % 60),
        _ => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

fn ago(at: Instant) -> String {
    format!("{} ago", duration_text(at.elapsed()))
}

fn timestamp_system(at: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(at)
        .format("%H:%M:%S")
        .to_string()
}

fn degrees(value: f64) -> String {
    format!("{value:.5}°")
}
