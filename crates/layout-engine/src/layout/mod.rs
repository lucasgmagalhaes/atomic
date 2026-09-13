//! Layout orchestration: box-model resolution shared by every display type,
//! dispatching to block-stacking or flex for how children get arranged.
//! Block formatting context: children stack vertically, no inline flow, no
//! floats. `auto` margins resolve to `0.0`, not CSS's actual auto-margin
//! centering. No margin collapsing: adjacent margins both take full effect
//! instead of collapsing to the larger one.
//!
//! `position: relative`/`absolute` are real now (`static` is still every
//! box's default and behaves exactly as before) - see
//! `layout_block`/`layout_children`'s own docs for exactly what's
//! modeled. `fixed`/`sticky` aren't.
//!
//! `border-width`/`border-style`/`border-color` are real now too: a
//! bordered box's `border-width` genuinely grows `Dimensions` (real
//! box-model space, not just a paint-time decoration) — `Dimensions` is
//! the border box (content + padding + border) rather than the padding
//! box it used to be. `border-radius`/per-side border colors/styles
//! aren't modeled (see `style::ComputedStyle::border_color`'s own doc).
//!
//! Split into `text_boxes.rs` (`layout_text_box`/`layout_inline_box`),
//! `offsets.rs` (box-model edge/offset resolution helpers), `block.rs`
//! (`layout_block`), and `children.rs` (`layout_children`).

mod block;
mod children;
mod offsets;
mod text_boxes;

pub use block::layout_block;
pub(crate) use children::layout_children;
pub(crate) use offsets::resolve_edge;
