//! The app's own shell UI (toolbar/sidebar/screens), rendered by a
//! dedicated CEF browser instead of `egui` — see
//! `spec/architecture/chrome-ui.md` for the full design and
//! `crates/atomic/chrome-ui/index.html` for the real Preact bundle this
//! spawns. Mirrors `browser_view::BrowserView`'s own shape deliberately
//! (`spawn`/`poll_texture`/`error`) since it wraps the exact same
//! `profile::Profile` primitive — a "chrome browser" is just another
//! `cef_profile_worker` process, not a special case (see the design
//! doc's "No new binary needed" finding).
use std::cell::RefCell;
use std::rc::Rc;

use crate::browser_view::{rgba_to_color_image, worker_binary_path};

/// A real pane rect the chrome page's own CSS grid actually laid out —
/// queried from the live page (`getBoundingClientRect`), not computed by
/// hand in Rust from the CSS's own column/row fractions. Keeps the CSS in
/// `chrome-ui/index.html` as the single source of truth for pane layout;
/// Rust just asks "where did you actually put pane N" every frame,
/// exactly the technique `crates/cef/tests/chrome_toolbar_preact_test.rs`
/// already proved for finding a button to click.
#[derive(Debug, Clone, Copy)]
pub struct PaneRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One real action the chrome page's own `dispatch(action)` reported over
/// `console.log` — see `chrome-ui/index.html`'s own `dispatch` doc
/// comment. Only the actions with a real Rust-side effect are modeled
/// here; every other action the page dispatches (tab/section/language
/// switches, script selection, etc.) is genuinely UI-only state the page
/// already handles itself via its own `set(...)` call alongside the
/// dispatch — see that file's `onClick` handlers, which always call both.
#[derive(Debug, Clone, PartialEq)]
pub enum ChromeAction {
    SwitchWorkspace {
        id: String,
    },
    SelectAccount {
        index: usize,
    },
    OpenAddProfile,
    CreateProfile,
    /// A real action the page sends but this integration doesn't have a
    /// real backend behavior for yet (see this module's own doc on
    /// `SetGrid` below) — surfaced rather than silently dropped, so a
    /// caller can log/ignore it visibly instead of the action just
    /// vanishing.
    Unhandled {
        action: String,
        raw: String,
    },
}

fn parse_action(raw: &str) -> ChromeAction {
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => {
            return ChromeAction::Unhandled {
                action: String::new(),
                raw: raw.to_string(),
            }
        }
    };
    let action = value.get("action").and_then(|v| v.as_str()).unwrap_or("");
    match action {
        "switchWorkspace" => match value.get("id").and_then(|v| v.as_str()) {
            Some(id) => ChromeAction::SwitchWorkspace { id: id.to_string() },
            None => ChromeAction::Unhandled {
                action: action.to_string(),
                raw: raw.to_string(),
            },
        },
        "selectAccount" => match value.get("index").and_then(|v| v.as_u64()) {
            Some(index) => ChromeAction::SelectAccount {
                index: index as usize,
            },
            None => ChromeAction::Unhandled {
                action: action.to_string(),
                raw: raw.to_string(),
            },
        },
        "openAddProfile" => ChromeAction::OpenAddProfile,
        "createProfile" => ChromeAction::CreateProfile,
        // `setGrid` (the mockup's manual 1/2/4/6 tiling picker) has no
        // real backend counterpart yet: `grid_ui.rs`'s existing tiling is
        // automatic from the *actual* pane count
        // (`tiling::grid_layout(container, visible.len())`), not a
        // user-selectable override. Wiring this for real means deciding
        // what "grid: 1" should do when more than one pane actually
        // exists (hide the rest? force a layout that ignores real pane
        // count?) - a real product decision, not something to invent
        // silently here. Surfaced as `Unhandled`, not dropped.
        _ => ChromeAction::Unhandled {
            action: action.to_string(),
            raw: raw.to_string(),
        },
    }
}

pub struct ChromeUi {
    profile: Option<Rc<RefCell<profile::Profile>>>,
    texture: Option<egui::TextureHandle>,
    last_generation: u32,
    width: u32,
    height: u32,
    error: Option<String>,
}

impl ChromeUi {
    /// Spawns the chrome browser at `width` × `height`, navigated to the
    /// real `chrome-ui/index.html` bundle. Path resolution is dev-time
    /// only (relative to this crate's own source via `CARGO_MANIFEST_DIR`,
    /// same pattern `crates/cef`'s own tests already use) — packaging a
    /// real install needs to revisit this, same open item
    /// `worker_binary_path`'s own doc already flags for the worker binary
    /// itself.
    pub fn spawn(width: u32, height: u32) -> Self {
        let (profile, error) = match Self::spawn_inner(width, height) {
            Ok(p) => (Some(Rc::new(RefCell::new(p))), None),
            Err(e) => (None, Some(e)),
        };
        ChromeUi {
            profile,
            texture: None,
            last_generation: 0,
            width,
            height,
            error,
        }
    }

    fn spawn_inner(width: u32, height: u32) -> Result<profile::Profile, String> {
        let worker_path = worker_binary_path()?;
        let bundle_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("chrome-ui/index.html")
            .canonicalize()
            .map_err(|e| format!("chrome-ui/index.html not found: {e}"))?;
        let bundle_path_str = bundle_path.to_string_lossy();
        let bundle_path_str = bundle_path_str
            .strip_prefix(r"\\?\")
            .unwrap_or(&bundle_path_str);
        let url = format!("file:///{}", bundle_path_str.replace('\\', "/"));

        let mut profile = profile::Profile::spawn(
            &worker_path.to_string_lossy(),
            "atomic-chrome-ui",
            width,
            height,
        )
        .map_err(|e| e.to_string())?;
        profile
            .navigate(&url)
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(profile)
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn poll_texture(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if let Some(profile) = &self.profile {
            let profile = profile.borrow();
            let generation = profile.frame_generation();
            if generation != self.last_generation {
                if let Some(pixels) = profile.latest_frame() {
                    let image = rgba_to_color_image(&pixels, self.width, self.height);
                    let handle =
                        ctx.load_texture("atomic-chrome-ui", image, egui::TextureOptions::LINEAR);
                    self.texture = Some(handle);
                    self.last_generation = generation;
                }
            }
        }
        self.texture.as_ref()
    }

    /// Pushes a full/partial state replacement into the page via
    /// `window.__atomicBridge.setState(...)` — the same `Runtime.evaluate`
    /// primitive `chrome_bridge_test.rs`/`chrome_toolbar_preact_test.rs`
    /// already proved. `json` must be a valid JS object literal (or JSON,
    /// which is a subset).
    pub fn push_state(&mut self, json: &str) {
        if let Some(profile) = &self.profile {
            let _ = profile
                .borrow_mut()
                .evaluate(&format!("window.__atomicBridge.setState({json})"));
        }
    }

    /// Queries the page for every `.pane[data-pane-index]`'s real,
    /// currently-rendered rect — one `EVAL` round trip for all of them,
    /// not one per pane. Empty if the page hasn't rendered any panes yet
    /// (e.g. still on a non-grid screen) or the query itself fails.
    pub fn pane_rects(&mut self) -> Vec<PaneRect> {
        let Some(profile) = &self.profile else {
            return Vec::new();
        };
        let script = "JSON.stringify([...document.querySelectorAll('[data-pane-index]')].map(function(el){var r=el.getBoundingClientRect();return {x:r.left,y:r.top,width:r.width,height:r.height};}))";
        let Ok(Ok(result)) = profile.borrow_mut().evaluate(script) else {
            return Vec::new();
        };
        let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(&result) else {
            return Vec::new();
        };
        values
            .into_iter()
            .filter_map(|v| {
                Some(PaneRect {
                    x: v.get("x")?.as_f64()? as f32,
                    y: v.get("y")?.as_f64()? as f32,
                    width: v.get("width")?.as_f64()? as f32,
                    height: v.get("height")?.as_f64()? as f32,
                })
            })
            .collect()
    }

    /// Drains the page's `dispatch(action)` calls (see `chrome-ui/index.html`'s
    /// own doc comment) and parses each into a [`ChromeAction`] — the
    /// same `CONSOLE`-draining primitive `chrome_bridge_test.rs` already
    /// proved, just parsed into a real enum here instead of asserted
    /// against directly.
    pub fn drain_actions(&mut self) -> Vec<ChromeAction> {
        let Some(profile) = &self.profile else {
            return Vec::new();
        };
        let Ok(messages) = profile.borrow_mut().console() else {
            return Vec::new();
        };
        messages
            .into_iter()
            .map(|(_, text)| parse_action(&text))
            .collect()
    }

    pub fn click_at(&mut self, x: f64, y: f64) {
        if let Some(profile) = &self.profile {
            let _ = profile.borrow_mut().click_at(x, y);
        }
    }

    pub fn scroll_by(&mut self, dy: f64) {
        if let Some(profile) = &self.profile {
            let _ = profile.borrow_mut().scroll_by(dy);
        }
    }

    pub fn type_key(&mut self, key: &str) {
        if let Some(profile) = &self.profile {
            let _ = profile.borrow_mut().type_key(key);
        }
    }
}
