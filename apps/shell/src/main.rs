fn main() -> eframe::Result<()> {
    eframe::run_simple_native("Nimble", eframe::NativeOptions::default(), move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Nimble — shell stub (phase 1)");
        });
    })
}
