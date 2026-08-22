//! Box tree construction: walks a `dom::Dom` and attaches a resolved
//! `ComputedStyle` to every element, using `css`'s cascade against the
//! live ancestor chain. `dom::NodeData::Element` nodes become regular
//! boxes; `Text` nodes become boxes too, but carry their string in
//! `LayoutBox::text` instead of child boxes — actual glyph shaping/sizing
//! happens during layout (`layout::layout_children`), not here, since it
//! needs the containing width to wrap against. `Comment`/`Document` are
//! skipped. `display: none` prunes the whole subtree, matching the real
//! CSS box generation rule.
//!
//! No general inline formatting context: a text box is a leaf sized by
//! its own content, but there's no support for *mixing* text with inline
//! element children on the same line (e.g. `<p>hello <b>world</b></p>` -
//! the `<b>` would lay out as its own block-like box, not flow inline
//! with the surrounding text). Only the common "just text inside a block
//! element" case is handled.
use css::{matching_declarations, ElementSnapshot, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::{resolve_style, Color, ComputedStyle, Display};
use crate::text::PositionedGlyph;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Dimensions {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub node: NodeId,
    pub style: ComputedStyle,
    pub children: Vec<LayoutBox>,
    pub dimensions: Dimensions,
    /// `Some` only for boxes built from a `dom::NodeData::Text` node.
    /// Mutually exclusive with non-empty `children` in practice (text
    /// boxes are leaves).
    pub text: Option<String>,
    /// Filled in by `layout::layout_children` once the text box's width
    /// constraint is known; empty until then and always empty for
    /// non-text boxes.
    pub glyphs: Vec<PositionedGlyph>,
}

fn classes_of(attributes: &std::collections::HashMap<String, String>) -> Vec<String> {
    attributes
        .get("class")
        .map(|c| c.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

fn build(
    dom: &Dom,
    node: NodeId,
    sheet: &Stylesheet,
    chain: &mut Vec<ElementSnapshot>,
    parent_font_size: f64,
    parent_color: Color,
) -> Option<LayoutBox> {
    let n = dom.get(node)?;

    if let NodeData::Text(text) = &n.data {
        if text.trim().is_empty() {
            return None;
        }
        let mut style = ComputedStyle::initial();
        style.font_size = parent_font_size;
        style.color = parent_color;
        return Some(LayoutBox {
            node,
            style,
            children: Vec::new(),
            dimensions: Dimensions::default(),
            text: Some(text.clone()),
            glyphs: Vec::new(),
        });
    }

    let NodeData::Element { tag, attributes } = &n.data else {
        return None;
    };

    chain.push(ElementSnapshot {
        tag: tag.clone(),
        id: attributes.get("id").cloned(),
        classes: classes_of(attributes),
    });
    let style = resolve_style(&matching_declarations(sheet, chain), parent_font_size, parent_color);

    let children = if style.display == Display::None {
        Vec::new()
    } else {
        n.children
            .iter()
            .filter_map(|&child| build(dom, child, sheet, chain, style.font_size, style.color))
            .collect()
    };
    chain.pop();

    if style.display == Display::None {
        return None;
    }

    Some(LayoutBox {
        node,
        style,
        children,
        dimensions: Dimensions::default(),
        text: None,
        glyphs: Vec::new(),
    })
}

/// Builds the box tree rooted at `node` (typically an `<html>`-equivalent
/// element, or any element for testing in isolation). Returns `None` if
/// `node` doesn't exist or isn't an element/non-empty-text node (includes
/// `display: none`, which produces no box at all per CSS box generation).
/// `node`'s inherited `font-size` starts at the CSS initial value (16px),
/// same as a real document root.
pub fn build_box_tree(dom: &Dom, node: NodeId, sheet: &Stylesheet) -> Option<LayoutBox> {
    let mut chain = Vec::new();
    let initial = ComputedStyle::initial();
    build(dom, node, sheet, &mut chain, initial.font_size, initial.color)
}
