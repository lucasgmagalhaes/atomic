//! Spike: one piece of shell chrome (the toolbar) rendered by Atomic's own
//! engine instead of egui — proves the Track B architecture (in-process
//! engine instance, native JS bridge, GPU-buffer compositing) end to end
//! before the rest of the shell's chrome migrates the same way. Deliberately
//! self-contained rather than reusing `profile::bin::profile_worker::Page`
//! (which is private to that binary crate) — this mirrors its load/layout/
//! render/hit-test shape using the same public `html`/`css`/`layout-engine`/
//! `render`/`js-runtime` crates, minus what a static local chrome bundle
//! never needs (navigation, images, CSP, cookies/storage).
//!
//! Split into `helpers.rs` (`html_escape`/`js_string_literal`),
//! `render_hit_test.rs` (hover/render/hit-testing/click dispatch),
//! `input.rs` (focus/typing/input value reads), and `sync.rs` (the
//! `sync_*`/`set_*` DOM-mutation methods) — this file keeps the struct
//! definitions, constructors, `layout`, and `Drop`.

use std::cell::RefCell;

use css::{parse_stylesheet, Stylesheet};
use dom::NodeId;
use js_runtime::{Context, Runtime};
use layout_engine::{build_box_tree_with_viewport, layout_block, LayoutBox};

use crate::chrome_bridge;

mod helpers;
mod input;
mod render_hit_test;
mod sync;

/// One loaded chrome surface (e.g. the toolbar): its DOM root, stylesheet,
/// and the JS context driving it, plus a cached box tree so `render` and
/// `hit_test` never disagree — same reasoning `Page::layout`'s cache
/// documents for a real page.
pub(crate) struct ChromeEngine<'rt> {
    ctx: Context<'rt>,
    root: NodeId,
    sheet: Stylesheet,
    layout_cache: RefCell<Option<LayoutCache>>,
    /// The `id` of whichever `<input>`/`<textarea>` a click most recently
    /// landed on — set by [`click_at_with_focus`](Self::click_at_with_focus),
    /// consumed by [`type_key`](Self::type_key). `None` after a click on
    /// anything non-input-like, same "clicking away blurs" behavior
    /// `profile_worker::input_commands::dispatch_click_at` documents (minus
    /// this spike's simplification of not also dispatching real `blur`/
    /// `change` events — nothing in the bundles built on this yet depends
    /// on either).
    focused_input: RefCell<Option<String>>,
}

struct LayoutCache {
    width: u32,
    layout_ver: u64,
    /// `dom::Dom::style_version()` at the time this tree was built — a
    /// hover/focus change bumps this without bumping `layout_ver` (see
    /// that field's own doc), so keying the cache on it too is what makes
    /// `update_hover`'s DOM writes actually visible in the next `render`
    /// instead of silently serving a stale cached tree forever.
    style_ver: u64,
    tree: LayoutBox,
}

impl<'rt> ChromeEngine<'rt> {
    /// The toolbar spike's bundle — `apps/shell/chrome/toolbar.{html,css,js}`.
    pub(crate) fn new_toolbar(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../../chrome/toolbar.html"),
            include_str!("../../chrome/toolbar.css"),
            include_str!("../../chrome/toolbar.js"),
        )
    }

    /// The Settings window's bundle —
    /// `apps/shell/chrome/settings.{html,css,js}`.
    pub(crate) fn new_settings(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../../chrome/settings.html"),
            include_str!("../../chrome/settings.css"),
            include_str!("../../chrome/settings.js"),
        )
    }

    /// The downloads/history panel's bundle —
    /// `apps/shell/chrome/downloads_history.{html,css,js}`.
    pub(crate) fn new_downloads_history(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../../chrome/downloads_history.html"),
            include_str!("../../chrome/downloads_history.css"),
            include_str!("../../chrome/downloads_history.js"),
        )
    }

    /// Loads a fixed local HTML/CSS/JS bundle (no navigation, no network —
    /// chrome is not a tab). Registers `globalThis.atomic.*`
    /// (`chrome_bridge::register`) before running `js`, so the bundle's own
    /// script can wire button handlers immediately.
    fn new(runtime: &'rt Runtime, html: &str, css_text: &str, js: &str) -> Self {
        let (dom, root) = html::parse_to_html_element(html);
        let sheet = parse_stylesheet(css_text);
        let ctx = Context::with_dom(runtime, dom);
        unsafe {
            chrome_bridge::register(ctx.as_raw());
        }
        ctx.eval(js, "chrome-toolbar.js")
            .expect("chrome bundle script is static and must not fail to eval");
        ctx.dispatch_lifecycle_events();
        ChromeEngine {
            ctx,
            root,
            sheet,
            layout_cache: RefCell::new(None),
            focused_input: RefCell::new(None),
        }
    }

    /// The Add Profile modal's bundle —
    /// `apps/shell/chrome/add_profile.{html,css,js}`.
    pub(crate) fn new_add_profile(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../../chrome/add_profile.html"),
            include_str!("../../chrome/add_profile.css"),
            include_str!("../../chrome/add_profile.js"),
        )
    }

    /// Builds/lays out this chrome surface's box tree against `width`,
    /// reusing the cached tree only when both `width` and the DOM's own
    /// layout version match the cached entry — same cache key `Page::layout`
    /// uses (minus `adopted_version`; chrome has no `adoptedStyleSheets`
    /// today). Getting this check right matters more here than for a real
    /// page: `sync_toolbar_state`/`sync_downloads_history`'s `innerHTML`/
    /// `className` mutations are this chrome surface's *only* way to
    /// reflect changed `AtomicApp` state, so a cache that never
    /// re-validates against `layout_ver` would render the very first frame
    /// forever regardless of what the sync calls do.
    fn layout(&self, width: u32) -> LayoutBox {
        let dom = self
            .ctx
            .dom()
            .expect("ChromeEngine always builds its context over a dom");
        let layout_ver = dom.layout_version();
        let style_ver = dom.style_version();
        if let Some(cached) = self.layout_cache.borrow().as_ref() {
            if cached.width == width
                && cached.layout_ver == layout_ver
                && cached.style_ver == style_ver
            {
                return cached.tree.clone();
            }
        }
        // `layout_engine::DEFAULT_VIEWPORT_HEIGHT`, not this surface's own
        // fixed `CHROME_HEIGHT` constant: `ChromeEngine::layout` only
        // takes `width` (see this method's own cache key) - no chrome
        // bundle uses a height-based `@media` rule today, so plumbing the
        // real per-surface height through here has no observable effect
        // yet and isn't worth the ripple until one does.
        let mut tree = build_box_tree_with_viewport(
            dom,
            self.root,
            &self.sheet,
            width as f64,
            layout_engine::DEFAULT_VIEWPORT_HEIGHT,
        )
        .expect("chrome bundle's HTML always produces a box tree");
        layout_block(&mut tree, width as f64, 0.0, 0.0);
        *self.layout_cache.borrow_mut() = Some(LayoutCache {
            width,
            layout_ver,
            style_ver,
            tree: tree.clone(),
        });
        tree
    }
}

impl<'rt> Drop for ChromeEngine<'rt> {
    /// Evicts this surface's entry from `chrome_bridge`'s thread-local
    /// registry (keyed by raw `*mut JSContext` as `usize`) before the
    /// `Context` itself drops — same reasoning `class_registry.rs` documents
    /// for why `Runtime::drop` must evict its own registry entries first:
    /// without this, a later `Context` allocated at the same freed address
    /// could inherit a leftover queued action from this one. Currently
    /// unreachable in practice (`AtomicApp` leaks every chrome `Runtime` via
    /// `Box::leak`, so no `ChromeEngine` is ever actually dropped), but the
    /// bridge shouldn't rely on that staying true.
    fn drop(&mut self) {
        let _ = chrome_bridge::drain_actions(self.ctx.as_raw());
    }
}
