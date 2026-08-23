pub mod sink;

use dom::Dom;
use html5ever::driver::ParseOpts;
use html5ever::tendril::TendrilSink;
use html5ever::parse_document;

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
        .and_then(|n| n.children.iter().find(|&&c| is_element_named(&dom, c, "html")).copied())
        .unwrap_or(root);
    (dom, html_el)
}

fn is_element_named(dom: &Dom, id: dom::NodeId, name: &str) -> bool {
    matches!(&dom.get(id).map(|n| &n.data), Some(dom::NodeData::Element { tag, .. }) if tag == name)
}
