//! `crates/atomic`'s binary entry point. The whole GUI shell (`AtomicApp`) is
//! defined in [`app`] — see that module's doc for how it's split.

mod app;
mod chrome_bridge;
mod chrome_engine;

use app::AtomicApp;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Atomic",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(AtomicApp::default()))),
    )
}
