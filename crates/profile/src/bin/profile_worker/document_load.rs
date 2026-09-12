//! Resolving what a page's source actually is (the built-in demo, or a
//! navigated URL) into a real, fetched [`LoadedDocument`] and then a fully
//! built [`crate::page::Page`] — the top of `RELOAD`/`NAVIGATE`'s own call
//! chain.

use js_runtime::Runtime;

use crate::network::fetch_with_cookies;
use crate::page::Page;

const DEMO_HTML: &str = r#"
<div id="container">
  <p>Atomic profile worker</p>
  <p>Rendering real HTML via html5ever.</p>
  <p id="counter">tick 0</p>
</div>
"#;

/// What the currently loaded page came from - re-resolved on `RELOAD`.
pub(crate) enum PageSource {
    Demo,
    Url(String),
}

fn escape_html_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn error_page_html(url: &str, message: &str) -> String {
    format!(
        r#"<div id="container"><p>Failed to load {}</p><p>{}</p></div>"#,
        escape_html_text(url),
        escape_html_text(message)
    )
}

/// What one real page load delivered: the HTML source itself plus every
/// `Content-Security-Policy` policy that came with it (one entry per
/// repeated response header, in wire order — kept separate, not joined,
/// since real CSP policies intersect rather than merge; see
/// `crate::page_source::collect_meta_csp_policies` for where `<meta
/// http-equiv>` delivery adds to this after parsing). The demo page has no
/// response headers, so its list is always empty.
pub(crate) struct LoadedDocument {
    pub(crate) html: String,
    pub(crate) csp_policies: Vec<String>,
}

/// Resolves `source` to a real [`LoadedDocument`]: the built-in demo
/// string, or a real `net::get` response decoded as UTF-8 (lossily - this
/// doesn't attempt charset detection from `Content-Type`/`<meta charset>`,
/// real HTML often isn't UTF-8 but plenty is, and this crate doesn't have
/// a non-UTF-8 text decoder yet) *plus* the response's own
/// `Content-Security-Policy` headers — extracted here because this is the
/// only place that still holds the raw response; `Page::load` only ever
/// saw the body string.
fn resolve_document(
    source: &PageSource,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
) -> Result<LoadedDocument, String> {
    match source {
        PageSource::Demo => Ok(LoadedDocument {
            html: DEMO_HTML.to_string(),
            csp_policies: Vec::new(),
        }),
        PageSource::Url(url) => fetch_with_cookies(url, storage_root, proxy, dns_server)
            .map(|response| LoadedDocument {
                html: String::from_utf8_lossy(&response.body).into_owned(),
                csp_policies: response
                    .headers
                    .iter()
                    .filter(|(name, _)| name.eq_ignore_ascii_case("content-security-policy"))
                    .map(|(_, value)| value.clone())
                    .filter(|value| !value.is_empty())
                    .collect(),
            })
            .map_err(|e| e.to_string()),
    }
}

/// Loads `source`, returning the built page and `Some(error)` if the
/// underlying fetch failed (the returned page is then the rendered error
/// page, not a panic/empty page - callers still get something to publish
/// either way).
/// The demo page's real storage host - not a URL, so not derived via
/// `url::Url::parse` like a navigated page's host is; a fixed name is
/// enough for it to get real per-origin `localStorage`/`document.cookie`
/// (see `crate::page::DEMO_SCRIPT`).
const DEMO_STORAGE_HOST: &str = "demo.internal";

pub(crate) fn load_source<'rt>(
    runtime: &'rt Runtime,
    source: &PageSource,
    viewport_width: f64,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
) -> (Page<'rt>, Option<String>) {
    let base_url = match source {
        PageSource::Demo => None,
        PageSource::Url(url) => Some(url.as_str()),
    };
    let storage_host = match source {
        PageSource::Demo => Some(DEMO_STORAGE_HOST.to_string()),
        PageSource::Url(url) => url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string)),
    };
    match resolve_document(source, storage_root, proxy, dns_server) {
        Ok(doc) => {
            let run_demo_script = matches!(source, PageSource::Demo);
            (
                Page::load(
                    runtime,
                    &doc,
                    run_demo_script,
                    base_url,
                    storage_host.as_deref(),
                    viewport_width,
                    storage_root,
                    proxy,
                    dns_server,
                ),
                None,
            )
        }
        Err(message) => {
            let url = match source {
                PageSource::Demo => "the demo page",
                PageSource::Url(url) => url,
            };
            // The synthetic error page has no `<link>`s of its own to
            // resolve - `base_url: None` here, not the failed `url`. It
            // still gets real storage scoped to the same host though, so
            // a subsequent successful load/reload of that host sees
            // consistent state. No CSP either - a failed fetch delivered
            // no policy (an empty `LoadedDocument.csp_policies`, same as
            // the demo page).
            (
                Page::load(
                    runtime,
                    &LoadedDocument {
                        html: error_page_html(url, &message),
                        csp_policies: Vec::new(),
                    },
                    false,
                    None,
                    storage_host.as_deref(),
                    viewport_width,
                    storage_root,
                    proxy,
                    dns_server,
                ),
                Some(message),
            )
        }
    }
}
