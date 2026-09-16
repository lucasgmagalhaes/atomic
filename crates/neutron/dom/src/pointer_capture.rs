use crate::{Dom, NodeId};

impl Dom {
    /// Real `Element.setPointerCapture(pointerId)` state: from now on,
    /// `pointerId` is captured by `node` (last caller wins if called again
    /// for the same `pointerId` - real spec semantics: re-capturing just
    /// moves it, no error). No-op if `node` doesn't exist. Unlike
    /// [`Dom::set_hovered`]/[`Dom::focus`], this never bumps
    /// `style_version` - no `:hover`/`:focus`-shaped CSS pseudo-class is
    /// tied to pointer capture.
    pub fn set_pointer_capture(&mut self, pointer_id: i32, node: NodeId) {
        if self.get(node).is_some() {
            self.pointer_captures.insert(pointer_id, node);
        }
    }

    /// Real `Element.releasePointerCapture(pointerId)` state. Returns the
    /// node that was capturing it, if any — `None` both when nothing was
    /// captured for `pointer_id` and when the caller passes a
    /// `pointer_id` never captured, letting a binding fire
    /// `"lostpointercapture"` only when a real release actually happened
    /// (matches real spec: releasing an uncaptured pointer is a no-op, no
    /// event).
    pub fn release_pointer_capture(&mut self, pointer_id: i32) -> Option<NodeId> {
        self.pointer_captures.remove(&pointer_id)
    }

    /// Real `Element.hasPointerCapture(pointerId)` read side.
    pub fn has_pointer_capture(&self, pointer_id: i32, node: NodeId) -> bool {
        self.pointer_captures.get(&pointer_id) == Some(&node)
    }
}
