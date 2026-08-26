use dom::NodeData;

fn tag_of(dom: &dom::Dom, id: dom::NodeId) -> Option<&str> {
  match &dom.get(id)?.data {
    NodeData::Element { tag, .. } => Some(tag.as_str()),
    _ => None,
  }
}

#[test]
fn simple_fragment_returns_top_level_elements_in_order() {
  let (dom, ids) = html::parse_fragment("<p>hello</p><span>world</span>");
  assert_eq!(ids.len(), 2, "expected exactly two top-level nodes");
  assert_eq!(tag_of(&dom, ids[0]), Some("p"));
  assert_eq!(tag_of(&dom, ids[1]), Some("span"));
  assert_eq!(dom.text_content(ids[0]), "hello");
  assert_eq!(dom.text_content(ids[1]), "world");
}

#[test]
fn bare_text_fragment_produces_a_text_node() {
  let (dom, ids) = html::parse_fragment("hello world");
  assert!(!ids.is_empty(), "expected at least one top-level node");
  let has_text = ids
    .iter()
    .any(|&id| matches!(&dom.get(id).unwrap().data, NodeData::Text(text) if text == "hello world"));
  assert!(has_text, "expected a text node with the fragment's content");
}

#[test]
fn nested_fragment_keeps_the_whole_subtree() {
  let (dom, ids) = html::parse_fragment("<div><span>x</span></div>");
  assert_eq!(ids.len(), 1, "expected exactly one top-level node");
  let div = ids[0];
  assert_eq!(tag_of(&dom, div), Some("div"));
  let div_node = dom.get(div).unwrap();
  let has_span = div_node
    .children
    .iter()
    .any(|&c| tag_of(&dom, c) == Some("span"));
  assert!(
    has_span,
    "expected <div>'s children to contain the nested <span>"
  );
}

#[test]
fn malformed_tag_soup_does_not_panic() {
  let (dom, ids) = html::parse_fragment("<p>a<p>b");
  assert!(!ids.is_empty(), "expected some recovered top-level node(s)");
  // Malformed input should still recover into sensible <p> siblings
  // rather than one broken/nested mess.
  let p_count = ids
    .iter()
    .filter(|&&id| tag_of(&dom, id) == Some("p"))
    .count();
  assert_eq!(p_count, 2, "expected two implicitly-closed <p> siblings");
}
