use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: usize,
    generation: u32,
}

#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Element {
        tag: String,
        attributes: HashMap<String, String>,
        /// Real, independently-mutable form value — `Some` once either the
        /// `value` attribute or `Dom::set_value`/JS `.value =` has touched
        /// it, `None` beforehand (in which case [`Dom::value`] falls back
        /// to the element's own rules, see that method's doc). Present on
        /// every `Element`, not just `<input>`/`<textarea>` — this engine
        /// has one generic `Node` JS class, not a per-tag
        /// `HTMLInputElement`/`HTMLTextAreaElement` hierarchy, so `.value`
        /// is a documented deviation exposed generically rather than typed
        /// per element.
        value: Option<String>,
    },
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub data: NodeData,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

struct Slot {
    generation: u32,
    node: Option<Node>,
}

pub struct Dom {
    slots: Vec<Slot>,
    free: Vec<usize>,
    root: NodeId,
    /// Real `document.activeElement` state — the one node currently
    /// focused, or `None`. See [`Dom::focus`]/[`Dom::blur`]/
    /// [`Dom::clear_focus`]/[`Dom::active_element`]. No tab order/
    /// `tabindex` model yet: this only tracks *which* node is focused, not
    /// how focus moves between them.
    focused: Option<NodeId>,
    /// `.value` at the moment [`Dom::focus`] was called on `focused`, kept
    /// only so [`Dom::blur`]/[`Dom::clear_focus`] can tell a caller whether
    /// a real `"change"` event should fire (real DOM semantics: `change`
    /// fires on commit — blur after the value actually moved — not on
    /// every keystroke, unlike `input`).
    focused_value_snapshot: Option<String>,
}

impl Default for Dom {
    fn default() -> Self {
        Self::new()
    }
}

impl Dom {
    pub fn new() -> Self {
        let root_id = NodeId {
            index: 0,
            generation: 0,
        };
        let root = Node {
            id: root_id,
            data: NodeData::Document,
            parent: None,
            children: vec![],
        };
        Dom {
            slots: vec![Slot {
                generation: 0,
                node: Some(root),
            }],
            free: vec![],
            root: root_id,
            focused: None,
            focused_value_snapshot: None,
        }
    }

    fn insert(&mut self, data: NodeData) -> NodeId {
        if let Some(index) = self.free.pop() {
            let generation = self.slots[index].generation;
            let id = NodeId { index, generation };
            self.slots[index].node = Some(Node {
                id,
                data,
                parent: None,
                children: vec![],
            });
            id
        } else {
            let index = self.slots.len();
            let id = NodeId {
                index,
                generation: 0,
            };
            self.slots.push(Slot {
                generation: 0,
                node: Some(Node {
                    id,
                    data,
                    parent: None,
                    children: vec![],
                }),
            });
            id
        }
    }

    pub fn create_element(&mut self, tag: &str) -> NodeId {
        self.insert(NodeData::Element {
            tag: tag.to_string(),
            attributes: HashMap::new(),
            value: None,
        })
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.insert(NodeData::Text(text.to_string()))
    }

    pub fn create_comment(&mut self, text: &str) -> NodeId {
        self.insert(NodeData::Comment(text.to_string()))
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        if let Some(node) = self.get_mut(child) {
            node.parent = Some(parent);
        }
        if let Some(node) = self.get_mut(parent) {
            node.children.push(child);
        }
    }

    /// Inserts `new_node` as the sibling immediately before `sibling`,
    /// under `sibling`'s current parent. Panics (via `unwrap`) if
    /// `sibling` has no parent — mirrors `html5ever`'s own contract for
    /// `TreeSink::append_before_sibling`, which never calls this on a
    /// node without one.
    pub fn insert_before(&mut self, sibling: NodeId, new_node: NodeId) {
        let parent = self.get(sibling).and_then(|n| n.parent).expect("sibling must have a parent");
        self.detach(new_node);
        if let Some(node) = self.get_mut(new_node) {
            node.parent = Some(parent);
        }
        if let Some(node) = self.get_mut(parent) {
            let pos = node.children.iter().position(|&c| c == sibling).unwrap_or(node.children.len());
            node.children.insert(pos, new_node);
        }
    }

    /// Removes `child` from its current parent's children list without
    /// freeing it — the node and its subtree remain valid and can be
    /// reattached elsewhere. Use [`Dom::remove`] to actually delete a
    /// subtree instead.
    pub fn remove_from_parent(&mut self, child: NodeId) {
        self.detach(child);
    }

    fn detach(&mut self, child: NodeId) {
        let old_parent = self.get(child).and_then(|n| n.parent);
        if let Some(old_parent) = old_parent {
            if let Some(node) = self.get_mut(old_parent) {
                node.children.retain(|&c| c != child);
            }
        }
        if let Some(node) = self.get_mut(child) {
            node.parent = None;
        }
    }

    /// Removes a node and its whole subtree, freeing slots for reuse (generation bumped).
    pub fn remove(&mut self, id: NodeId) {
        let children = self.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child in children {
            self.remove(child);
        }
        if self.focused == Some(id) {
            self.focused = None;
            self.focused_value_snapshot = None;
        }
        self.detach(id);
        if id.index < self.slots.len() && self.slots[id.index].generation == id.generation {
            self.slots[id.index].node = None;
            self.slots[id.index].generation = self.slots[id.index].generation.wrapping_add(1);
            self.free.push(id.index);
        }
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.slots.get(id.index).and_then(|slot| {
            if slot.generation == id.generation {
                slot.node.as_ref()
            } else {
                None
            }
        })
    }

    fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.slots.get_mut(id.index).and_then(|slot| {
            if slot.generation == id.generation {
                slot.node.as_mut()
            } else {
                None
            }
        })
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn set_attribute(&mut self, id: NodeId, name: &str, value: &str) {
        if let Some(Node {
            data: NodeData::Element { attributes, value: value_field, .. },
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
        }
    }

    pub fn attribute(&self, id: NodeId, name: &str) -> Option<&str> {
        match &self.get(id)?.data {
            NodeData::Element { attributes, .. } => attributes.get(name).map(String::as_str),
            _ => None,
        }
    }

    /// Appends `more` onto an existing `Text` node's content in place —
    /// mirrors the HTML parsing spec's "adjacent text nodes are merged"
    /// rule (`TreeSink::append`'s `AppendText` case), where consecutive
    /// character tokens land in the same DOM text node rather than each
    /// getting their own. No-op if `id` isn't a `Text` node.
    pub fn append_text(&mut self, id: NodeId, more: &str) {
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
            Some(NodeData::Element { tag, value: None, .. }) if tag == "textarea" => self.text_content(id),
            _ => String::new(),
        }
    }

    /// Real `.value =` write side — independent of `textContent`/children,
    /// unlike the pre-existing `set_text_content`. No-op on a non-`Element`
    /// node.
    pub fn set_value(&mut self, id: NodeId, value: &str) {
        if let Some(Node {
            data: NodeData::Element { value: value_field, .. },
            ..
        }) = self.get_mut(id)
        {
            *value_field = Some(value.to_string());
        }
    }

    /// Real `Node.prototype.focus()` — becomes `document.activeElement`
    /// (see [`Dom::active_element`]). No-op if `id` doesn't exist. No real
    /// tab order/`tabindex` model: this only records *which* node is
    /// focused, a caller (JS `.focus()`, or `profile-worker`'s coordinate
    /// click routing) decides *when*. Also snapshots the node's current
    /// `.value`, so a later [`Dom::blur`]/[`Dom::clear_focus`] can tell
    /// whether it moved (real `"change"`-event semantics).
    pub fn focus(&mut self, id: NodeId) {
        if self.get(id).is_some() {
            self.focused = Some(id);
            self.focused_value_snapshot = Some(self.value(id));
        }
    }

    /// Real `Node.prototype.blur()` — clears focus only if `id` is the
    /// currently focused node (matches the real DOM: blurring an element
    /// that isn't focused is a no-op). Returns `None` for that no-op case
    /// (a caller uses this to know a real `"blur"` event must *not*
    /// dispatch — the real DOM doesn't fire one for an already-unfocused
    /// element), `Some(changed)` when it actually blurred, where `changed`
    /// is whether `.value` moved since the matching [`Dom::focus`] call
    /// (a caller uses this to decide whether to also dispatch a real
    /// `"change"` event).
    pub fn blur(&mut self, id: NodeId) -> Option<bool> {
        if self.focused == Some(id) {
            let changed = self.focused_value_snapshot.as_deref() != Some(self.value(id).as_str());
            self.focused = None;
            self.focused_value_snapshot = None;
            Some(changed)
        } else {
            None
        }
    }

    /// Clears focus unconditionally, regardless of which node (if any) is
    /// currently focused — used when a click lands on a non-focusable
    /// element, matching a real browser blurring whatever was focused
    /// before. Returns the node that was blurred, if any, and whether its
    /// `.value` changed since it was focused (see [`Dom::blur`]).
    pub fn clear_focus(&mut self) -> Option<(NodeId, bool)> {
        let id = self.focused?;
        let changed = self.focused_value_snapshot.as_deref() != Some(self.value(id).as_str());
        self.focused = None;
        self.focused_value_snapshot = None;
        Some((id, changed))
    }

    /// Real `document.activeElement` read side. Filters out a stale id
    /// (a focused node later removed from the tree) rather than requiring
    /// every removal path to also clear `focused` — [`Dom::remove`] does
    /// that too as a fast path, but this stays correct even if some future
    /// removal path forgets to.
    pub fn active_element(&self) -> Option<NodeId> {
        self.focused.filter(|&id| self.get(id).is_some())
    }
}
