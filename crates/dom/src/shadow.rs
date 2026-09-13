//! Shadow DOM (`ROADMAP.md` item 38) — `Dom::attach_shadow`/`shadow_root`/
//! `shadow_host`, split out from `mutation.rs`/`query.rs` since this is
//! its own cohesive concern. See `NodeData::ShadowRoot`'s own doc for the
//! scope cut.

use crate::{DirtyFlags, Dom, Node, NodeData, NodeId, ShadowRootMode};

impl Dom {
    /// Real `Element.attachShadow({mode})`: creates a `ShadowRoot` node
    /// and attaches it to `host`. Returns `None` (a no-op, no dirty flags
    /// touched) if `host` doesn't exist, isn't an `Element`, or already
    /// has a shadow root — real spec throws `NotSupportedError` for the
    /// last case; the JS-facing binding is what actually throws, this
    /// low-level primitive just reports failure the same permissive way
    /// [`Dom::replace_child`] reports a failed precondition.
    ///
    /// The new node's `.parent` is set directly to `host` (bypassing
    /// [`Dom::append_child`]) so it's real for tree-walking purposes
    /// (`is_connected`/`contains`/`compare_document_position` all already
    /// walk `.parent` chains and need no changes to see through a shadow
    /// root) without being pushed into `host`'s own `children` — a real
    /// light-DOM traversal (`childNodes`/`children`) must never see it.
    pub fn attach_shadow(&mut self, host: NodeId, mode: ShadowRootMode) -> Option<NodeId> {
        let already_has_one = match self.get(host).map(|n| &n.data) {
            Some(NodeData::Element { shadow_root, .. }) => shadow_root.is_some(),
            _ => return None,
        };
        if already_has_one {
            return None;
        }
        let shadow_id = self.insert(NodeData::ShadowRoot { mode });
        if let Some(node) = self.get_mut(shadow_id) {
            node.parent = Some(host);
        }
        if let Some(Node {
            data: NodeData::Element { shadow_root, .. },
            ..
        }) = self.get_mut(host)
        {
            *shadow_root = Some(shadow_id);
        }
        // Real structural change (a real host now has real, addressable
        // shadow content) — mark the same dirty flags a tree-shape
        // mutation already does, even though nothing in this crate's
        // layout/paint pipeline consumes shadow content yet (see the
        // module's own scope-cut doc); a future consumer that starts
        // walking shadow trees should see this as a normal invalidation,
        // not a silent gap.
        self.mark_dirty(
            DirtyFlags::DOM
                .union(DirtyFlags::LAYOUT)
                .union(DirtyFlags::PAINT),
        );
        self.push_style_invalidation(host, true);
        Some(shadow_id)
    }

    /// `host`'s attached shadow root, if any — `None` for a non-`Element`
    /// or one that never called [`Dom::attach_shadow`].
    pub fn shadow_root(&self, host: NodeId) -> Option<NodeId> {
        match self.get(host).map(|n| &n.data) {
            Some(NodeData::Element { shadow_root, .. }) => *shadow_root,
            _ => None,
        }
    }

    /// `shadow_root`'s host element — its `.parent`, but only meaningful
    /// (and only returned) when `shadow_root` is actually a `ShadowRoot`
    /// node; `None` for anything else, including a plain element (which
    /// has a "parent" in the ordinary tree sense, not a "host").
    pub fn shadow_host(&self, shadow_root: NodeId) -> Option<NodeId> {
        match self.get(shadow_root).map(|n| &n.data) {
            Some(NodeData::ShadowRoot { .. }) => self.get(shadow_root).and_then(|n| n.parent),
            _ => None,
        }
    }

    /// `shadow_root`'s mode (`open`/`closed`) — `None` if `shadow_root`
    /// isn't actually a `ShadowRoot` node.
    pub fn shadow_root_mode(&self, shadow_root: NodeId) -> Option<ShadowRootMode> {
        match self.get(shadow_root).map(|n| &n.data) {
            Some(NodeData::ShadowRoot { mode }) => Some(*mode),
            _ => None,
        }
    }
}
