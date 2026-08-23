//! Per-pane CPU/RAM/FPS telemetry — the GUI half of the mockup's "Resource
//! monitor (CPU/RAM/FPS por profile, gráfico 60s, kill process)" gap.
//! `platform_apis::process_stats` (real `GetProcessTimes`/
//! `GetProcessMemoryInfo` sampling on Windows) does the actual OS calls;
//! this module owns the throttled sampling cadence, the 60-point rolling
//! history a sparkline draws from, and FPS derived from
//! `profile::Profile::frame_generation()`. "Kill process" isn't new work -
//! it's `NimbleApp::close_pane`, already real (closes a pane's
//! `BrowserView`/`Profile`, which force-kills the child on `Drop`).
//!
//! On a platform `process_stats::sample` doesn't support yet (anything but
//! Windows - see that module's own doc), [`PaneMonitor::tick`] is a
//! documented no-op: the monitor just never accumulates history rather
//! than showing fabricated numbers.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use platform_apis::process_stats;

/// How much history a sparkline keeps - one point per [`SAMPLE_INTERVAL`],
/// so 60 points is the mockup's literal "60s graph".
const HISTORY_LEN: usize = 60;
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub struct PaneMonitor {
    last_stats: Option<process_stats::ProcessStats>,
    last_frame_generation: Option<u32>,
    last_sampled_at: Instant,
    cpu_history: VecDeque<f64>,
    latest_memory_bytes: u64,
    latest_fps: Option<f64>,
}

impl PaneMonitor {
    pub fn new() -> Self {
        PaneMonitor {
            last_stats: None,
            last_frame_generation: None,
            // In the past so the very first `tick()` call samples
            // immediately instead of waiting a full `SAMPLE_INTERVAL`.
            last_sampled_at: Instant::now() - SAMPLE_INTERVAL,
            cpu_history: VecDeque::with_capacity(HISTORY_LEN),
            latest_memory_bytes: 0,
            latest_fps: None,
        }
    }

    /// Samples `pid`/`frame_generation` at most once per [`SAMPLE_INTERVAL`]
    /// - a no-op otherwise, so calling this every GUI frame (~60Hz) is
    /// fine and expected. Silently does nothing on a sampling error
    /// (process exited, unsupported platform, ...) rather than corrupting
    /// history with a bogus point - the monitor just stops advancing.
    pub fn tick(&mut self, pid: u32, frame_generation: u32) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sampled_at);
        if elapsed < SAMPLE_INTERVAL {
            return;
        }

        let Ok(curr) = process_stats::sample(pid) else { return };
        self.latest_memory_bytes = curr.memory_bytes;

        if let Some(prev) = self.last_stats {
            let cores = process_stats::logical_core_count();
            let cpu_percent = process_stats::cpu_percent(&prev, &curr, elapsed, cores);
            self.cpu_history.push_back(cpu_percent);
            if self.cpu_history.len() > HISTORY_LEN {
                self.cpu_history.pop_front();
            }
        }
        self.last_stats = Some(curr);

        if let Some(prev_gen) = self.last_frame_generation {
            let frames = frame_generation.saturating_sub(prev_gen);
            self.latest_fps = Some(frames as f64 / elapsed.as_secs_f64());
        }
        self.last_frame_generation = Some(frame_generation);

        self.last_sampled_at = now;
    }

    pub fn latest_cpu_percent(&self) -> Option<f64> {
        self.cpu_history.back().copied()
    }

    pub fn latest_memory_bytes(&self) -> Option<u64> {
        self.last_stats.map(|_| self.latest_memory_bytes)
    }

    pub fn latest_fps(&self) -> Option<f64> {
        self.latest_fps
    }

    /// Up to [`HISTORY_LEN`] most recent CPU-percent samples, oldest
    /// first - what a sparkline draws.
    pub fn cpu_history(&self) -> impl Iterator<Item = f64> + '_ {
        self.cpu_history.iter().copied()
    }
}

impl Default for PaneMonitor {
    fn default() -> Self {
        Self::new()
    }
}
