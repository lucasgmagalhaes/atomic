//! Performance settings — the "max live panes" half of the mockup's
//! "Settings: Performance (max live panes, background throttling, GPU,
//! frame cap)" spec gap. Pure client-side logic, real: [`set_pane_count`]
//! actually clamps against this.
//!
//! Background throttling, GPU selection, and per-pane frame cap all need
//! a *command* `profile-worker` doesn't have yet (its vsync loop runs a
//! fixed `TARGET_FPS` with no way to change it after spawn - see that
//! binary's own doc) - not built here to avoid touching
//! `profile_worker.rs` while another session is mid-edit on it (DNS
//! resolver wiring, coordinated over cross-session messages). Tracked as
//! the remaining half of this gap, not silently dropped.
pub struct PerformanceSettings {
    /// Caps how many panes the grid can hold at once (the mockup's 1/2/4/6
    /// toggle) - defaults to 6, the mockup's own largest preset, so the
    /// default behaves exactly like no cap at all.
    pub max_panes: usize,
}

impl PerformanceSettings {
    pub fn new() -> Self {
        PerformanceSettings { max_panes: 6 }
    }

    /// Clamps `requested` into `1..=self.max_panes` - a shell should
    /// never show zero panes (nothing to click to add one back) or more
    /// than the configured cap.
    pub fn clamp_pane_count(&self, requested: usize) -> usize {
        requested.clamp(1, self.max_panes.max(1))
    }
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self::new()
    }
}
