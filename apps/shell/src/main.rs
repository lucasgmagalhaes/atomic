//! `apps/shell`'s binary entry point. The whole GUI shell (`NimbleApp`) is
//! defined in [`app`] — see that module's doc for how it's split.

mod app;

use app::NimbleApp;

fn main() -> eframe::Result<()> {
  eframe::run_native(
    "Nimble",
    eframe::NativeOptions::default(),
    Box::new(|_cc| Ok(Box::new(NimbleApp::default()))),
  )
}
