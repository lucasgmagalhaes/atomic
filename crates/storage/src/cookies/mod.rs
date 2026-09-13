//! `document.cookie`-shaped cookie jar: real `Set-Cookie` parsing (name,
//! value, `Domain`, `Path`, `Max-Age`, `Expires`, `Secure`, `HttpOnly`),
//! real domain/path/expiry matching for building an outgoing `Cookie`
//! header, and real file persistence (same one-mutation-one-flush pattern
//! as [`crate::LocalStorage`]).
//!
//! `SameSite` (`Strict`/`Lax`/`None`) is now real: parsed from
//! `Set-Cookie` (defaulting to `Lax`, matching modern browsers' own
//! unspecified-`SameSite` default), stored, and enforced by
//! [`CookieJar::matching_with_context`]/[`CookieJar::header_value_with_context`]
//! against an explicit `request_is_same_site` flag a caller supplies — a
//! `SameSite=None` cookie set without `Secure` is rejected outright at
//! parse time (`parse_set_cookie` returns `None`), matching real browser
//! behavior. The plain [`CookieJar::matching`]/[`CookieJar::header_value`]
//! (used by every caller in this workspace today) are unchanged thin
//! wrappers that always pass `request_is_same_site: true` — this
//! workspace has no cross-origin embedding (no iframes exist in `dom` at
//! all) and already partitions cookie storage per request host one
//! directory per origin, so there is no cross-site cookie flow for the
//! policy to meaningfully block yet; the enforcement exists and is
//! tested at the crate level, ready for whatever caller first needs a
//! real cross-site request context. One deliberate simplification
//! against the full spec: `Lax` is enforced identically to `Strict` here
//! (both require `request_is_same_site: true`) rather than additionally
//! allowing top-level cross-site GET navigations, since nothing in this
//! workspace distinguishes a top-level navigation from a subresource
//! fetch yet.
//!
//! Other deviations from RFC 6265: no public-suffix-list-aware domain checks (a
//! cookie for `Domain=co.uk` would wrongly match every `co.uk` subdomain —
//! the PSL is a large, frequently-updated external dataset, out of scope
//! here), and `Expires`' HTTP-date is parsed by a small hand-rolled parser
//! (RFC 1123's fixed six-token format only, via the standard
//! days-since-epoch civil calendar algorithm — no crate dependency for
//! one date format).
//!
//! Split into `same_site.rs` (`SameSite`), `cookie.rs` (`Cookie`),
//! `date.rs` (HTTP-date parsing), `parse.rs` (`parse_set_cookie`),
//! `persist.rs` (the on-disk line format), and `jar.rs` (`CookieJar`).

mod cookie;
mod date;
mod jar;
mod parse;
mod persist;
mod same_site;

pub use cookie::Cookie;
pub use jar::CookieJar;
pub use parse::parse_set_cookie;
pub use same_site::SameSite;
