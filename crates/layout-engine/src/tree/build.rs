//! `build_children`/`build` — the recursive box-tree construction itself,
//! split out from `tree/mod.rs`.

use css::{ElementSnapshot, SelectorIndex, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::{Color, ComputedStyle, Display, ListStyleType};

use super::inline::{collect_inline_spans, is_inline_level};
use super::snapshot::{is_never_rendered, resolve_element_style};
use super::types::{Dimensions, InlineSpanSource, LayoutBox, ResolvedBorder};

/// The literal marker text for one `<li>`, given its own resolved
/// `list_style_type` (already defaulted — see [`inject_list_markers`]).
/// Unicode glyphs, not a new paint primitive — `render` never knows a
/// marker isn't ordinary text (see [`ListStyleType`]'s own doc).
fn marker_text(kind: ListStyleType, ordinal: u32) -> Option<String> {
    match kind {
        ListStyleType::None => None,
        ListStyleType::Disc => Some("\u{2022} ".to_string()),
        ListStyleType::Circle => Some("\u{25E6} ".to_string()),
        ListStyleType::Square => Some("\u{25AA} ".to_string()),
        ListStyleType::Decimal => Some(format!("{ordinal}. ")),
    }
}

/// Merges `marker` onto the front of `li_box`'s own first line, reusing
/// whichever shape that first child already has instead of inserting a new
/// sibling box — the only way to land the marker on the *same* line as the
/// item's own text with this crate's simple block-stacking layout (a
/// separate marker box would just stack above it as its own row, since
/// [`crate::layout::layout_children`] lays out every direct child as a
/// block). Falls back to a standalone leading marker box (its own row,
/// above the item's content) only when the `<li>`'s first child is itself
/// block-level (e.g. `<li><div>...</div></li>`) — an honest, narrower
/// scope cut for that uncommon shape, same "real but not always inline"
/// convention `Float`'s own doc already documents elsewhere in this crate.
fn merge_marker_into_li(li_box: &mut LayoutBox, marker: &str) {
    let Some(first) = li_box.children.first_mut() else {
        li_box.children.push(LayoutBox {
            node: li_box.node,
            style: li_box.style,
            children: Vec::new(),
            dimensions: Dimensions::default(),
            border: ResolvedBorder::default(),
            text: Some(marker.to_string()),
            inline_spans: None,
            glyphs: Vec::new(),
            image: None,
            scroll_offset: (0.0, 0.0),
        });
        return;
    };
    if let Some(text) = &mut first.text {
        text.insert_str(0, marker);
    } else if let Some(spans) = &mut first.inline_spans {
        spans.insert(
            0,
            InlineSpanSource {
                text: marker.to_string(),
                font_size: li_box.style.font_size,
                color: li_box.style.color,
            },
        );
    } else {
        li_box.children.insert(
            0,
            LayoutBox {
                node: li_box.node,
                style: li_box.style,
                children: Vec::new(),
                dimensions: Dimensions::default(),
                border: ResolvedBorder::default(),
                text: Some(marker.to_string()),
                inline_spans: None,
                glyphs: Vec::new(),
                image: None,
                scroll_offset: (0.0, 0.0),
            },
        );
    }
}

/// Real per-list marker generation for `<ol>`/`<ul>` — see `ListStyleType`'s
/// own doc. Walks only `children`'s own direct entries (already grouped by
/// `build_children`, so this never sees below one level), matching each
/// against `dom` by its `LayoutBox::node` to find the real `<li>` elements
/// among them (text/other-element siblings of an `<li>` are real but rare
/// and simply skipped, same as unknown elements elsewhere in this crate).
/// The ordinal counter is local to this one call — a nested `<ol>`/`<ul>`
/// inside an `<li>` is built by its own separate recursive `build()` call
/// (see `build_children`), which runs this exact function again with a
/// fresh counter, giving real, correct counter nesting for free.
///
/// `list_style_type` isn't a generically inherited property in this crate
/// (see `style/mod.rs`'s own doc: only `font_size`/`color` are threaded
/// through the whole recursive build chain). Rather than add a third
/// inherited parameter everywhere just for this, the one real-world case
/// that matters — `ol { list-style-type: ... }` styling every item at once
/// — is covered directly here: an `<li>` with no `list-style-type` of its
/// own falls back to its `<ol>`/`<ul>` parent's own resolved value
/// (`own_list_style_type`) before the tag-based default.
fn inject_list_markers(
    dom: &Dom,
    tag: &str,
    own_list_style_type: Option<ListStyleType>,
    children: &mut [LayoutBox],
) {
    let default_type = own_list_style_type.unwrap_or(if tag == "ol" {
        ListStyleType::Decimal
    } else {
        ListStyleType::Disc
    });
    let mut ordinal = 0u32;
    for child in children.iter_mut() {
        let Some(NodeData::Element { tag: child_tag, .. }) = dom.get(child.node).map(|n| &n.data)
        else {
            continue;
        };
        if child_tag != "li" {
            continue;
        }
        ordinal += 1;
        let kind = child.style.list_style_type.unwrap_or(default_type);
        if let Some(marker) = marker_text(kind, ordinal) {
            merge_marker_into_li(child, &marker);
        }
    }
}

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
                scroll_offset: (0.0, 0.0),
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
            scroll_offset: (0.0, 0.0),
        });
    }

    let NodeData::Element {
        tag, attributes, ..
    } = &n.data
    else {
        return None;
    };
    if is_never_rendered(tag) {
        return None;
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
    );

    let mut children = if style.display == Display::None {
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

    if tag == "ol" || tag == "ul" {
        inject_list_markers(dom, tag, style.list_style_type, &mut children);
    }

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
        scroll_offset: dom.element_scroll_offset(node),
    })
}
