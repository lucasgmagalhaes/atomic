//! `load_scripts` — split out from `page_source.rs`.

use dom::{Dom, NodeData, NodeId};

use crate::network::ResourceCache;

use super::csp::is_script_allowed;
use super::url::resolve_url;

/// One `<script>` element in document order - either its real inline text
/// content, or the real `src` URL an external one needs fetched before it
/// can run. `defer` is real per spec only for an external script (`src`
/// present) - a `defer`/`async` attribute on an inline `<script>` is
/// ignored (ignored here the same way: `Inline` carries no defer flag,
/// always runs in its document-order slot in the non-deferred group).
/// `is_module` reflects a real `type="module"` attribute (`ROADMAP.md`
/// item 19) - previously read but discarded, treating every script as
/// classic; a caller now uses it to pick `Context::eval` vs
/// `Context::eval_module`.
enum ScriptSource {
    Inline {
        text: String,
        is_module: bool,
    },
    External {
        src: String,
        defer: bool,
        is_module: bool,
    },
}

/// Collects every `<script>` element's source in document order (matches
/// real script execution order - a script placed after the elements it
/// references, the common real pattern, sees them already in the DOM by
/// the time it runs).
fn collect_script_sources(dom: &Dom, node: NodeId, out: &mut Vec<ScriptSource>) {
    let Some(n) = dom.get(node) else { return };
    if let NodeData::Element {
        tag, attributes, ..
    } = &n.data
    {
        if tag == "script" {
            let is_module = attributes
                .get("type")
                .is_some_and(|t| t.eq_ignore_ascii_case("module"));
            match attributes.get("src") {
                Some(src) => out.push(ScriptSource::External {
                    src: src.clone(),
                    defer: attributes.contains_key("defer"),
                    is_module,
                }),
                None => out.push(ScriptSource::Inline {
                    text: dom.text_content(node),
                    is_module,
                }),
            }
        }
    }
    for &child in &n.children {
        collect_script_sources(dom, child, out);
    }
}

/// One script ready to run: its real source text, whether it's a real ES
/// module (`ROADMAP.md` item 19 - a caller must use `Context::eval_module`,
/// not `Context::eval`, for these), and the real URL its own relative
/// `import`/`import()` specifiers resolve against - the page's own
/// `base_url` for an inline module, or the external script's own resolved
/// `src` for a fetched one, matching real per-module base-URL semantics.
pub(crate) struct LoadedScript {
    pub(crate) text: String,
    pub(crate) is_module: bool,
    pub(crate) url: String,
}

/// Real `<script>` tag execution - previously only the hardcoded demo
/// page's own `DEMO_SCRIPT` ever ran (`run_demo_script`, see `Page::load`);
/// a genuinely fetched/navigated page's own inline `<script>` content was
/// parsed into the DOM (as a real text-content child, same as any other
/// element) but never evaluated, and a `<script src="...">` external
/// script wasn't fetched at all. Both are real now: inline text is used
/// verbatim, and an external script is fetched (same
/// `ResourceCache`/`resolve_url` pipeline `<link>`/`<img>` already use) and
/// its response body decoded as UTF-8 - a fetch failure (bad URL, network
/// error, non-UTF8 body) silently drops that one script and moves on,
/// same "one resource failing doesn't fail the whole page" convention
/// `load_images` already has for a broken `<img>`. Every source lands in
/// the returned `Vec` in real document order regardless of inline/external
/// mix, so multi-script execution order still matches a real browser's.
///
/// Real `defer` ordering: every non-deferred script (inline, or external
/// without `defer`) runs first, in document order - then every `defer`red
/// external script runs, also in its own document order. This engine
/// parses the whole document before running any script (no streaming
/// parser insertion-point timing to honor), so `defer`'s only observable
/// effect here is that ordering guarantee - a `<script defer>` placed
/// *before* a later plain `<script>` in markup must still run *after* it,
/// exactly like a real browser deferring it past the parse. Both groups
/// still run before `Context::dispatch_lifecycle_events`'s
/// `DOMContentLoaded`/`load`, matching spec (`defer` scripts run before
/// `DOMContentLoaded`, not after).
///
/// Scope cut: no `async` distinction. Real `async` ordering (a script
/// runs as soon as *its own* fetch completes, out of document order
/// relative to other scripts) needs concurrent, independently-completing
/// fetches - this worker fetches scripts one at a time on a single
/// thread, so there's no real race to reorder around; treating `async`
/// as a synchronous non-deferred script (this engine's existing
/// classic-script behavior) is the honest projection of that, not a
/// distinct code path. `type="module"` *is* real, though (this doc block
/// predates that landing - see this file's own `LoadedScript`/
/// `ScriptSource` doc above, and `page/load.rs`'s real
/// `Context::eval_module` call for `is_module` scripts): real
/// import/export linking, dynamic `import()`, circular-dependency
/// handling and a real module cache all exist (`module_loader.rs`,
/// `context/eval.rs`'s `eval_module`). Still `[ ]`: import maps (a bare
/// specifier with no scheme, e.g. `import "lodash"`, has no real
/// resolution mechanism yet - see `module_loader.rs`'s own scope-cut
/// doc).
pub(crate) fn load_scripts(
    dom: &Dom,
    root: NodeId,
    base_url: Option<&str>,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
    cache: &mut ResourceCache,
    csp_policies: &[String],
    page_origin: Option<&str>,
) -> Vec<LoadedScript> {
    let mut sources = Vec::new();
    collect_script_sources(dom, root, &mut sources);

    let fetch_text = |src: &str, cache: &mut ResourceCache| -> Option<(String, String)> {
        let url = resolve_url(base_url, src)?;
        // Real CSP `script-src` gating (`spec/matrix/runtime.md`'s own
        // former gap): every policy this page will ever have is already
        // known by the time this runs (see this module's own doc /
        // `csp.rs`'s `is_script_allowed` doc for why gating happens here
        // rather than inside `js-runtime`) - a disallowed external
        // script is silently dropped, same "one resource failing doesn't
        // fail the whole page" convention a fetch failure already has.
        if !is_script_allowed(csp_policies, &url, page_origin) {
            return None;
        }
        let response = cache
            .fetch_cached(&url, storage_root, proxy, dns_server)
            .ok()?;
        let text = String::from_utf8(response.body.to_vec()).ok()?;
        Some((text, url))
    };

    let mut immediate = Vec::new();
    let mut deferred = Vec::new();
    for source in sources {
        match source {
            ScriptSource::Inline { text, is_module } => immediate.push(LoadedScript {
                text,
                is_module,
                url: base_url.unwrap_or("<inline script>").to_string(),
            }),
            ScriptSource::External {
                src,
                defer,
                is_module,
            } if defer => {
                if let Some((text, url)) = fetch_text(&src, cache) {
                    deferred.push(LoadedScript {
                        text,
                        is_module,
                        url,
                    });
                }
            }
            ScriptSource::External { src, is_module, .. } => {
                if let Some((text, url)) = fetch_text(&src, cache) {
                    immediate.push(LoadedScript {
                        text,
                        is_module,
                        url,
                    });
                }
            }
        }
    }
    immediate.extend(deferred);
    immediate
}
