//! `load_font_faces` — split out from `page_source.rs`.

use neutron::css::Stylesheet;

use crate::network::ResourceCache;

use super::url::resolve_url;

/// Fetches and registers every real `@font-face` (`neutron::css::FontFaceRule`)
/// `sheet` carries (already merged across `@import`s by
/// `build_stylesheet`/`merge_stylesheet_text`), same "one real GET per
/// resource, at load time" convention `load_images` already uses for
/// `<img src>` - `url` resolved against `base_url` via [`resolve_url`],
/// fetched through the same [`ResourceCache`] every other request in this
/// worker shares (so a profile's proxy/DNS/cookie jar apply here too), and
/// handed to `neutron::layout::register_font_face` to load into the one
/// process-wide `cosmic-text` font database. An unresolvable URL or a
/// failed fetch simply registers nothing for that face - the same
/// "best-effort, never fail the whole page load over one bad resource"
/// stance `load_images`/`build_stylesheet` already take; a page whose
/// `@font-face` font never loads just keeps whatever `font-family`
/// resolves to without it (its own fallback, per `FontFamily`'s own doc),
/// not a failed page load.
pub(crate) fn load_font_faces(
    sheet: &Stylesheet,
    base_url: Option<&str>,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
    cache: &mut ResourceCache,
) {
    for face in &sheet.font_faces {
        let Some(url) = resolve_url(base_url, &face.url) else {
            continue;
        };
        let Ok(response) = cache.fetch_cached(&url, storage_root, proxy, dns_server) else {
            continue;
        };
        neutron::layout::register_font_face(response.body.clone());
    }
}
