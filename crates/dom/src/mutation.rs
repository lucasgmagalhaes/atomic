use crate::{Dom, Node, NodeData, NodeId, Slot};

impl Dom {
    pub(crate) fn insert(&mut self, data: NodeData) -> NodeId {
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
            attributes: std::collections::HashMap::new(),
            value: None,
        })
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.insert(NodeData::Text(text.to_string()))
    }

    pub fn create_comment(&mut self, text: &str) -> NodeId {
        self.insert(NodeData::Comment(text.to_string()))
    }

    pub fn create_document_fragment(&mut self) -> NodeId {
        self.insert(NodeData::DocumentFragment)
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.mutations = self.mutations.wrapping_add(1);
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
        self.mutations = self.mutations.wrapping_add(1);
        let parent = self
            .get(sibling)
            .and_then(|n| n.parent)
            .expect("sibling must have a parent");
        self.detach(new_node);
        if let Some(node) = self.get_mut(new_node) {
            node.parent = Some(parent);
        }
        if let Some(node) = self.get_mut(parent) {
            let pos = node
                .children
                .iter()
                .position(|&c| c == sibling)
                .unwrap_or(node.children.len());
            node.children.insert(pos, new_node);
        }
    }

    /// Removes `child` from its current parent's children list without
    /// freeing it — the node and its subtree remain valid and can be
    /// reattached elsewhere. Use [`Dom::remove`] to actually delete a
    /// subtree instead.
    pub fn remove_from_parent(&mut self, child: NodeId) {
        self.mutations = self.mutations.wrapping_add(1);
        self.detach(child);
    }

    pub(crate) fn detach(&mut self, child: NodeId) {
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
        self.mutations = self.mutations.wrapping_add(1);
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

    /// Real `Node.prototype.replaceChild(newChild, oldChild)`: swaps
    /// `old_child` for `new_child` at the same position under `parent`.
    /// Returns `false` and mutates nothing if `old_child` isn't currently
    /// one of `parent`'s children — a checked precondition, same
    /// "no partial mutation on a failed precondition" contract this crate's
    /// JS-facing callers already enforce themselves before calling in (see
    /// `dom_bindings::node_replace_child`, which also rejects `new_child`
    /// being `parent`'s own ancestor — a `HierarchyRequestError` case this
    /// low-level method doesn't check, matching how [`Dom::append_child`]/
    /// [`Dom::insert_before`] leave that check to their JS-facing callers
    /// too). Replacing a node with itself is a real no-op (`true`, nothing
    /// moves) rather than detaching and reinserting the same id. `old_child`
    /// is only unlinked, not destroyed (see [`Dom::remove_from_parent`]) —
    /// it keeps its identity, its own subtree, and stays reattachable, per
    /// `spec/architecture/primitives.md` §4.1.
    pub fn replace_child(&mut self, parent: NodeId, new_child: NodeId, old_child: NodeId) -> bool {
        if new_child == old_child {
            return self
                .get(parent)
                .map(|n| n.children.contains(&old_child))
                .unwrap_or(false);
        }
        let Some(position) = self
            .get(parent)
            .and_then(|node| node.children.iter().position(|&c| c == old_child))
        else {
            return false;
        };
        self.mutations = self.mutations.wrapping_add(1);
        self.detach(new_child);
        self.detach(old_child);
        if let Some(node) = self.get_mut(parent) {
            let insert_pos = position.min(node.children.len());
            node.children.insert(insert_pos, new_child);
        }
        if let Some(node) = self.get_mut(new_child) {
            node.parent = Some(parent);
        }
        true
    }
}
