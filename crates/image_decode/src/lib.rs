//! Real image decoding for `<img>` — the spec's "gap total, zero code
//! anywhere" item (`mockup/rendering-engine-gaps.md` §5). A thin wrapper
//! around the `image` crate (PNG + JPEG only, matching this workspace's
//! own "vet a real crate for a complex binary format rather than
//! hand-roll it" convention already applied to `hyper`/`rusqlite`/
//! `wgpu`) that hands back real pixel dimensions and RGBA8 bytes —
//! nothing browser-specific lives here, that's `layout-engine` (intrinsic
//! sizing) and `render` (painting) each consuming [`DecodedImage`].
//!
//! Deliberately its own tiny crate rather than living inside
//! `layout-engine` or `render` directly: both of those need to depend on
//! the decoded-image *type*, and `render` already depends on
//! `layout-engine` (so `layout-engine` can't depend on `render` without
//! a cycle) — a small shared crate is the same "crate-per-concern, no
//! dependency cycles" shape the rest of this workspace already follows.
//!
//! Scope: no GIF/WebP/AVIF/SVG (not in the `image` crate's default
//! feature set enabled here — real formats, just not turned on), no
//! animated images (always decodes the first/only frame), no ICC color
//! profile handling (`image` itself doesn't apply one either).
#[derive(Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8, row-major, top-to-bottom — same convention
    /// `render::GpuRenderer::render_to_rgba`'s own return value uses.
    pub rgba: Vec<u8>,
}

/// Decodes `bytes` (a whole PNG or JPEG file's contents, as fetched over
/// the network) into real pixel data. `None` on anything that isn't a
/// decodable PNG/JPEG — a truncated download, an unsupported format, or
/// genuinely corrupt data all collapse to the same "no image" outcome a
/// caller treats like a `<img>` with no `src` at all, rather than a
/// distinct error type nothing downstream would act on differently.
pub fn decode(bytes: &[u8]) -> Option<DecodedImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    Some(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}
