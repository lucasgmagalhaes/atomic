use crate::{Dom, NodeData, NodeId};

impl Dom {
    /// Real `Node.prototype.focus()` — becomes `document.activeElement`
    /// (see [`Dom::active_element`]). No-op if `id` doesn't exist. This
    /// only records *which* node is focused; a caller (JS `.focus()`,
    /// `profile-worker`'s coordinate click routing, or its `TAB` command
    /// via [`Dom::next_focus_target`]) decides *when*. Also snapshots the
    /// node's current `.value`, so a later [`Dom::blur`]/[`Dom::clear_focus`]
    /// can tell whether it moved (real `"change"`-event semantics).
    pub fn focus(&mut self, id: NodeId) {
        if self.get(id).is_some() {
            let previous = self.focused;
            self.focused = Some(id);
            self.focused_value_snapshot = Some(self.value(id));
            self.bump_style_version();
            if let Some(previous) = previous {
                self.push_style_invalidation(previous, false);
            }
            self.push_style_invalidation(id, false);
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
            self.bump_style_version();
            self.push_style_invalidation(id, false);
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
        self.bump_style_version();
        self.push_style_invalidation(id, false);
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

    /// Real (scoped) HTML tab order: naturally focusable `<input>`/
    /// `<textarea>` elements, plus any element carrying an explicit
    /// non-negative `tabindex` attribute, walked in document order.
    /// `tabindex="-1"` excludes an element from this list even if it's
    /// otherwise focusable — matches the real DOM (a negative `tabindex`
    /// only removes an element from sequential keyboard navigation, not
    /// from being focusable via a direct `.focus()` call, which this
    /// method has no effect on). Ordering follows the real spec's
    /// two-group rule: elements with a positive `tabindex` come first,
    /// sorted by that value ascending (ties broken by document order),
    /// followed by every `tabindex="0"`/tabindex-less focusable element
    /// in document order. Scoped down from the full spec: this engine has
    /// no generic "is focusable" concept beyond `<input>`/`<textarea>`
    /// (no `<button>`/`<a href>`/`<select>`/`contenteditable`), so a
    /// `tabindex` on any other element still adds it (real browsers do
    /// this too — an explicit `tabindex` makes *any* element focusable),
    /// but a tag outside that pair without one never appears here.
    pub fn tab_order(&self) -> Vec<NodeId> {
        fn tabindex_of(dom: &Dom, id: NodeId) -> Option<i32> {
            dom.attribute(id, "tabindex")
                .and_then(|v| v.trim().parse::<i32>().ok())
        }
        fn is_natural_focusable(dom: &Dom, id: NodeId) -> bool {
            matches!(&dom.get(id).map(|n| &n.data), Some(NodeData::Element { tag, .. }) if tag == "input" || tag == "textarea")
        }
        fn walk(
            dom: &Dom,
            node: NodeId,
            positives: &mut Vec<(i32, NodeId)>,
            defaults: &mut Vec<NodeId>,
        ) {
            let Some(n) = dom.get(node) else { return };
            if matches!(n.data, NodeData::Element { .. }) {
                match tabindex_of(dom, node) {
                    Some(t) if t < 0 => {}
                    Some(t) if t > 0 => positives.push((t, node)),
                    Some(_zero) => defaults.push(node),
                    None => {
                        if is_natural_focusable(dom, node) {
                            defaults.push(node);
                        }
                    }
                }
            }
            for &child in &n.children {
                walk(dom, child, positives, defaults);
            }
        }
        let mut positives = Vec::new();
        let mut defaults = Vec::new();
        walk(self, self.root, &mut positives, &mut defaults);
        positives.sort_by_key(|&(t, _)| t);
        positives
            .into_iter()
            .map(|(_, id)| id)
            .chain(defaults)
            .collect()
    }

    /// What [`Dom::focused`] should move to on a real `Tab`
    /// (`reverse: false`) or `Shift+Tab` (`reverse: true`) press, per
    /// [`Dom::tab_order`]. Cycles: past the last entry wraps to the first
    /// and vice versa. With nothing currently focused — or a focused node
    /// that isn't even in the tab order (e.g. it lost its `tabindex`
    /// since being focused) — `Tab` starts at the first entry and
    /// `Shift+Tab` at the last, matching real browser behavior. `None` if
    /// the tab order is empty. Doesn't itself move focus — a caller
    /// (`profile-worker`'s `TAB` command) still calls `focus`/`blur` (or
    /// their JS-dispatching equivalents) with the result.
    pub fn next_focus_target(&self, reverse: bool) -> Option<NodeId> {
        let order = self.tab_order();
        if order.is_empty() {
            return None;
        }
        let current_index = self
            .active_element()
            .and_then(|id| order.iter().position(|&n| n == id));
        let next_index = match (current_index, reverse) {
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (None, false) => 0,
            (None, true) => order.len() - 1,
        };
        Some(order[next_index])
    }
}
