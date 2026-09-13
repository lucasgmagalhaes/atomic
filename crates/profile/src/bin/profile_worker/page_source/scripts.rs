//! `load_scripts` — split out from `page_source.rs`.

use dom::{Dom, NodeData, NodeId};

use crate::network::ResourceCache;

use super::url::resolve_url;

/// One `<script>` element in document order - either its real inline text
/// content, or the real `src` URL an external one needs fetched before it
/// can run. `defer` is real per spec only for an external script (`src`
/// present) - a `defer`/`async` attribute on an inline `<script>` is
/// ignored (ignored here the same way: `Inline` carries no defer flag,
/// always runs in its document-order slot in the non-deferred group).
enum ScriptSource {
    Inline(String),
    External { src: String, defer: bool },
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
            match attributes.get("src") {
                Some(src) => out.push(ScriptSource::External {
                    src: src.clone(),
                    defer: attributes.contains_key("defer"),
                }),
                None => out.push(ScriptSource::Inline(dom.text_content(node))),
            }
        }
    }
    for &child in &n.children {
        collect_script_sources(dom, child, out);
    }
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
/// Scope cut: no `async`/module-type distinction. Real `async` ordering
/// (a script runs as soon as *its own* fetch completes, out of document
/// order relative to other scripts) needs concurrent, independently-
/// completing fetches - this worker fetches scripts one at a time on a
/// single thread, so there's no real race to reorder around; treating
/// `async` as a synchronous non-deferred script (this engine's existing
/// classic-script behavior) is the honest projection of that, not a
/// distinct code path. `type="module"` scripts are treated as plain
/// classic scripts (no ES module resolution exists yet - see
/// `ROADMAP.md` P2 item 19).
pub(crate) fn load_scripts(
    dom: &Dom,
    root: NodeId,
    base_url: Option<&str>,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
    cache: &mut ResourceCache,
) -> Vec<String> {
    let mut sources = Vec::new();
    collect_script_sources(dom, root, &mut sources);

    let fetch_text = |src: &str, cache: &mut ResourceCache| -> Option<String> {
        let url = resolve_url(base_url, src)?;
        let response = cache
            .fetch_cached(&url, storage_root, proxy, dns_server)
            .ok()?;
        String::from_utf8(response.body.to_vec()).ok()
    };

    let mut immediate = Vec::new();
    let mut deferred = Vec::new();
    for source in sources {
        match source {
            ScriptSource::Inline(text) => immediate.push(text),
            ScriptSource::External { src, defer } if defer => {
                if let Some(text) = fetch_text(&src, cache) {
                    deferred.push(text);
                }
            }
            ScriptSource::External { src, .. } => {
                if let Some(text) = fetch_text(&src, cache) {
                    immediate.push(text);
                }
            }
        }
    }
    immediate.extend(deferred);
    immediate
}
