use dom::NodeData;

fn find_first(dom: &dom::Dom, id: dom::NodeId, tag: &str) -> Option<dom::NodeId> {
    if let Some(node) = dom.get(id) {
        if let NodeData::Element { tag: t, .. } = &node.data {
            if t == tag {
                return Some(id);
            }
        }
        for &child in &node.children {
            if let Some(found) = find_first(dom, child, tag) {
                return Some(found);
            }
        }
    }
    None
}

#[test]
fn parses_a_simple_element_with_text() {
    let dom = html::parse("<p>hello</p>");
    let p = find_first(&dom, dom.root(), "p").expect("should find <p>");
    assert_eq!(dom.text_content(p), "hello");
}

#[test]
fn parses_attributes() {
    let dom = html::parse(r#"<div id="main" class="box highlighted"></div>"#);
    let div = find_first(&dom, dom.root(), "div").expect("should find <div>");
    assert_eq!(dom.attribute(div, "id"), Some("main"));
    assert_eq!(dom.attribute(div, "class"), Some("box highlighted"));
}

#[test]
fn parses_nested_elements() {
    let dom = html::parse("<div><p>outer</p><span>inner</span></div>");
    let div = find_first(&dom, dom.root(), "div").unwrap();
    let node = dom.get(div).unwrap();
    // html5ever wraps content in implied <html><head></head><body>...
    // so we can't assume div's direct children are p/span - just confirm
    // both exist somewhere under it via find_first from div.
    assert!(find_first(&dom, div, "p").is_some());
    assert!(find_first(&dom, div, "span").is_some());
    let _ = node;
}

#[test]
fn merges_adjacent_text_into_one_node() {
    // "hello " and "world" arrive as separate character tokens around
    // nothing that would split them into different elements - the parser
    // should merge them into a single text node's content.
    let dom = html::parse("<p>hello world</p>");
    let p = find_first(&dom, dom.root(), "p").unwrap();
    let text_children: Vec<_> = dom
        .get(p)
        .unwrap()
        .children
        .iter()
        .filter(|&&c| matches!(dom.get(c).unwrap().data, NodeData::Text(_)))
        .collect();
    assert_eq!(
        text_children.len(),
        1,
        "adjacent text should merge into one node"
    );
    assert_eq!(dom.text_content(p), "hello world");
}

#[test]
fn recovers_from_malformed_unclosed_tags() {
    // Real HTML5 tag-soup error recovery: no closing tags at all, parser
    // must still produce a sensible tree instead of erroring out.
    let dom = html::parse("<div><p>one<p>two");
    let div = find_first(&dom, dom.root(), "div").unwrap();
    let ps: Vec<_> = dom
        .get(div)
        .unwrap()
        .children
        .iter()
        .filter(|&&c| find_first(&dom, c, "p") == Some(c))
        .collect();
    assert_eq!(
        ps.len(),
        2,
        "two implicitly-closed <p> siblings should both exist"
    );
}

#[test]
fn parse_to_html_element_finds_the_html_tag() {
    let (dom, html_el) = html::parse_to_html_element("<p>x</p>");
    assert!(
        matches!(&dom.get(html_el).unwrap().data, NodeData::Element { tag, .. } if tag == "html")
    );
}

#[test]
fn template_content_is_a_real_isolated_fragment() {
    let dom = html::parse("<div><template><p>hi</p></template></div>");
    let div = find_first(&dom, dom.root(), "div").unwrap();
    let template = find_first(&dom, div, "template").unwrap();

    // The template's own light-DOM children must be empty - its parsed
    // content was redirected into the content fragment, not appended as
    // normal children.
    assert!(
        dom.get(template).unwrap().children.is_empty(),
        "template's light-DOM children should be empty"
    );

    let content = dom
        .template_content(template)
        .expect("a <template> should always have a content fragment");
    assert!(matches!(
        dom.get(content).unwrap().data,
        NodeData::DocumentFragment
    ));
    let p = find_first(&dom, content, "p");
    assert!(
        p.is_some(),
        "the <p> should live inside the content fragment"
    );
    assert_eq!(dom.text_content(content), "hi");
}

#[test]
fn nested_templates_each_get_their_own_content_fragment() {
    let dom = html::parse("<template><template><span>inner</span></template></template>");
    let outer = find_first(&dom, dom.root(), "template").unwrap();
    let outer_content = dom.template_content(outer).unwrap();
    let inner = find_first(&dom, outer_content, "template").unwrap();
    let inner_content = dom.template_content(inner).unwrap();

    assert_ne!(outer_content, inner_content);
    assert!(dom.get(inner).unwrap().children.is_empty());
    assert_eq!(dom.text_content(inner_content), "inner");
}

#[test]
fn comments_become_comment_nodes() {
    let dom = html::parse("<div><!-- a comment --></div>");
    let div = find_first(&dom, dom.root(), "div").unwrap();
    let has_comment = dom.get(div).unwrap().children.iter().any(
    |&c| matches!(&dom.get(c).unwrap().data, NodeData::Comment(text) if text.trim() == "a comment"),
  );
    assert!(has_comment);
}
