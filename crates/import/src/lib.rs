//! Real "Import from Chrome" — the mockup's onboarding flow CLAUDE.md
//! flagged as needing a decision (it conflicts with this project's "no
//! fingerprint spoofing" isolation requirement: pulling a real Chrome
//! profile's cookies/passwords into a nimble profile makes that profile
//! traceable back to the user's real browser). Decision made: full import,
//! **opt-in per profile** — nothing here runs unless a caller (`apps/shell`)
//! explicitly invokes it for one profile the user chose, never automatic,
//! never for every profile at once.
//!
//! Scope landed this pass: [`bookmarks::import_bookmarks`] (real Chrome
//! `Bookmarks` JSON) and [`history::import_history`] (real Chrome
//! `History` SQLite database, via `rusqlite`) — both read-only, no
//! decryption needed since Chrome stores neither encrypted. Cookies
//! (`Cookies` SQLite + AES-256-GCM values wrapped by Windows DPAPI) and
//! saved passwords (`Login Data` SQLite, same DPAPI wrapping) are the
//! genuinely harder half of "full import" — real decryption work, not
//! implemented in this pass, tracked as a real follow-up rather than
//! silently declared "done."
pub mod bookmarks;
pub mod chrome_profile;
pub mod history;
mod json;

pub use bookmarks::{import_bookmarks, Bookmark};
pub use history::{import_history, HistoryEntry};
