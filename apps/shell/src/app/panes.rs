//! Pane lifecycle: spawn/close/duplicate, pane-count changes, per-pane
//! proxy/FPS/GPU application, address bar navigation, and the per-pane
//! download box.

use shell::browser_view::BrowserView;
use shell::resource_monitor::PaneMonitor;

use super::pane::{spawn_pane, spawn_pane_with_proxy};
use super::{AddProfileForm, NimbleApp, PANE_HEIGHT, PANE_WIDTH};

impl NimbleApp {
    /// Indices into `self.panes` whose id is registered in the *active*
    /// workspace - what the grid actually draws, and what the "Panes:"
    /// count buttons grow/shrink. A pane belonging to a different
    /// workspace keeps running (its process isn't touched) but isn't
    /// visible while that other workspace isn't active - switching
    /// `WorkspaceManager`'s active index is what changes this list, not a
    /// respawn.
    pub(super) fn active_workspace_pane_indices(&self) -> Vec<usize> {
        let active_ids = self.workspace.active().profiles();
        self.panes
            .iter()
            .enumerate()
            .filter(|(_, p)| active_ids.iter().any(|id| id == &p.id))
            .map(|(i, _)| i)
            .collect()
    }

    /// The next `"pane-N"` id not already held by a *currently-live* pane
    /// (closed panes free their number back up) - unlike the old
    /// positional `panes.len() + 1` scheme, this can't collide with a pane
    /// that's still running after an earlier close shrank the vec (e.g.
    /// closing pane-2 out of pane-1/2/3 then spawning would otherwise
    /// reassign "pane-3", already in use). Collision now matters for real:
    /// [`BrowserView::spawn_with_identity`] derives the shared-memory name
    /// straight from the id.
    pub(super) fn next_pane_id(&self) -> String {
        let next = self
            .panes
            .iter()
            .filter_map(|p| {
                p.id.strip_prefix("pane-")
                    .and_then(|n| n.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1;
        format!("pane-{next}")
    }

    /// The "+ Add Profile" toolbar button - opens the modal with a fresh
    /// `AddProfileForm`, pre-filling `name` with [`next_pane_id`](Self::next_pane_id)
    /// so a user who doesn't care about naming can just hit Create.
    pub(super) fn open_add_profile_modal(&mut self) {
        self.add_profile_error = None;
        self.add_profile_form = Some(AddProfileForm {
            name: self.next_pane_id(),
            ..Default::default()
        });
    }

    /// The mockup's "Add profile modal" Create button - a real, named
    /// profile spawned with everything the form specified applied up
    /// front (not a generic pane later reconfigured): the start URL is
    /// navigated to immediately, email/password (if either is non-empty)
    /// go into the real credential vault under `profile:<id>#email`/
    /// `#password`, and the proxy (if set) is applied at spawn via
    /// [`spawn_pane_with_proxy`] rather than the toolbar's spawn-then-
    /// respawn `apply_proxy` dance. Rejects a blank or already-used name
    /// (see [`spawn_pane_with_proxy`]'s own doc on why a duplicate id is a
    /// real hazard, not just a UX nicety) rather than silently colliding
    /// storage with an existing pane. Leaves the modal open on failure (so
    /// the user's typed values aren't lost) and closes it on success.
    pub(super) fn create_profile(&mut self) {
        let Some(form) = &self.add_profile_form else {
            return;
        };
        let name = form.name.trim().to_string();
        if name.is_empty() {
            self.add_profile_error = Some("Name is required.".to_string());
            return;
        }
        if self.panes.iter().any(|p| p.id == name) {
            self.add_profile_error = Some(format!("A pane named \"{name}\" already exists."));
            return;
        }

        let proxy = if form.proxy.trim().is_empty() {
            None
        } else {
            Some(form.proxy.trim())
        };
        let start_url = form.start_url.trim().to_string();
        let email = form.email.trim().to_string();
        let password = form.password.clone();

        let mut pane = spawn_pane_with_proxy(&mut self.workspace, name.clone(), proxy);
        if !start_url.is_empty() {
            pane.browser.navigate(&start_url);
            pane.history.record(start_url);
        }
        self.panes.push(pane);
        self.apply_fps_cap_to_all_panes();
        self.apply_visibility_throttling();

        if !email.is_empty() || !password.is_empty() {
            self.ensure_vault_open();
            if let Some(vault) = self.vault.as_mut() {
                let _ = vault.set(&format!("profile:{name}#email"), &email);
                let _ = vault.set(&format!("profile:{name}#password"), &password);
            }
        }

        self.add_profile_form = None;
        self.add_profile_error = None;
    }

    /// Grows or shrinks the *active workspace's visible* pane count to
    /// exactly `count` (the mockup's 1/2/4/6 grid toggle - see
    /// `tiling::grid_layout`'s doc for why any other count still works).
    /// A newly grown pane is a genuinely new process, appended to
    /// `self.panes` and registered into the active workspace (same as
    /// [`spawn_pane`]). Shrinking closes the active workspace's own
    /// *trailing* panes (via [`close_pane`](Self::close_pane), largest
    /// index first so removal doesn't shift indices still to be closed
    /// out from under this loop) - panes belonging to other workspaces
    /// are never touched by this, regardless of count.
    pub(super) fn set_pane_count(&mut self, count: usize) {
        let count = self.performance.clamp_pane_count(count);
        let visible = self.active_workspace_pane_indices();
        if visible.len() < count {
            for _ in visible.len()..count {
                let id = self.next_pane_id();
                self.panes.push(spawn_pane(&mut self.workspace, id));
            }
            self.apply_fps_cap_to_all_panes();
        } else if visible.len() > count {
            let mut to_close: Vec<usize> = visible[count..].to_vec();
            to_close.sort_unstable_by(|a, b| b.cmp(a)); // descending
            for index in to_close {
                self.close_pane(index);
            }
        }
    }

    /// Applies `self.performance.fps_cap` (if set) to every live pane's
    /// real worker process via `profile::Profile::set_fps_cap` - called
    /// whenever the cap changes and after spawning any new pane, so a
    /// newly grown/duplicated pane picks up whatever cap is already
    /// active instead of silently running uncapped. `fps_cap: None` is a
    /// no-op (an already-running worker keeps whatever cap it last had -
    /// there's no `profile-worker` command to explicitly clear one back to
    /// its default).
    pub(super) fn apply_fps_cap_to_all_panes(&mut self) {
        let Some(fps) = self.performance.fps_cap else {
            return;
        };
        for pane in &self.panes {
            if let Some(profile) = pane.browser.profile_handle() {
                match profile.borrow_mut().set_fps_cap(fps) {
                    Ok(Ok(())) => self.performance_error = None,
                    Ok(Err(message)) => self.performance_error = Some(message),
                    Err(io_error) => self.performance_error = Some(io_error.to_string()),
                }
            }
        }
    }

    /// The Settings window's GPU dropdown "Apply" button: respawns every
    /// live pane with `self.performance.gpu_adapter` (see that field's own
    /// doc on why this can't be a live setting like `fps_cap`) - loses
    /// each pane's current page/proxy the same way `apply_proxy` already
    /// does for a single pane, since a respawn is a genuinely fresh
    /// process either way.
    pub(super) fn apply_gpu_adapter_to_all_panes(&mut self) {
        let adapter = self.performance.gpu_adapter;
        for pane in &mut self.panes {
            pane.browser = BrowserView::spawn_with_identity_and_gpu(
                &pane.id,
                PANE_WIDTH,
                PANE_HEIGHT,
                None,
                adapter,
            );
            pane.monitor = PaneMonitor::new();
        }
        self.apply_visibility_throttling();
        self.apply_fps_cap_to_all_panes();
    }

    pub(super) fn navigate_to_address_bar(&mut self) {
        let url = self.address_bar_text.trim().to_string();
        if !url.is_empty() {
            let pane = &mut self.panes[self.selected];
            pane.browser.navigate(&url);
            pane.history.record(url);
        }
    }

    /// Respawns *the selected pane* with the proxy text box's current
    /// contents - an empty box means "no proxy". A running profile's own
    /// proxy is fixed for its process lifetime (matches `profile-worker`'s
    /// "set once at spawn" scope), so applying a change here is a real
    /// respawn of that one pane, not a live setting - its address bar and
    /// current page are lost, same as before this pane count existed.
    pub(super) fn apply_proxy(&mut self) {
        let proxy = self.proxy_text.trim();
        let proxy = if proxy.is_empty() { None } else { Some(proxy) };
        let id = self.panes[self.selected].id.clone();
        self.panes[self.selected].browser =
            BrowserView::spawn_with_identity(&id, PANE_WIDTH, PANE_HEIGHT, proxy);
        // A respawn is a new OS process (see this method's own doc) - a
        // stale monitor would diff the new process's first sample against
        // the old process's last one, reading a nonsense CPU/FPS spike.
        self.panes[self.selected].monitor = PaneMonitor::new();
        self.address_bar_text.clear();
    }

    /// Downloads `self.download_url_text` (real `net::download`, see
    /// `downloads::Downloads`) into the *selected* pane's own download
    /// directory, recording a real entry - success or failure - and
    /// clears the text box on submit either way (the failed attempt is
    /// still visible in the list below, no need to keep the URL sitting
    /// in the box).
    pub(super) fn download_to_selected_pane(&mut self) {
        let url = self.download_url_text.trim().to_string();
        if url.is_empty() {
            return;
        }
        self.panes[self.selected].downloads.download(&url);
        self.download_url_text.clear();
    }

    /// Removes exactly `self.panes[index]` (not just the trailing pane -
    /// unlike [`set_pane_count`](Self::set_pane_count)'s grow/shrink,
    /// which only ever pops the end). Force-kills that pane's process the
    /// same way any other pane removal does (`Pane`'s `BrowserView` ->
    /// `profile::Profile`'s own real `Drop`). Refuses to remove the last
    /// remaining pane - a shell with zero panes has nothing to show and no
    /// way to add one back without a "new pane" control this pass doesn't
    /// add (mirrors `WorkspaceManager::remove`'s own "never zero" refusal).
    pub(super) fn close_pane(&mut self, index: usize) {
        if self.panes.len() <= 1 {
            return;
        }
        let pane = self.panes.remove(index);
        let active = self.workspace.active_index();
        self.workspace.remove_profile(active, &pane.id);
        if self.selected >= self.panes.len() {
            self.selected = self.panes.len() - 1;
        } else if self.selected > index {
            self.selected -= 1;
        }
    }

    /// Spawns a fresh pane and navigates it to `self.panes[index]`'s
    /// current URL - "duplicate" in the sense of "another pane on the same
    /// page", not a real session clone: each profile gets its own
    /// `storage_root` keyed by a fresh shmem name (see `profile-worker`'s
    /// own doc), so cookies/`localStorage`/login state are NOT copied.
    /// Real account cloning would need to duplicate that storage
    /// directory, which this doesn't do - documented here rather than
    /// silently producing a pane that looks duplicated but isn't logged
    /// in.
    pub(super) fn duplicate_pane(&mut self, index: usize) {
        let url = self.panes[index].browser.current_url().to_string();
        let id = self.next_pane_id();
        let mut pane = spawn_pane(&mut self.workspace, id);
        if !url.is_empty() {
            pane.browser.navigate(&url);
            pane.history.record(url);
        }
        self.panes.push(pane);
        self.apply_fps_cap_to_all_panes();
    }
}
