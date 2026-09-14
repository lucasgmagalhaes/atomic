//! Cascaded declarations → typed computed style. Scoped property set:
//! `display` (block/inline/flex/none), `width`/`height`, `margin`/
//! `padding` (shorthands + longhands), `position` (`static`/`relative`/
//! `absolute`) with `top`/`right`/`bottom`/`left`, `border-width`/
//! `border-style`/`border-color` (plus the `border` shorthand — solid
//! only, one color for all four sides, real box-model growth not just a
//! paint decoration), and the flex properties `flex-direction`/
//! `justify-content`/`align-items`/`flex-grow`/`flex-shrink`/
//! `flex-basis`, `background-color` (also accepted as `background`, but
//! only the solid-color form — no gradients/images), `font-size` (px
//! only), and `color`. `font-size` and `color` are the only properties
//! this crate inherits — every other property resolves independently of
//! the parent's computed style. No `flex`/`border` per-side-longhand
//! shorthand beyond what's listed above.
//!
//! Split into `types.rs` (plain enum/struct property types),
//! `computed_style.rs` (the `ComputedStyle` struct + its initial
//! values), `property_parsers.rs` (per-property/shorthand token
//! parsing), and `apply_declaration.rs` (the one property-name match
//! that ties parsing to `ComputedStyle` fields) — this file keeps only
//! the public entry point, `resolve_style`.

use css::{Declaration, MatchedDeclarations};

mod apply_declaration;
mod computed_style;
mod property_parsers;
mod types;

pub use computed_style::ComputedStyle;
pub use types::{
    AlignItems, BorderStyle, BoxShadow, Clear, Color, Display, EdgeSizes, FlexDirection, Float,
    FontFamily, GenericFontFamily, GridTrackSize, GridTracks, JustifyContent, Length,
    LinearGradient, ListStylePosition, ListStyleType, Overflow, Position,
};

/// Applies `matched` (already cascade-ordered lowest to highest priority,
/// see `css::matching_declarations`) on top of the initial values.
/// `parent_font_size` seeds the one inherited property this crate models
/// — pass `ComputedStyle::initial().font_size` for the document root,
/// which has no parent to inherit from.
pub fn resolve_style(
    matched: &[MatchedDeclarations],
    parent_font_size: f64,
    parent_color: Color,
) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.font_size = parent_font_size;
    style.color = parent_color;
    for m in matched {
        for decl in m.declarations {
            apply_declaration::apply_declaration(&mut style, decl);
        }
    }
    style
}

/// Applies an inline `style="..."` HTML attribute's declarations
/// (already parsed via `css::parse_inline_style`) on top of an
/// already-cascaded [`ComputedStyle`] — inline style always wins over
/// every stylesheet rule regardless of selector specificity (CSS
/// Cascading and Inheritance §6.4.1), so this must run strictly after
/// every `matching_declarations`-sourced declaration, never merged into
/// the same sorted pass.
pub fn apply_inline_declarations(style: &mut ComputedStyle, decls: &[Declaration]) {
    for decl in decls {
        apply_declaration::apply_declaration(style, decl);
    }
}
