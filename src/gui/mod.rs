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

const GREEN: egui::Color32 = egui::Color32::from_rgb(70, 190, 90);
const AMBER: egui::Color32 = egui::Color32::from_rgb(230, 170, 40);
const RED: egui::Color32 = egui::Color32::from_rgb(220, 70, 60);
const GRAY: egui::Color32 = egui::Color32::GRAY;

pub fn run(state: StateHandle, control: UnboundedSender<Control>) -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ComSpotter")
            .with_inner_size([440.0, 560.0])
            .with_min_inner_size([360.0, 420.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ComSpotter",
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
    title: &str,
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

fn status_color(status: &ConnStatus) -> egui::Color32 {
    match status {
        ConnStatus::Connected => GREEN,
        ConnStatus::Connecting => AMBER,
        ConnStatus::Failed(_) => RED,
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
        ui.colored_label(status_color(status), "●");
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

fn ago_system(at: SystemTime) -> String {
    let elapsed = at.elapsed().unwrap_or_default();
    format!("{} ago", duration_text(elapsed))
}

fn degrees(value: f64) -> String {
    format!("{value:.5}°")
}
