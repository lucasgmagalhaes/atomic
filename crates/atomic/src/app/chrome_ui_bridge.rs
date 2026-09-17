//! Dispatches real `atomic::chrome_ui::ChromeAction`s (the chrome page's
//! own `dispatch(action)` calls, drained via `ChromeUi::drain_actions`)
//! into real `AtomicApp` methods — the actual "wire the bridge to real
//! `AtomicApp` methods" item `spec/ROADMAP.md` P5 calls for.

use atomic::chrome_ui::ChromeAction;

use super::AtomicApp;

impl AtomicApp {
    pub(super) fn handle_chrome_action(&mut self, action: ChromeAction) {
        match action {
            ChromeAction::SelectAccount { index } => {
                // The chrome page's account list is indexed into its own
                // (still fake, see chrome_ui's own doc) ACC array, not
                // real live panes yet - clamp against the real, currently
                // visible pane list so this can't index out of bounds
                // once real data replaces ACC.
                let visible = self.active_workspace_pane_indices();
                if let Some(&pane_index) = visible.get(index) {
                    self.selected = pane_index;
                }
            }
            ChromeAction::OpenAddProfile => {
                self.add_profile_form = Some(super::AddProfileForm::default());
            }
            ChromeAction::CreateProfile => {
                self.create_profile();
            }
            ChromeAction::SwitchWorkspace { id } => {
                // The chrome page's workspace list (WS) is still fake
                // data (see chrome_ui's own doc) with string ids
                // ("main"/"farm"/...) that don't correspond to this app's
                // real, index-addressed WorkspaceManager - there's
                // usually only one real workspace at this point in the
                // integration. A real mapping needs the chrome page to
                // actually receive the real workspace list first (a
                // follow-up, see spec/ROADMAP.md P5); until then this is
                // a real, visible no-op rather than a silent crash or a
                // fabricated mapping.
                eprintln!(
                    "chrome_ui: switchWorkspace({id:?}) has no real target yet - workspace list isn't wired to real data"
                );
            }
            ChromeAction::Unhandled { action, raw } => {
                eprintln!("chrome_ui: unhandled action {action:?} (raw: {raw})");
            }
        }
    }
}
