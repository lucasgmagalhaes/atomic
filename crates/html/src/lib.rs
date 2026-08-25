pub mod sink;

use dom::{Dom, NodeId};
use html5ever::driver::ParseOpts;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;

pub use sink::Sink;

/// Parses `html` into a `dom::Dom`. Real HTML5 tree construction via
/// `html5ever` — tag-soup error recovery, implied tags, foster-parenting
/// misnested table content, and so on all come from the parser itself,
/// not reimplemented here. See `sink`'s module docs for the handful of
/// things `dom`'s node model can't represent (doctype, template content
/// isolation, processing instructions).
pub fn parse(html: &str) -> Dom {
    let sink = Sink::new();
    parse_document(sink, ParseOpts::default())
        .from_utf8()
        .one(html.as_bytes())
}

/// Same as [`parse`], but returns the id of the parsed `<html>` element
/// specifically (falling back to the document root if none was
/// found — malformed input html5ever still recovers from), which is
/// usually what a caller actually wants to hand to
/// `layout_engine::build_box_tree` instead of the document node itself.
pub fn parse_to_html_element(html: &str) -> (Dom, dom::NodeId) {
    let dom = parse(html);
    let root = dom.root();
    let html_el = dom
        .get(root)
        .and_then(|n| {
            n.children
                .iter()
                .find(|&&c| is_element_named(&dom, c, "html"))
                .copied()
        })
        .unwrap_or(root);
    (dom, html_el)
}

/// Parses `html` as an HTML *fragment* — the write side of
/// `element.innerHTML = html` / `element.outerHTML = html`. There is no
/// real context-sensitive fragment parsing here (e.g. `<tr>innerHTML`
/// should parse content inside an implicit `<table>` context per spec;
/// this doesn't do that) — `html` is parsed the same way `parse` parses
/// a full document (via the existing `Sink`, so all the same tag-soup
/// recovery/implied-tag behavior applies), then this returns the ids of
/// whatever ended up as the direct children of the parsed `<body>`
/// element (falling back to the parsed `<html>` element's own children
/// if no `<body>` was found — html5ever's implied-tag recovery should
/// always produce one for any non-degenerate input, but a caller
/// shouldn't have to worry about that edge case). The caller is
/// responsible for cloning (`dom::Dom::adopt`, once that method lands)
/// each returned id into the real document — the returned `Dom` is a
/// throwaway parse buffer, not meant to be kept around or attached to
/// anything directly, since its `NodeId`s are only valid within it.
pub fn parse_fragment(html: &str) -> (Dom, Vec<NodeId>) {
    let dom = parse(html);
    let root = dom.root();
    let html_el = dom.get(root).and_then(|n| {
        n.children
            .iter()
            .find(|&&c| is_element_named(&dom, c, "html"))
            .copied()
    });
    let body_el = html_el.and_then(|html_el| dom.get(html_el)).and_then(|n| {
        n.children
            .iter()
            .find(|&&c| is_element_named(&dom, c, "body"))
            .copied()
    });
    let children = body_el
        .or(html_el)
        .or(Some(root))
        .and_then(|id| dom.get(id))
        .map(|n| n.children.clone())
        .unwrap_or_default();
    (dom, children)
}

fn is_element_named(dom: &Dom, id: dom::NodeId, name: &str) -> bool {
    matches!(&dom.get(id).map(|n| &n.data), Some(dom::NodeData::Element { tag, .. }) if tag == name)
}
