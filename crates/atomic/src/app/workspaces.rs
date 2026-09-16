//! Workspace switching/creation and moving a pane between workspaces, plus
//! background-throttling panes not in the active workspace.

use super::AtomicApp;

impl AtomicApp {
    /// Moves `self.panes[index]`'s id to the workspace at `workspace_index`
    /// via the real, already-tested `WorkspaceManager::move_profile`. The
    /// pane keeps running exactly as before - only which workspace lists
    /// its id changes, which in turn changes whether an automation script
    /// targeting the *other* workspace can still address it (see
    /// `automation_bridge::run_script`'s "only what the active workspace
    /// lists" scoping).
    pub(super) fn move_pane_to_workspace(&mut self, index: usize, workspace_index: usize) {
        self.workspace
            .move_profile(&self.panes[index].id, workspace_index);
        self.apply_visibility_throttling();
    }

    /// Creates a new workspace (named positionally, "Workspace 2", "Workspace
    /// 3", ...) and moves `self.panes[index]` into it in one step - the
    /// context menu's "Move to workspace... > New workspace" action. No
    /// text-input prompt for a custom name in this pass (egui has no
    /// blocking dialog primitive here) - renaming a workspace after
    /// creation isn't wired into this GUI at all yet.
    pub(super) fn move_pane_to_new_workspace(&mut self, index: usize) {
        let name = format!("Workspace {}", self.workspace.workspaces().len() + 1);
        let new_index = self.workspace.create(name);
        self.move_pane_to_workspace(index, new_index);
    }

    /// Switches which workspace is active - the grid now shows only
    /// `self.panes` whose id that workspace lists (see
    /// `active_workspace_pane_indices`'s doc), all other panes keep
    /// running unseen. Re-clamps `self.selected` immediately (not left for
    /// the next `CentralPanel` frame) since the toolbar's Reload/address
    /// bar/proxy controls run before the grid in `update`'s draw order and
    /// would otherwise act on a pane this frame no longer shows.
    pub(super) fn switch_workspace(&mut self, index: usize) {
        self.workspace.set_active(index);
        let visible = self.active_workspace_pane_indices();
        self.selected = visible.first().copied().unwrap_or(0);
        self.apply_visibility_throttling();
    }

    /// Real "background throttling" (mockup's Settings/Performance knob):
    /// pauses every pane not in the *active* workspace's own real vsync
    /// loop (`profile::Profile::pause` - see that method's doc) and
    /// resumes whichever ones are - a hidden pane's process keeps running
    /// (still killable/movable/inspectable) but stops burning CPU on
    /// timers/rendering nobody can see. Idempotent, so calling it whenever
    /// workspace membership might have changed is cheap and safe.
    pub(super) fn apply_visibility_throttling(&mut self) {
        let visible = self.active_workspace_pane_indices();
        for (index, pane) in self.panes.iter().enumerate() {
            let Some(profile) = pane.browser.profile_handle() else {
                continue;
            };
            let mut profile = profile.borrow_mut();
            if visible.contains(&index) {
                let _ = profile.resume();
            } else {
                let _ = profile.pause();
            }
        }
    }

    /// The toolbar's "+ New" workspace button: creates an empty workspace
    /// (positionally named, same as [`move_pane_to_new_workspace`](Self::move_pane_to_new_workspace))
    /// and switches to it - the grid shows nothing until the pane-count
    /// buttons spawn one (which registers into whichever workspace is now
    /// active) or a pane is moved in from elsewhere via its context menu.
    pub(super) fn create_workspace_and_switch(&mut self) {
        let name = format!("Workspace {}", self.workspace.workspaces().len() + 1);
        let new_index = self.workspace.create(name);
        self.switch_workspace(new_index);
    }
}
