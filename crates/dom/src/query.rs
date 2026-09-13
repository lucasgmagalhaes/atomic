use crate::{DirtyFlags, Dom, MutationRecord, Node, NodeId};

impl Dom {
    /// See the `mutations` field's own doc — a caller compares two
    /// snapshots of this for equality to know whether anything layout-
    /// relevant changed in between, without diffing the tree itself.
    pub fn mutation_count(&self) -> u64 {
        self.mutations
    }

    /// Returns the accumulated [`DirtyFlags`] since the last drain and
    /// resets them to empty. Consumers should call this once per frame
    /// (or once per script-turn) to see what changed, then skip work
    /// for subsystems whose flag is clear.
    pub fn drain_dirty(&mut self) -> DirtyFlags {
        let flags = self.dirty;
        self.dirty = DirtyFlags::EMPTY;
        flags
    }

    /// Peek at the current dirty flags without resetting them.
    pub fn dirty_flags(&self) -> DirtyFlags {
        self.dirty
    }

    /// How many [`MutationRecord`]s are queued right now, without draining
    /// them — a caller (`js_runtime`'s `mutation_observer::observe`) reads
    /// this when a new observation starts, so it can ignore records that
    /// were already queued *before* that point once the next drain happens
    /// (a `MutationObserver` must never report history from before its own
    /// `observe()` call).
    pub fn pending_mutation_record_count(&self) -> usize {
        self.mutation_records.len()
    }

    /// Returns every [`MutationRecord`] queued since the last call and
    /// empties the queue — same drain-and-reset shape as
    /// [`Dom::drain_dirty`]. A caller (`js_runtime`'s `mutation_observer`
    /// module) calls this once per pump, not per mutation, matching this
    /// crate's general "batch, don't call back synchronously" convention
    /// for anything JS-observable.
    pub fn take_mutation_records(&mut self) -> Vec<MutationRecord> {
        std::mem::take(&mut self.mutation_records)
    }

    /// Version counter that increments only when layout-relevant
    /// mutations occur (the LAYOUT dirty flag is set). Cheaper than
    /// comparing `mutation_count()` for callers that only need to know
    /// "did layout change?" — e.g. `profile-worker`'s layout cache.
    pub fn layout_version(&self) -> u64 {
        self.layout_version
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

    pub(crate) fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
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

    /// Real `Node.prototype.contains(other)`: `true` if `other` is `ancestor`
    /// itself or a descendant of it, walking up from `other` toward the
    /// root rather than down from `ancestor` — cheaper for the common case
    /// (`other` is usually much closer to a leaf than `ancestor` is to the
    /// root) and doesn't need to visit every descendant just to answer a
    /// yes/no question. `false` if `other` doesn't exist or the walk never
    /// reaches `ancestor`.
    pub fn contains(&self, ancestor: NodeId, other: NodeId) -> bool {
        let mut current = Some(other);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.get(id).and_then(|n| n.parent);
        }
        false
    }

    /// Real `Node.prototype.isConnected`: whether `id` is the document root
    /// or a descendant of it, i.e. still attached to the document tree
    /// rather than an orphaned/detached subtree. `false` for a stale or
    /// unknown id too, same permissive style as [`Dom::attribute`].
    pub fn is_connected(&self, id: NodeId) -> bool {
        self.contains(self.root, id)
    }

    /// Real `Node.prototype.compareDocumentPosition(other)`: returns a
    /// bitmask describing the two nodes' relative document position.
    /// Constants match the DOM spec values:
    ///   0x01 = DOCUMENT_POSITION_DISCONNECTED
    ///   0x02 = DOCUMENT_POSITION_PRECEDING
    ///   0x04 = DOCUMENT_POSITION_FOLLOWING
    ///   0x08 = DOCUMENT_POSITION_CONTAINS
    ///   0x10 = DOCUMENT_POSITION_CONTAINED_BY
    pub fn compare_document_position(&self, a: NodeId, b: NodeId) -> i32 {
        const DISCONNECTED: i32 = 0x01;
        const PRECEDING: i32 = 0x02;
        const FOLLOWING: i32 = 0x04;
        const CONTAINS: i32 = 0x08;
        const CONTAINED_BY: i32 = 0x10;

        if self.get(a).is_none() || self.get(b).is_none() {
            return DISCONNECTED | PRECEDING;
        }
        if a == b {
            return 0;
        }

        // Collect ancestor chain for `a` (including a itself).
        let mut chain_a = Vec::new();
        let mut cur = Some(a);
        while let Some(id) = cur {
            chain_a.push(id);
            cur = self.get(id).and_then(|n| n.parent);
        }

        // Walk `b`'s ancestors looking for a common ancestor.
        let mut b_cur = Some(b);
        let mut b_chain = Vec::new();
        while let Some(id) = b_cur {
            b_chain.push(id);
            if let Some(pos) = chain_a.iter().position(|&x| x == id) {
                let common = id;
                let a_child = if pos > 0 { chain_a[pos - 1] } else { a };
                let b_child_pos = b_chain.len().saturating_sub(2);
                let b_child = if b_chain.len() > 1 {
                    b_chain[b_child_pos]
                } else {
                    b
                };

                if common == a {
                    return CONTAINED_BY;
                }
                if common == b {
                    return CONTAINS;
                }

                // Both are descendants of common — compare positions
                // within common's children list.
                if let Some(common_node) = self.get(common) {
                    let children = &common_node.children;
                    let a_pos = children.iter().position(|&c| c == a_child);
                    let b_pos = children.iter().position(|&c| c == b_child);
                    match (a_pos, b_pos) {
                        (Some(ap), Some(bp)) => {
                            return if ap < bp { FOLLOWING } else { PRECEDING };
                        }
                        _ => {
                            return PRECEDING;
                        }
                    }
                } else {
                    return PRECEDING;
                }
            }
            b_cur = self.get(id).and_then(|n| n.parent);
        }

        // No common ancestor — disconnected.
        DISCONNECTED
    }
}
