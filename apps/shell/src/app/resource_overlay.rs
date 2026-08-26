use shell::resource_monitor::PaneMonitor;

const SPARKLINE_HEIGHT: f32 = 24.0;
const SPARKLINE_MARGIN: f32 = 6.0;

/// Draws the resource-monitor overlay (mockup's "CPU/RAM/FPS por profile,
/// gráfico 60s") in `cell_rect`'s top-left corner: one text line, plus a
/// hand-drawn CPU sparkline underneath it (no `egui_plot`/charting crate
/// dependency for one small line - `monitor.cpu_history()` is already
/// exactly the 0..=60-point series a sparkline needs). Draws nothing but
/// the text placeholder "..." before the monitor's first real sample
/// arrives (see `PaneMonitor::tick`'s throttling) rather than showing
/// stale zeros.
pub fn draw_resource_overlay(ui: &egui::Ui, cell_rect: egui::Rect, monitor: &PaneMonitor) {
    let text = match (
        monitor.latest_cpu_percent(),
        monitor.latest_memory_bytes(),
        monitor.latest_fps(),
    ) {
        (Some(cpu), Some(mem), Some(fps)) => format!(
            "CPU {cpu:.0}% · RAM {:.0} MB · FPS {fps:.0}",
            mem as f64 / 1_000_000.0
        ),
        _ => "CPU ... · RAM ... · FPS ...".to_string(),
    };
    let text_pos = cell_rect.min + egui::vec2(SPARKLINE_MARGIN, SPARKLINE_MARGIN);
    ui.painter().text(
        text_pos,
        egui::Align2::LEFT_TOP,
        text,
        egui::FontId::monospace(11.0),
        egui::Color32::WHITE,
    );

    let history: Vec<f64> = monitor.cpu_history().collect();
    if history.len() < 2 {
        return;
    }
    let sparkline_rect = egui::Rect::from_min_size(
        text_pos + egui::vec2(0.0, 16.0),
        egui::vec2(
            (cell_rect.width() - SPARKLINE_MARGIN * 2.0).max(0.0),
            SPARKLINE_HEIGHT,
        ),
    );
    ui.painter()
        .rect_filled(sparkline_rect, 0.0, egui::Color32::from_black_alpha(120));
    let max = history.iter().cloned().fold(1.0_f64, f64::max); // at least 1.0 so an all-zero window doesn't divide by zero
    let points: Vec<egui::Pos2> = history
        .iter()
        .enumerate()
        .map(|(i, &cpu)| {
            let x = sparkline_rect.min.x
                + (i as f32 / (history.len() - 1) as f32) * sparkline_rect.width();
            let y = sparkline_rect.max.y - (cpu / max) as f32 * sparkline_rect.height();
            egui::pos2(x, y)
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_GREEN),
    ));
}
