#![cfg_attr(windows, windows_subsystem = "windows")]

mod applog;
mod chaseplane;
mod console;
mod engine;
mod gui;
mod options;
mod state;
mod trackaudio;
mod tracking;

use state::StateHandle;
use tokio::sync::mpsc;

const USAGE: &str = "Usage: comspotter [--headless]\n\n  --headless  Run without the GUI, logging to the console only";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let has_terminal = console::attach_parent();
    let mut headless = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--headless" => headless = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("Unknown argument: {other}\n\n{USAGE}").into()),
        }
    }

    if headless || has_terminal {
        if headless && !has_terminal {
            console::alloc();
        }
        console::keep_ctrl_c_enabled();
    }

    let options = options::load();
    app_info!("Loaded options: {:?}", options);

    let runtime = tokio::runtime::Runtime::new()?;
    let state = StateHandle::new(options.clone());
    // Kept alive until exit: the engine stops when all senders are dropped.
    let (control_tx, control_rx) = mpsc::unbounded_channel();
    let engine_task = runtime.spawn(engine::run(state.clone(), control_rx, options));

    if headless {
        runtime.block_on(async move {
            tokio::signal::ctrl_c().await?;
            drop(control_tx);
            engine_task.await?;
            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
        if has_terminal {
            console::restore_prompt();
        }
        return Ok(());
    }

    if has_terminal {
        runtime.spawn(async {
            if tokio::signal::ctrl_c().await.is_ok() && !applog::request_close() {
                std::process::exit(0);
            }
        });
    }

    // eframe has to own the main thread; the engine runs on the runtime's worker threads.
    gui::run(state, control_tx)?;
    if has_terminal {
        console::restore_prompt();
    }
    Ok(())
}
