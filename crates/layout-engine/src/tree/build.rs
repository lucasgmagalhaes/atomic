//! `build_children`/`build` — the recursive box-tree construction itself,
//! split out from `tree/mod.rs`.

use css::{matching_declarations_indexed, ElementSnapshot, SelectorIndex, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::{resolve_style, Color, ComputedStyle, Display};

use super::inline::{collect_inline_spans, is_inline_level};
use super::snapshot::{build_element_snapshot, is_never_rendered};
use super::types::{Dimensions, LayoutBox, ResolvedBorder};

/// Builds every child box of `node` in one pass, grouping consecutive
/// inline-level children (see [`is_inline_level`]) into a single
/// [`super::types::InlineSpanSource`]-carrying box instead of one box per
/// child — the real inline formatting context this module's doc
/// describes. A run of exactly one plain-text child (no inline element
/// siblings) still takes the cheaper pre-existing `text: Some(...)` path
/// via `build` instead, unchanged from before multi-span runs existed.
#[allow(clippy::too_many_arguments)]
fn build_children<'a>(
    dom: &'a Dom,
    child_ids: &[NodeId],
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
    parent_font_size: f64,
    parent_color: Color,
) -> Vec<LayoutBox> {
    let mut result = Vec::new();
    let mut pending_inline_run: Vec<NodeId> = Vec::new();

    let flush = |run: &mut Vec<NodeId>,
                 result: &mut Vec<LayoutBox>,
                 chain: &mut Vec<ElementSnapshot<'a>>| {
        if run.is_empty() {
            return;
        }
        if run.len() == 1 {
            if let Some(NodeData::Text(_)) = dom.get(run[0]).map(|n| &n.data) {
                if let Some(b) = build(
                    dom,
                    run[0],
                    sheet,
                    index,
                    viewport_width,
                    viewport_height,
                    chain,
                    parent_font_size,
                    parent_color,
                ) {
                    result.push(b);
                }
                run.clear();
                return;
            }
        }

        let mut spans = Vec::new();
        let first_node = run[0];
        for &id in run.iter() {
            collect_inline_spans(
                dom,
                id,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
                &mut spans,
            );
        }
        run.clear();
        if !spans.is_empty() {
            // The run's own baseline style, for the common single-span
            // case (plain text, no inline element siblings) where it's
            // exactly the text's real style - matches what a plain text
            // leaf box carried before this module supported multi-span
            // runs. When a run mixes multiple differently-styled spans,
            // this is just the run's inherited starting point, not
            // necessarily any one glyph's actual color/size - each glyph
            // still gets its own correct value via `inline_spans`.
            let mut style = ComputedStyle::initial();
            style.font_size = parent_font_size;
            style.color = parent_color;
            result.push(LayoutBox {
                node: first_node,
                style,
                children: Vec::new(),
                dimensions: Dimensions::default(),
                border: ResolvedBorder::default(),
                text: None,
                inline_spans: Some(spans),
                glyphs: Vec::new(),
                image: None,
            });
        }
    };

    for &child in child_ids {
        if is_inline_level(
            dom,
            child,
            sheet,
            index,
            viewport_width,
            viewport_height,
            chain,
            parent_font_size,
            parent_color,
        ) {
            pending_inline_run.push(child);
        } else {
            flush(&mut pending_inline_run, &mut result, chain);
            if let Some(b) = build(
                dom,
                child,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
            ) {
                result.push(b);
            }
        }
    }
    flush(&mut pending_inline_run, &mut result, chain);

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build<'a>(
    dom: &'a Dom,
    node: NodeId,
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
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
            border: ResolvedBorder::default(),
            text: Some(text.clone()),
            inline_spans: None,
            glyphs: Vec::new(),
            image: None,
        });
    }

    let NodeData::Element { tag, .. } = &n.data else {
        return None;
    };
    if is_never_rendered(tag) {
        return None;
    }

    chain.push(build_element_snapshot(dom, node));
    let style = resolve_style(
        &matching_declarations_indexed(index, sheet, chain, viewport_width, viewport_height),
        parent_font_size,
        parent_color,
    );

    let children = if style.display == Display::None {
        Vec::new()
    } else {
        build_children(
            dom,
            &n.children,
            sheet,
            index,
            viewport_width,
            viewport_height,
            chain,
            style.font_size,
            style.color,
        )
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
        border: ResolvedBorder::default(),
        text: None,
        inline_spans: None,
        glyphs: Vec::new(),
        image: None,
    })
}
