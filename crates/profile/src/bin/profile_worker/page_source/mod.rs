//! Pulling CSS/script/image *sources* out of a parsed DOM and turning them
//! into real, fetched, merged inputs for a `Page`: `<style>`/`<link>`
//! stylesheets (including `@import`), inline `<script>` text, `<meta
//! http-equiv="Content-Security-Policy">` policies, and `<img src>`
//! decoding.
//!
//! Split into `url.rs` (`resolve_url`), `scripts.rs` (`load_scripts`),
//! `images.rs` (`load_images`), `csp.rs` (`collect_meta_csp_policies`),
//! and `stylesheet.rs` (`build_stylesheet`).

mod csp;
mod images;
mod scripts;
mod stylesheet;
mod url;

pub(crate) use csp::collect_meta_csp_policies;
pub(crate) use images::load_images;
pub(crate) use scripts::load_scripts;
pub(crate) use stylesheet::build_stylesheet;
pub(crate) use url::resolve_url;
