mod applog;
mod chaseplane;
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

    let options = options::load();
    app_info!("Loaded options: {:?}", options);

    let runtime = tokio::runtime::Runtime::new()?;
    let state = StateHandle::new(options.clone());
    // Kept alive until exit: the engine stops when all senders are dropped.
    let (control_tx, control_rx) = mpsc::unbounded_channel();
    runtime.spawn(engine::run(state.clone(), control_rx, options));

    if headless {
        runtime.block_on(tokio::signal::ctrl_c())?;
        return Ok(());
    }

    // eframe has to own the main thread; the engine runs on the runtime's worker threads.
    gui::run(state, control_tx)?;
    Ok(())
}
