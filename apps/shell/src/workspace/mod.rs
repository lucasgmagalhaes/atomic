//! Named groups of profiles (the mockup's left-rail workspace switcher:
//! "Principal", "Farm squad", ...). Pure data/logic, no GUI wiring yet —
//! `apps/shell`'s window is still the phase-1 stub. Holds profile *ids*
//! (plain strings), not live `profile::Profile` process handles — keeps
//! this testable without spawning real processes; whatever eventually
//! wires this to the GUI owns the id->Profile mapping separately.
pub type ProfileId = String;

#[derive(Debug, Clone)]
pub struct Workspace {
    pub name: String,
    profiles: Vec<ProfileId>,
}

impl Workspace {
    pub fn new(name: impl Into<String>) -> Self {
        Workspace {
            name: name.into(),
            profiles: Vec::new(),
        }
    }

    pub fn profiles(&self) -> &[ProfileId] {
        &self.profiles
    }
}

pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,
    active: usize,
}

impl WorkspaceManager {
    /// Starts with one default workspace, active — a shell should never
    /// have zero workspaces to show, mirroring the mockup always
    /// rendering at least "Principal".
    pub fn new() -> Self {
        WorkspaceManager {
            workspaces: vec![Workspace::new("Principal")],
            active: 0,
        }
    }

    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &Workspace {
        &self.workspaces[self.active]
    }

    /// `false` if `index` is out of range — the active workspace is left
    /// unchanged rather than panicking on a stale/bad index.
    pub fn set_active(&mut self, index: usize) -> bool {
        if index < self.workspaces.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    pub fn create(&mut self, name: impl Into<String>) -> usize {
        self.workspaces.push(Workspace::new(name));
        self.workspaces.len() - 1
    }

    /// Removes the workspace at `index`. Refuses to remove the last
    /// remaining workspace (mirrors `new()`'s "never zero workspaces"
    /// invariant) or an out-of-range index; returns whether it removed
    /// anything. If the active workspace shifted or was the one removed,
    /// clamps `active` back into range.
    pub fn remove(&mut self, index: usize) -> bool {
        if self.workspaces.len() <= 1 || index >= self.workspaces.len() {
            return false;
        }
        self.workspaces.remove(index);
        if self.active >= self.workspaces.len() {
            self.active = self.workspaces.len() - 1;
        } else if self.active > index {
            self.active -= 1;
        }
        true
    }

    /// Finds which workspace (if any) currently holds `profile_id`.
    pub fn find_profile(&self, profile_id: &str) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|w| w.profiles.iter().any(|p| p == profile_id))
    }

    /// Adds `profile_id` to the workspace at `index`. A no-op (returns
    /// `true`, not an error) if it's already there — matches "add
    /// account" being idempotent from a caller's perspective.
    pub fn add_profile(&mut self, index: usize, profile_id: impl Into<ProfileId>) -> bool {
        let Some(workspace) = self.workspaces.get_mut(index) else {
            return false;
        };
        let profile_id = profile_id.into();
        if !workspace.profiles.contains(&profile_id) {
            workspace.profiles.push(profile_id);
        }
        true
    }

    pub fn remove_profile(&mut self, index: usize, profile_id: &str) -> bool {
        let Some(workspace) = self.workspaces.get_mut(index) else {
            return false;
        };
        let before = workspace.profiles.len();
        workspace.profiles.retain(|p| p != profile_id);
        workspace.profiles.len() != before
    }

    /// Moves `profile_id` from wherever it currently is into the
    /// workspace at `to` — the mockup's pane context-menu "Move to
    /// workspace…" action. `false` if the profile isn't in any workspace
    /// or `to` is out of range.
    pub fn move_profile(&mut self, profile_id: &str, to: usize) -> bool {
        if to >= self.workspaces.len() {
            return false;
        }
        let Some(from) = self.find_profile(profile_id) else {
            return false;
        };
        if from == to {
            return true;
        }
        self.remove_profile(from, profile_id);
        self.add_profile(to, profile_id.to_string())
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}
