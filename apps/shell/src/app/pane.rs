use shell::browser_view::BrowserView;
use shell::downloads::Downloads;
use shell::history::History;
use shell::resource_monitor::PaneMonitor;
use shell::workspace::WorkspaceManager;

use super::{PANE_HEIGHT, PANE_WIDTH};

/// One live grid cell: a stable id (registered into the active workspace
/// so `automation_bridge::run_script` can address it via `pane("id")`,
/// and so `BrowserView::spawn_with_identity` gives it real persistent
/// storage across shell restarts - see that method's own doc) plus the
/// `BrowserView` actually rendering it. The grid's own "Panes: 1/2/4/6"
/// buttons assign positional ids (`pane-1`, `pane-2`, ...) via
/// `next_pane_id`; the "+ Add Profile" modal (`NimbleApp::create_profile`)
/// lets a user pick a real name instead.
pub struct Pane {
  pub id: String,
  pub browser: BrowserView,
  /// Real CPU/RAM/FPS telemetry for this pane's process - see
  /// `resource_monitor`'s own doc. Lives on `Pane` (not a separate
  /// `Vec` NimbleApp would have to keep in sync with `panes`) so it
  /// naturally follows spawn/close/duplicate without extra bookkeeping.
  pub monitor: PaneMonitor,
  /// Real, in-memory (see `history`'s own doc on why not persisted)
  /// browsing history - one entry per real navigation this pane's
  /// `BrowserView` was actually asked to perform.
  pub history: History,
  /// Real per-pane download list - see `downloads`'s own doc. Files
  /// land under `%TEMP%/nimble-downloads/<pane-id>/`.
  pub downloads: Downloads,
}

/// Spawns one pane with `id` and registers it into the active workspace -
/// see [`spawn_pane_with_proxy`] for the general form.
pub fn spawn_pane(workspace: &mut WorkspaceManager, id: String) -> Pane {
  spawn_pane_with_proxy(workspace, id, None)
}

/// Same as [`spawn_pane`], routing the new profile's fetches through
/// `proxy` from the moment it spawns (the "Add Profile" modal's own proxy
/// field - see `NimbleApp::create_profile`) instead of the two-step
/// spawn-then-`apply_proxy`-respawn dance the toolbar's plain proxy box
/// still uses. `id` must be unique among every currently-live pane (see
/// `NimbleApp::next_pane_id`) - since [`BrowserView::spawn_with_identity`]
/// derives the shared-memory name directly from it, two live panes with
/// the same id would fight over the same region and storage directory.
pub fn spawn_pane_with_proxy(
  workspace: &mut WorkspaceManager,
  id: String,
  proxy: Option<&str>,
) -> Pane {
  let active = workspace.active_index();
  workspace.add_profile(active, id.clone());
  let downloads_dir = std::env::temp_dir().join("nimble-downloads").join(&id);
  let history_path = std::env::temp_dir()
    .join("nimble-shell-history")
    .join(format!("{id}.txt"));
  let browser = BrowserView::spawn_with_identity(&id, PANE_WIDTH, PANE_HEIGHT, proxy);
  Pane {
    browser,
    monitor: PaneMonitor::new(),
    history: History::open(history_path),
    downloads: Downloads::new(downloads_dir),
    id,
  }
}
