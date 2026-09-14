//! `collect_inline_spans`/`is_inline_level` — split out from `tree/mod.rs`.

use css::{ElementSnapshot, SelectorIndex, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::{Color, Display, FontFamily};

use super::snapshot::{is_never_rendered, resolve_element_style};
use super::types::InlineSpanSource;

/// Recursively flattens `node`'s subtree into `out` as [`InlineSpanSource`]
/// runs, for a node already known to be part of an inline formatting
/// context (a text node, or an element whose *own* resolved `display` is
/// `Inline`). Once inside such a run, every descendant contributes
/// inline — this doesn't re-check `display` on nested elements, per the
/// module doc's "block-inside-inline isn't de-inlined" scope cut.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_inline_spans<'a>(
    dom: &'a Dom,
    node: NodeId,
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
    parent_font_size: f64,
    parent_color: Color,
    parent_font_family: FontFamily,
    out: &mut Vec<InlineSpanSource>,
) {
    let Some(n) = dom.get(node) else { return };

    match &n.data {
        NodeData::Text(text) => {
            if !text.trim().is_empty() {
                out.push(InlineSpanSource {
                    text: text.clone(),
                    font_size: parent_font_size,
                    color: parent_color,
                    font_family: parent_font_family,
                });
            }
        }
        NodeData::Element {
            tag, attributes, ..
        } => {
            if is_never_rendered(tag) {
                return;
            }
            let style = resolve_element_style(
                dom,
                node,
                tag,
                attributes,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
                parent_font_family,
            );
            if style.display != Display::None {
                for &child in &n.children {
                    collect_inline_spans(
                        dom,
                        child,
                        sheet,
                        index,
                        viewport_width,
                        viewport_height,
                        chain,
                        style.font_size,
                        style.color,
                        style.font_family.unwrap_or(parent_font_family),
                        out,
                    );
                }
            }
            chain.pop();
        }
        // A shadow root is never a light-DOM child that ordinary layout
        // walks into (see `dom::NodeData::ShadowRoot`'s own scope-cut
        // doc: no render integration yet), grouped with the other
        // structural/non-visible node kinds this pass already skips.
        NodeData::Comment(_)
        | NodeData::Document
        | NodeData::DocumentFragment
        | NodeData::ShadowRoot { .. } => {}
    }
}

/// `true` for a DOM child that starts/continues an inline run: a
/// non-whitespace text node, or an element whose own resolved `display`
/// is `Inline`. Resolving an element's style here means it gets resolved
/// again by whichever branch actually uses it (`build` or
/// `collect_inline_spans`) — cascade resolution is cheap enough (a handful
/// of declaration lookups) that recomputing it once more beats threading a
/// pre-resolved style through both call shapes.
#[allow(clippy::too_many_arguments)]
pub(super) fn is_inline_level<'a>(
    dom: &'a Dom,
    node: NodeId,
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
    parent_font_size: f64,
    parent_color: Color,
    parent_font_family: FontFamily,
) -> bool {
    match dom.get(node).map(|n| &n.data) {
        Some(NodeData::Text(text)) => !text.trim().is_empty(),
        Some(NodeData::Element {
            tag, attributes, ..
        }) => {
            if is_never_rendered(tag) {
                return false;
            }
            let style = resolve_element_style(
                dom,
                node,
                tag,
                attributes,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
                parent_font_family,
            );
            chain.pop();
            style.display == Display::Inline
        }
        _ => false,
    }
}
