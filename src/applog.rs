//! Console + in-memory log. The GUI shows the in-memory part in the details window.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

const MAX_ENTRIES: usize = 300;

#[derive(Clone, Debug)]
pub struct Entry {
    pub at: SystemTime,
    pub error: bool,
    pub text: String,
}

static LOG: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
static REPAINT: OnceLock<eframe::egui::Context> = OnceLock::new();

/// Registers the egui context that is woken up whenever the log or the app state changes.
pub fn set_repaint(ctx: eframe::egui::Context) {
    let _ = REPAINT.set(ctx);
}

pub fn request_repaint() {
    if let Some(ctx) = REPAINT.get() {
        ctx.request_repaint();
    }
}

pub fn push(error: bool, text: String) {
    if error {
        eprintln!("{text}");
    } else {
        println!("{text}");
    }

    {
        let mut log = LOG.lock().unwrap();
        if log.len() >= MAX_ENTRIES {
            log.pop_front();
        }
        log.push_back(Entry {
            at: SystemTime::now(),
            error,
            text,
        });
    }
    request_repaint();
}

/// Snapshot of the retained log entries, oldest first.
pub fn entries() -> Vec<Entry> {
    LOG.lock().unwrap().iter().cloned().collect()
}

#[macro_export]
macro_rules! app_info {
    ($($arg:tt)*) => { $crate::applog::push(false, format!($($arg)*)) };
}

#[macro_export]
macro_rules! app_error {
    ($($arg:tt)*) => { $crate::applog::push(true, format!($($arg)*)) };
}
