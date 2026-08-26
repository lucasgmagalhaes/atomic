//! `new FormData(form)`'s population walk: pulls named `<input>`/`<select>`/
//! `<textarea>` controls out of a live `<form>` element, per the rules
//! documented on the crate module doc.

use super::types::{Entry, EntryValue};

/// Populates `entries` from a `<form>` element node — controls walked in
/// document order, rules documented on the module. Degrades to no-op when
/// there's no DOM behind the context.
pub(super) unsafe fn populate_from_form(
  ctx: *mut quickjs_sys::JSContext,
  form: quickjs_sys::JSValue,
  entries: &mut Vec<Entry>,
) {
  let Some(id) = crate::dom_bindings::node_id(ctx, form) else {
    return;
  };
  let state = crate::host_state::get(ctx);
  if state.is_null() {
    return;
  }
  let dom = std::ptr::addr_of_mut!((*state).dom);
  let controls = descendants_matching_tags(&*dom, id, &["input", "select", "textarea"]);
  for control in controls {
    let name = match (*dom).attribute(control, "name") {
      Some(n) if !n.is_empty() => n.to_string(),
      _ => continue,
    };
    if (*dom).attribute(control, "disabled").is_some() {
      continue;
    }
    let tag = match (*dom).get(control).map(|n| &n.data) {
      Some(dom::NodeData::Element { tag, .. }) => tag.as_str(),
      _ => continue,
    };
    let value = match tag {
      "input" => {
        let input_type = (*dom)
          .attribute(control, "type")
          .unwrap_or("text")
          .to_ascii_lowercase();
        if input_type == "checkbox" || input_type == "radio" {
          if (*dom).attribute(control, "checked").is_none() {
            continue;
          }
          (*dom)
            .attribute(control, "value")
            .unwrap_or("on")
            .to_string()
        } else {
          (*dom).value(control)
        }
      }
      "select" => {
        let Some(selected) = selected_option(&*dom, control) else {
          continue;
        };
        option_effective_value(&*dom, selected)
      }
      // textarea falls through to Dom::value's text-content fallback
      _ => (*dom).value(control),
    };
    entries.push(Entry {
      name,
      value: EntryValue::Text(value),
    });
  }
}

/// All element descendants of `start` whose tag is in `tags`, document
/// order (same iterative-DFS shape as `dom_bindings::matching_by_tag`,
/// with that function's visit/result limits).
fn descendants_matching_tags(
  dom: &dom::Dom,
  start: dom::NodeId,
  tags: &[&str],
) -> Vec<dom::NodeId> {
  const MAX_VISITS: usize = 4096;
  const MAX_RESULTS: usize = 4096;
  let mut result = Vec::new();
  let mut stack = vec![start];
  let mut visits = 0usize;
  while let Some(id) = stack.pop() {
    visits += 1;
    if visits > MAX_VISITS || result.len() > MAX_RESULTS {
      break;
    }
    if let Some(node) = dom.get(id) {
      if id != start {
        if let dom::NodeData::Element { tag, .. } = &node.data {
          if tags.contains(&tag.as_str()) {
            result.push(id);
          }
        }
      }
      stack.extend(node.children.iter().rev().copied());
    }
  }
  result
}

/// The first explicitly-selected `<option>` under `select`, mirroring
/// `select_value_get`'s documented deviation (nothing implicitly picked).
fn selected_option(dom: &dom::Dom, select: dom::NodeId) -> Option<dom::NodeId> {
  descendants_matching_tags(dom, select, &["option"])
    .into_iter()
    .find(|option| dom.attribute(*option, "selected").is_some())
}

/// An option's effective value: its `value` attribute when present,
/// otherwise its text content (same rule `dom_bindings` uses).
fn option_effective_value(dom: &dom::Dom, option: dom::NodeId) -> String {
  match dom.attribute(option, "value") {
    Some(v) => v.to_string(),
    None => dom.text_content(option),
  }
}
