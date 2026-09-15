//! `Page::load` — split out from `page.rs`.

use js_runtime::{Context, Runtime};

use crate::document_load::LoadedDocument;
use crate::network::ResourceCache;
use crate::page_source::{build_stylesheet, collect_meta_csp_policies, load_images, load_scripts};

use super::{Page, DEMO_SCRIPT};

impl<'rt> Page<'rt> {
    /// Builds a page from `doc` (the fetched HTML plus whatever CSP
    /// policies its response delivered - see [`LoadedDocument`]).
    /// `run_demo_script` should only be `true`
    /// for the built-in demo page - a real fetched page has no
    /// `#counter` element for `DEMO_SCRIPT` to find (which now uses real
    /// `localStorage`/`document.cookie` itself - see the const's doc).
    /// `document.cookie`/`localStorage`/`sessionStorage`/`indexedDB` are
    /// wired to real per-`storage_host` storage under `storage_root` (via
    /// `js_runtime::Context::with_storage`) whenever `storage_host` is
    /// `Some` - `None` gets a plain `Context::with_dom` instead. A
    /// storage-open failure (rare - a permissions problem, a full disk)
    /// degrades to `with_dom` rather than failing the whole page load.
    /// Every CSP policy `doc` carried (response headers) plus every
    /// `<meta http-equiv="Content-Security-Policy">` tag in the parsed
    /// HTML is enforced before the first script runs.
    pub(crate) fn load(
        runtime: &'rt Runtime,
        doc: &LoadedDocument,
        run_demo_script: bool,
        base_url: Option<&str>,
        storage_host: Option<&str>,
        viewport_width: f64,
        storage_root: &std::path::Path,
        proxy: Option<&net::ProxyConfig>,
        dns_server: Option<std::net::SocketAddr>,
    ) -> Self {
        let html = &doc.html;
        let (dom, html_el) = html::parse_to_html_element(html);
        // One cache for every resource this single load fetches (stylesheets,
        // their `@import`s, and images) - see `ResourceCache`'s own doc for
        // why it's scoped to just this call rather than living longer.
        let mut cache = ResourceCache::new();
        let sheet = build_stylesheet(
            &dom,
            html_el,
            base_url,
            viewport_width,
            storage_root,
            proxy,
            dns_server,
            &mut cache,
        );
        let images = load_images(
            &dom,
            html_el,
            base_url,
            storage_root,
            proxy,
            dns_server,
            &mut cache,
        );
        // Every CSP policy this page will ever have, known up front so
        // `load_scripts` can gate `script-src` on external `<script src>`
        // fetches before any of them happen - see `page_source::csp`'s own
        // doc on why this moved earlier than the `ctx.add_csp_policy` calls
        // below (which still exist, unchanged, to enforce `connect-src`/
        // Trusted Types for the page's own later fetches/DOM writes).
        let mut csp_policies = doc.csp_policies.clone();
        collect_meta_csp_policies(&dom, html_el, &mut csp_policies);
        let page_origin = base_url
            .and_then(|u| url::Url::parse(u).ok())
            .map(|u| u.origin().ascii_serialization());
        let scripts = load_scripts(
            &dom,
            html_el,
            base_url,
            storage_root,
            proxy,
            dns_server,
            &mut cache,
            &csp_policies,
            page_origin.as_deref(),
        );

        let mut ctx = match storage_host {
            Some(host) => {
                match Context::with_storage(runtime, dom, host, storage_root.join(host)) {
                    Ok(ctx) => ctx,
                    // `with_storage` already consumed `dom` by the time it can
                    // fail (a rare I/O error opening the storage files) - it
                    // has to be re-parsed from `html` rather than reused,
                    // acceptable for a path this unlikely to hit in practice.
                    Err(_) => Context::with_dom(runtime, html::parse_to_html_element(html).0),
                }
            }
            None => Context::with_dom(runtime, dom),
        };
        if let Some(url) = base_url {
            ctx.set_url(url);
        }
        // Real CSP delivery, before any script runs - matching a real
        // browser, where a page's policy is fully in effect by the time its
        // first script executes. Two delivery channels, enforced together:
        // the document response's own `Content-Security-Policy` headers
        // (extracted in `resolve_document`, repeated headers kept as
        // separate policies) and every `<meta http-equiv>` tag the parsed
        // DOM turned out to carry - both already collected into
        // `csp_policies` above (for `load_scripts`'s own `script-src`
        // gating). Each is appended via `add_csp_policy` rather than
        // joined into one string - real CSP policies intersect rather
        // than merge (see `js_runtime::csp`'s module docs).
        for policy in &csp_policies {
            ctx.add_csp_policy(policy);
        }
        if run_demo_script {
            let _ = ctx.eval(DEMO_SCRIPT, "<profile-worker demo>");
        }
        // Real page scripts, in real document order - a later script
        // failing (a real JS exception) doesn't stop earlier ones from
        // having already run, matching how a real browser keeps executing
        // a page after one `<script>` throws (each gets its own top-level
        // try - this engine still has no `window.onerror`/console to
        // report it to, so a failure is silent here, same "no error
        // surface for a page's own script" scope every other page-script
        // path in this worker already has).
        for script in &scripts {
            if script.is_module {
                // Real ES module execution (`ROADMAP.md` item 19): the
                // script's own resolved URL is its module's name, so its
                // own relative `import`/`import()` specifiers resolve
                // against it, not an arbitrary debug label.
                let _ = ctx.eval_module(&script.text, &script.url);
            } else {
                let _ = ctx.eval(&script.text, &script.url);
            }
        }
        // Real `DOMContentLoaded`/`load` lifecycle timing: fired once the
        // document is parsed and every page script has run, matching a
        // real browser's own ordering (see `Context::dispatch_lifecycle_events`'s
        // doc for the scope this crate cuts relative to the full spec).
        ctx.dispatch_lifecycle_events();
        Page {
            ctx,
            html_el,
            sheet,
            images,
            layout_cache: std::cell::RefCell::new(None),
            paint_cache: std::cell::RefCell::new(None),
            layers: std::cell::RefCell::new(std::collections::HashMap::new()),
            transitions: std::cell::RefCell::new(layout_engine::TransitionStates::new()),
            transitions_active: std::cell::Cell::new(false),
        }
    }
}
