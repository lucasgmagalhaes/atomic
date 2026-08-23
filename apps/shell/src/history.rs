//! Per-pane browsing history — the "history" half of the mockup's
//! "Downloads & history UI per profile" spec gap. In-memory, per running
//! `Pane` only (not written to disk) — deliberate, not a shortcut: the
//! shmem name (and therefore `profile-worker`'s own `storage_root`) a
//! pane spawns with is randomized per process launch, so there's no
//! stable per-profile identity yet for a persisted history file to key
//! off (see CLAUDE.md's own note on `storage`'s "doesn't persist across
//! separate profile-worker process launches" gap - this doesn't invent a
//! new deviation, it matches the existing one). A real implementation
//! would key off whatever stable profile id the "Add profile" flow
//! eventually assigns.
use std::time::Instant;

pub struct HistoryEntry {
    pub url: String,
    pub at: Instant,
}

#[derive(Default)]
pub struct History {
    entries: Vec<HistoryEntry>,
}

impl History {
    pub fn new() -> Self {
        History::default()
    }

    /// Records `url` as just-visited. Called from every real navigation
    /// this GUI actually performs (address bar, "Duplicate profile") -
    /// not a placeholder list, a real log of what `profile::Profile::navigate`
    /// was actually asked to load.
    pub fn record(&mut self, url: impl Into<String>) {
        self.entries.push(HistoryEntry { url: url.into(), at: Instant::now() });
    }

    /// Most recent visit first.
    pub fn entries(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().rev()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
