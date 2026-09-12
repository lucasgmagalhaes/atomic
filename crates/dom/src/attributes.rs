use crate::{DirtyFlags, Dom, Node, NodeData, NodeId};

/// Attribute mutations affect selector matching, style cascade, and
/// visual appearance. At the DOM layer we can't know which specific
/// attributes affect layout (e.g. `class` vs `data-*`), so LAYOUT is
/// included conservatively — a higher layer (layout engine) can refine
/// this if needed.
const ATTR_DIRTY: DirtyFlags = DirtyFlags::SELECTORS
    .union(DirtyFlags::STYLE)
    .union(DirtyFlags::LAYOUT)
    .union(DirtyFlags::PAINT);

impl Dom {
    pub fn set_attribute(&mut self, id: NodeId, name: &str, value: &str) {
        self.mark_dirty(ATTR_DIRTY);
        if let Some(Node {
            data:
                NodeData::Element {
                    attributes,
                    value: value_field,
                    checked: checked_field,
                    ..
                },
            ..
        }) = self.get_mut(id)
        {
            attributes.insert(name.to_string(), value.to_string());
            // Mirrors the HTML `value` content attribute into the real
            // `.value` property — real browsers only do this before the
            // user/JS first touches `.value` (a "dirty value flag" this
            // engine doesn't track); always mirroring is a documented
            // simplification, and matches how the only caller that sets
            // this attribute today (`html5ever`'s tree builder, via
            // `html::sink`) always runs before any script does.
            if name == "value" {
                *value_field = Some(value.to_string());
            }
            // Same mirroring for `.checked` — a boolean attribute's mere
            // presence means `true` regardless of its value string (real
            // HTML semantics for boolean attributes), so any `set_attribute`
            // call for "checked" sets it, never reads `value`.
            if name == "checked" {
                *checked_field = true;
            }
        }
    }

    pub fn attribute(&self, id: NodeId, name: &str) -> Option<&str> {
        match &self.get(id)?.data {
            NodeData::Element { attributes, .. } => attributes.get(name).map(String::as_str),
            _ => None,
        }
    }

    /// Removes an element attribute and returns whether it existed. The
    /// generic JS `Node` binding mirrors the `value` content attribute into
    /// its independent form value, so clearing that attribute clears the
    /// mirror as well.
    pub fn remove_attribute(&mut self, id: NodeId, name: &str) -> bool {
        self.mark_dirty(ATTR_DIRTY);
        if let Some(Node {
            data:
                NodeData::Element {
                    attributes,
                    value: value_field,
                    checked: checked_field,
                    ..
                },
            ..
        }) = self.get_mut(id)
        {
            if name == "value" {
                *value_field = None;
            }
            if name == "checked" {
                *checked_field = false;
            }
            return attributes.remove(name).is_some();
        }
        false
    }

    /// Appends `more` onto an existing `Text` node's content in place —
    /// mirrors the HTML parsing spec's "adjacent text nodes are merged"
    /// rule (`TreeSink::append`'s `AppendText` case), where consecutive
    /// character tokens land in the same DOM text node rather than each
    /// getting their own. No-op if `id` isn't a `Text` node.
    pub fn append_text(&mut self, id: NodeId, more: &str) {
        self.mark_dirty(DirtyFlags::DOM | DirtyFlags::LAYOUT | DirtyFlags::PAINT);
        if let Some(Node {
            data: NodeData::Text(text),
            ..
        }) = self.get_mut(id)
        {
            text.push_str(more);
        }
    }

    /// Depth-first search for the first element whose `id` attribute matches.
    /// Mirrors `document.getElementById`, minus JS-facing identity — the
    /// caller gets a `NodeId`, not a JS object (that needs a DOM node JS
    /// class, which doesn't exist yet).
    pub fn find_by_id(&self, id: &str) -> Option<NodeId> {
        fn walk(dom: &Dom, node: NodeId, id: &str) -> Option<NodeId> {
            let n = dom.get(node)?;
            if let NodeData::Element { attributes, .. } = &n.data {
                if attributes.get("id").map(|v| v.as_str()) == Some(id) {
                    return Some(node);
                }
            }
            for &child in &n.children {
                if let Some(found) = walk(dom, child, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(self, self.root, id)
    }

    /// Concatenation of every Text descendant, in document order — mirrors
    /// `Node.textContent`'s read side.
    pub fn text_content(&self, id: NodeId) -> String {
        fn walk(dom: &Dom, node: NodeId, out: &mut String) {
            let Some(n) = dom.get(node) else { return };
            if let NodeData::Text(text) = &n.data {
                out.push_str(text);
            }
            for &child in &n.children {
                walk(dom, child, out);
            }
        }
        let mut out = String::new();
        walk(self, id, &mut out);
        out
    }

    /// Replaces every child of `id` with a single Text node — mirrors
    /// `Node.textContent`'s write side (`el.textContent = "..."`).
    pub fn set_text_content(&mut self, id: NodeId, text: &str) {
        let children = self.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child in children {
            self.remove(child);
        }
        let text_node = self.create_text(text);
        self.append_child(id, text_node);
    }

    /// Real `HTMLInputElement`/`HTMLTextAreaElement`-shaped `.value` read
    /// side. `id` need not be an `Element` at all — returns `""` for
    /// anything else, same permissive style as [`Dom::attribute`].
    /// A `<textarea>` whose `.value` was never explicitly set (attribute
    /// or JS) falls back to its real text content, mirroring the real DOM
    /// (`<textarea>`'s initial value comes from its children, `<input>`'s
    /// from its `value` attribute — already handled by [`Dom::set_attribute`]
    /// mirroring that attribute in).
    pub fn value(&self, id: NodeId) -> String {
        match self.get(id).map(|n| &n.data) {
            Some(NodeData::Element { value: Some(v), .. }) => v.clone(),
            Some(NodeData::Element {
                tag, value: None, ..
            }) if tag == "textarea" => self.text_content(id),
            _ => String::new(),
        }
    }

    /// Real `.value =` write side — independent of `textContent`/children,
    /// unlike the pre-existing `set_text_content`. No-op on a non-`Element`
    /// node.
    pub fn set_value(&mut self, id: NodeId, value: &str) {
        self.mark_dirty(DirtyFlags::DOM);
        let len = value.chars().count();
        if let Some(Node {
            data:
                NodeData::Element {
                    value: value_field,
                    selection,
                    ..
                },
            ..
        }) = self.get_mut(id)
        {
            *value_field = Some(value.to_string());
            // Real caret behavior: a fresh `.value =` moves the cursor to
            // the end of the new text, same as a real `<input>`/
            // `<textarea>` — matches `setSelectionRange`'s own clamping
            // rule rather than leaving a stale range from the old value.
            *selection = (len, len, "none".to_string());
        }
    }

    /// Real `selectionStart`/`selectionEnd`/`selectionDirection` read side
    /// — `(0, 0, "none")` for a non-`Element` node, matching
    /// [`Dom::value`]'s own "permissive, returns the empty/default shape"
    /// convention for a node this doesn't apply to.
    pub fn selection_range(&self, id: NodeId) -> (usize, usize, String) {
        match self.get(id).map(|n| &n.data) {
            Some(NodeData::Element { selection, .. }) => selection.clone(),
            _ => (0, 0, "none".to_string()),
        }
    }

    /// Real `setSelectionRange(start, end, direction)` write side — clamps
    /// both bounds to `[0, value.chars().count()]` (a real browser clamps
    /// rather than throwing for an out-of-range index) and, per spec, if
    /// `start > end` after clamping, collapses to `(end, end)` rather than
    /// keeping an inverted range. `direction` is stored verbatim (real
    /// values are `"forward"`/`"backward"`/`"none"`; anything else is
    /// still accepted since this generic `Node` class doesn't validate
    /// enum-shaped IDL strings). No-op on a non-`Element` node.
    pub fn set_selection_range(&mut self, id: NodeId, start: usize, end: usize, direction: &str) {
        self.mark_dirty(DirtyFlags::DOM);
        let len = self.value(id).chars().count();
        let start = start.min(len);
        let end = end.min(len);
        let (start, end) = if start > end {
            (end, end)
        } else {
            (start, end)
        };
        if let Some(Node {
            data: NodeData::Element { selection, .. },
            ..
        }) = self.get_mut(id)
        {
            *selection = (start, end, direction.to_string());
        }
    }

    /// Real, independent `.checked` read side — mirrors [`Dom::value`]'s
    /// own doc. `false` for a non-`Element` node.
    pub fn checked(&self, id: NodeId) -> bool {
        matches!(
            self.get(id).map(|n| &n.data),
            Some(NodeData::Element { checked: true, .. })
        )
    }

    /// Real `.checked =` write side — independent of the `checked`
    /// attribute, same shape [`Dom::set_value`] already has for `.value`.
    /// No-op on a non-`Element` node.
    pub fn set_checked(&mut self, id: NodeId, checked: bool) {
        self.mark_dirty(DirtyFlags::DOM);
        if let Some(Node {
            data:
                NodeData::Element {
                    checked: checked_field,
                    ..
                },
            ..
        }) = self.get_mut(id)
        {
            *checked_field = checked;
        }
    }

    /// Per-spec `Node.nodeValue` getter — returns text data for Text/Comment
    /// nodes, `None` for everything else (Element, Document, DocumentFragment).
    pub fn node_value(&self, id: NodeId) -> Option<String> {
        match self.get(id).map(|n| &n.data) {
            Some(NodeData::Text(t)) => Some(t.clone()),
            Some(NodeData::Comment(t)) => Some(t.clone()),
            _ => None,
        }
    }

    /// Per-spec `Node.nodeValue` setter — updates text data for Text/Comment
    /// nodes, no-op for everything else.
    pub fn set_node_value(&mut self, id: NodeId, value: &str) {
        self.mark_dirty(DirtyFlags::DOM | DirtyFlags::LAYOUT | DirtyFlags::PAINT);
        if let Some(Node { data, .. }) = self.get_mut(id) {
            match data {
                NodeData::Text(t) => *t = value.to_string(),
                NodeData::Comment(t) => *t = value.to_string(),
                _ => {}
            }
        }
    }
}
