use crate::{Dom, NodeData, NodeId};

impl Dom {
    /// Real `Node.prototype.normalize()`: merges adjacent text nodes and
    /// removes empty text nodes throughout this node's subtree (bottom-up).
    pub fn normalize(&mut self, id: NodeId) {
        // First, recursively normalize all children (bottom-up).
        let children = self.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child in children {
            self.normalize(child);
        }

        // Now merge/remove within this node's direct children.
        // Work with a fresh snapshot each pass since removals mutate the vec.
        loop {
            let children = self.get(id).map(|n| n.children.clone()).unwrap_or_default();
            let mut merged = false;
            let mut i = 0;
            while i < children.len() {
                let child = children[i];
                let is_text = self
                    .get(child)
                    .map(|n| matches!(&n.data, NodeData::Text(_)))
                    .unwrap_or(false);
                if is_text {
                    let text = self
                        .get(child)
                        .and_then(|n| match &n.data {
                            NodeData::Text(t) => Some(t.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    // Remove empty text nodes.
                    if text.is_empty() {
                        self.remove(child);
                        merged = true;
                        break;
                    }
                    // Merge with following text siblings.
                    let mut j = i + 1;
                    while j < children.len() {
                        let next = children[j];
                        let next_is_text = self
                            .get(next)
                            .map(|n| matches!(&n.data, NodeData::Text(_)))
                            .unwrap_or(false);
                        if !next_is_text {
                            break;
                        }
                        let next_text = self
                            .get(next)
                            .and_then(|n| match &n.data {
                                NodeData::Text(t) => Some(t.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        self.append_text(child, &next_text);
                        self.remove(next);
                        merged = true;
                        j += 1;
                    }
                    if merged {
                        break;
                    }
                }
                i += 1;
            }
            if !merged {
                break;
            }
        }
    }

    /// Real `Node.prototype.cloneNode(deep)`, same-arena counterpart to
    /// [`Dom::adopt`] (which clones *across* two different arenas — this
    /// clones within `self`). `deep: false` clones only `node` itself
    /// (tag+attributes for an `Element`, full content for `Text`/`Comment`
    /// — nothing left to shallow-clone there) with no children; `deep: true`
    /// clones the whole subtree. The clone's `.value` is re-derived from the
    /// cloned `value` attribute, not copied directly — same documented
    /// simplification [`Dom::adopt`] already makes and for the same reason
    /// (every real caller sets `.value` via the `value` attribute, never
    /// independently, at clone time). A `Document` node isn't a supported
    /// input (mirrors [`Dom::adopt`]'s same restriction): best effort, only
    /// the first child (if any, and only when `deep`) is cloned and
    /// returned, since a single `NodeId` can't represent multiple cloned
    /// roots. An unknown `node` id clones as an empty text node, same
    /// graceful-degradation convention [`Dom::adopt`] uses for a missing
    /// source node.
    pub fn clone_node(&mut self, node: NodeId, deep: bool) -> NodeId {
        let Some(data) = self.get(node).map(|n| n.data.clone()) else {
            return self.create_text("");
        };
        match data {
            NodeData::Element {
                tag, attributes, ..
            } => {
                let new_id = self.create_element(&tag);
                for (name, value) in &attributes {
                    self.set_attribute(new_id, name, value);
                }
                if deep {
                    let children = self
                        .get(node)
                        .map(|n| n.children.clone())
                        .unwrap_or_default();
                    for child in children {
                        let cloned_child = self.clone_node(child, true);
                        self.append_child(new_id, cloned_child);
                    }
                }
                new_id
            }
            NodeData::Text(text) => self.create_text(&text),
            NodeData::Comment(text) => self.create_comment(&text),
            NodeData::DocumentFragment => {
                let new_id = self.create_document_fragment();
                if deep {
                    let children = self
                        .get(node)
                        .map(|n| n.children.clone())
                        .unwrap_or_default();
                    for child in children {
                        let cloned_child = self.clone_node(child, true);
                        self.append_child(new_id, cloned_child);
                    }
                }
                new_id
            }
            NodeData::Document => {
                if deep {
                    let first_child = self.get(node).and_then(|n| n.children.first().copied());
                    if let Some(first_child) = first_child {
                        return self.clone_node(first_child, true);
                    }
                }
                self.create_text("")
            }
        }
    }

    /// Deep-clones `node` and its subtree from a *different* `Dom` (`other`)
    /// into `self`, returning the new root's id in `self`. `NodeId`s are
    /// only valid within the arena that created them, so this is the one
    /// supported way to move content across that boundary (e.g. a fragment
    /// parsed into its own temporary `Dom`). Does not attach the result to
    /// any parent — that's the caller's job via `append_child`.
    ///
    /// `node` being a `Document` is not a supported case for this method's
    /// intended callers (fragment-parse roots are always Element/Text/
    /// Comment) — if it happens, only the first child (if any) is adopted
    /// and returned, since a single `NodeId` can't represent multiple
    /// adopted roots.
    ///
    /// The `value` field is not copied directly: `set_attribute("value", ..)`
    /// naturally re-derives it when the source's `value` attribute is
    /// present, which is simpler than threading the raw field through and
    /// correct for every caller today (fragment parsing never calls
    /// `set_value` independently of the attribute).
    pub fn adopt(&mut self, other: &Dom, node: NodeId) -> NodeId {
        let Some(src) = other.get(node) else {
            return self.create_text("");
        };
        match &src.data {
            NodeData::Element {
                tag, attributes, ..
            } => {
                let new_id = self.create_element(tag);
                for (name, value) in attributes {
                    self.set_attribute(new_id, name, value);
                }
                for &child in &src.children {
                    let cloned_child = self.adopt(other, child);
                    self.append_child(new_id, cloned_child);
                }
                new_id
            }
            NodeData::Text(text) => self.create_text(text),
            NodeData::Comment(text) => self.create_comment(text),
            NodeData::DocumentFragment => {
                let new_id = self.create_document_fragment();
                for &child in &src.children {
                    let cloned_child = self.adopt(other, child);
                    self.append_child(new_id, cloned_child);
                }
                new_id
            }
            NodeData::Document => {
                // Not a supported input per this method's contract; best
                // effort: adopt the first child alone since we can only
                // return one id.
                if let Some(&first_child) = src.children.first() {
                    self.adopt(other, first_child)
                } else {
                    self.create_text("")
                }
            }
        }
    }
}
