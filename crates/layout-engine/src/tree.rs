//! Box tree construction: walks a `dom::Dom` and attaches a resolved
//! `ComputedStyle` to every element, using `css`'s cascade against the
//! live ancestor chain. Scoped: only `dom::NodeData::Element` nodes become
//! boxes — `Text`/`Comment`/`Document` are skipped (no inline text layout
//! yet; that needs real glyph measurement, which belongs with the text
//! shaping crate, not here). `display: none` prunes the whole subtree,
//! matching the real CSS box generation rule.
use css::{matching_declarations, ElementSnapshot, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::{resolve_style, ComputedStyle, Display};

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
}

fn classes_of(attributes: &std::collections::HashMap<String, String>) -> Vec<String> {
    attributes
        .get("class")
        .map(|c| c.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

fn build(dom: &Dom, node: NodeId, sheet: &Stylesheet, chain: &mut Vec<ElementSnapshot>) -> Option<LayoutBox> {
    let n = dom.get(node)?;
    let NodeData::Element { tag, attributes } = &n.data else {
        return None;
    };

    chain.push(ElementSnapshot {
        tag: tag.clone(),
        id: attributes.get("id").cloned(),
        classes: classes_of(attributes),
    });
    let style = resolve_style(&matching_declarations(sheet, chain));

    let children = if style.display == Display::None {
        Vec::new()
    } else {
        n.children
            .iter()
            .filter_map(|&child| build(dom, child, sheet, chain))
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
    })
}

/// Builds the box tree rooted at `node` (typically an `<html>`-equivalent
/// element, or any element for testing in isolation). Returns `None` if
/// `node` doesn't exist or isn't an element (includes `display: none`,
/// which produces no box at all per CSS box generation).
pub fn build_box_tree(dom: &Dom, node: NodeId, sheet: &Stylesheet) -> Option<LayoutBox> {
    let mut chain = Vec::new();
    build(dom, node, sheet, &mut chain)
}
