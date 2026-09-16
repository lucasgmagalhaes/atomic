//! Network entry points every page-load path in this worker funnels through:
//! a real cookie-aware fetch, a per-page-load resource cache built on top
//! of it, and this process's own optional-CLI-argument proxy/DNS parsing.

use std::collections::HashMap;
use std::rc::Rc;

/// Fetches `url` with a `Cookie` header built from the jar at
/// `storage_root/<host>/cookies.txt` (the same file
/// `neutron::js::Context::with_storage` opens for `document.cookie` — see
/// that constructor's doc), then feeds every `Set-Cookie` response header
/// back into that same jar before returning. Because this runs *before*
/// `Page::load` opens its own `Context`, a `Set-Cookie` on the page's own
/// HTML response is already on disk by the time `document.cookie` reads
/// it — real browsers show the same same-navigation consistency. A jar
/// open/write failure (permissions, full disk) degrades to "send/store no
/// cookies" rather than failing the fetch — matches this file's existing
/// storage-failure fallback in `Page::load`.
///
/// `proxy`, when `Some`, routes the request through `net::get_via_proxy`
/// instead of connecting directly — every fetch this worker makes
/// (page HTML, `<link>` stylesheets, `@import`s) shares this one function,
/// so a profile's proxy choice applies to all of them uniformly, not just
/// the page's own HTML. `dns_server`, when `Some` and `proxy` is `None`,
/// resolves `url`'s host through that server instead of the OS resolver
/// via `net::get_via_dns` — closes the "DNS" half of the spec's Settings/
/// Network gap the same way `proxy` closed the proxy half. A proxy always
/// wins if both are set (there's no `net` entry point combining custom
/// DNS resolution with proxy tunneling — a proxied request's DNS
/// resolution is the proxy's own job, not this worker's).
pub(crate) fn fetch_with_cookies(
    url: &str,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
) -> Result<net::Response, net::Error> {
    let parsed = url::Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .map(str::to_string);
    let path = parsed
        .as_ref()
        .map(|u| u.path().to_string())
        .unwrap_or_else(|| "/".to_string());
    let secure = parsed
        .as_ref()
        .map(|u| u.scheme() == "https")
        .unwrap_or(false);

    let mut jar = host.as_deref().and_then(|h| {
        storage::cookies::CookieJar::open(storage_root.join(h).join("cookies.txt")).ok()
    });

    let cookie_header = jar
        .as_ref()
        .zip(host.as_deref())
        .and_then(|(jar, h)| jar.header_value(h, &path, secure));
    let extra_headers: Vec<(&str, &str)> = cookie_header
        .as_deref()
        .map(|v| vec![("Cookie", v)])
        .unwrap_or_default();

    let response = match (proxy, dns_server) {
        (Some(proxy), _) => net::get_via_proxy(url, &extra_headers, proxy)?,
        (None, Some(dns_server)) => net::get_via_dns(url, &extra_headers, dns_server)?,
        (None, None) => net::get_with_headers(url, &extra_headers)?,
    };

    if let (Some(jar), Some(host)) = (jar.as_mut(), host.as_deref()) {
        for (name, value) in &response.headers {
            if name.eq_ignore_ascii_case("set-cookie") {
                let _ = jar.set_from_header(value, host);
            }
        }
    }

    Ok(response)
}

/// A same-page-load resource cache — the "Cache" step of the shared
/// Resource Loader pipeline (`spec/architecture/primitives.md` §10:
/// URL resolution → security → CSP → mixed content → cache → network →
/// decode), sitting directly in front of [`fetch_with_cookies`]. Without
/// it, a page whose `<style>`/`<link>` sources `@import` the same
/// stylesheet twice (or reference an `<img src>` that also happens to be
/// `@import`ed as CSS, or any other URL reused across a single load)
/// issued one real GET per reference — wasted round-trips for bytes this
/// same page load already has. Scoped to exactly one [`Page::load`] call
/// ([`crate::page_source::load_source`] builds a fresh one per navigation/
/// reload, same lifetime as `Page`'s own `images`/`sheet` fields) — this
/// is not a cross-navigation HTTP cache (no freshness/expiry handling,
/// nothing persists across a `RELOAD`), just dedup within one document's
/// own resource fetches. Only successful fetches are cached; a failed
/// fetch is retried on every reference rather than caching the failure,
/// matching this file's "best-effort, never let one bad resource poison
/// more than its own reference" stance elsewhere.
#[derive(Default)]
pub(crate) struct ResourceCache {
    entries: HashMap<String, Rc<net::Response>>,
}

impl ResourceCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Same signature/behavior as [`fetch_with_cookies`], except a second
    /// call with the same exact `url` string returns the first call's
    /// response without touching the network (or the cookie jar) again.
    /// URLs are matched verbatim, not re-normalized — two different
    /// spellings of the same resource (e.g. differing only in a trailing
    /// slash) miss the cache and fetch twice, same imprecision
    /// `resolve_url`'s own string-based callers already accept.
    pub(crate) fn fetch_cached(
        &mut self,
        url: &str,
        storage_root: &std::path::Path,
        proxy: Option<&net::ProxyConfig>,
        dns_server: Option<std::net::SocketAddr>,
    ) -> Result<Rc<net::Response>, net::Error> {
        if let Some(cached) = self.entries.get(url) {
            return Ok(Rc::clone(cached));
        }
        let response = Rc::new(fetch_with_cookies(url, storage_root, proxy, dns_server)?);
        self.entries.insert(url.to_string(), Rc::clone(&response));
        Ok(response)
    }
}

/// Parses this worker's optional 5th CLI argument into a
/// [`net::ProxyConfig`]: `"host:port"` (no auth) or
/// `"user:pass@host:port"` (HTTP Basic `Proxy-Authorization`) — the
/// authority portion of a `user:pass@host:port` proxy URL, minus the
/// scheme (this worker only ever tunnels via `CONNECT`, so a `http://`
/// vs `https://` proxy-facing scheme wouldn't change anything). Returns
/// `None` for a missing argument, an empty string, or one that doesn't
/// parse as `host:port` — an unparseable proxy argument degrades to "no
/// proxy" rather than refusing to start, matching this file's general
/// "best-effort, never fails the whole page load over one bad input"
/// stance elsewhere (see `crate::page_source::resolve_url`).
pub(crate) fn parse_proxy_arg(arg: Option<&str>) -> Option<net::ProxyConfig> {
    let arg = arg?;
    if arg.is_empty() {
        return None;
    }
    let (credentials, host_port) = match arg.split_once('@') {
        Some((credentials, host_port)) => (Some(credentials), host_port),
        None => (None, arg),
    };
    let (host, port) = host_port.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    let (username, password) = match credentials.and_then(|c| c.split_once(':')) {
        Some((user, pass)) => (Some(user.to_string()), Some(pass.to_string())),
        None => (None, None),
    };
    Some(net::ProxyConfig {
        host: host.to_string(),
        port,
        username,
        password,
    })
}

/// Parses this worker's optional 6th CLI argument (`"host:port"`) into
/// the DNS server every fetch should resolve through instead of the OS
/// resolver, via `net::get_via_dns`. Same "degrade to none rather than
/// refuse to start" stance as [`parse_proxy_arg`] — a missing argument,
/// empty string, or one that doesn't resolve to a real `SocketAddr`
/// (`host:port` needs the host to already be an IP literal - DNS-syntax
/// resolution of the server's own address isn't attempted, this worker
/// has no resolver to bootstrap one with) all degrade to "use the OS
/// resolver".
pub(crate) fn parse_dns_arg(arg: Option<&str>) -> Option<std::net::SocketAddr> {
    let arg = arg?;
    if arg.is_empty() {
        return None;
    }
    arg.parse().ok()
}
