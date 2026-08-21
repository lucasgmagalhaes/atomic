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
        })
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.insert(NodeData::Text(text.to_string()))
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

    /// Removes `child` from its current parent's children list without freeing it.
    fn detach(&mut self, child: NodeId) {
        let old_parent = self.get(child).and_then(|n| n.parent);
        if let Some(old_parent) = old_parent {
            if let Some(node) = self.get_mut(old_parent) {
                node.children.retain(|&c| c != child);
            }
        }
    }

    /// Removes a node and its whole subtree, freeing slots for reuse (generation bumped).
    pub fn remove(&mut self, id: NodeId) {
        let children = self.get(id).map(|n| n.children.clone()).unwrap_or_default();
        for child in children {
            self.remove(child);
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
            data: NodeData::Element { attributes, .. },
            ..
        }) = self.get_mut(id)
        {
            attributes.insert(name.to_string(), value.to_string());
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_append_sets_parent_and_children() {
        let mut dom = Dom::new();
        let root = dom.root();
        let div = dom.create_element("div");

        dom.append_child(root, div);

        assert_eq!(dom.get(div).unwrap().parent, Some(root));
        assert_eq!(dom.get(root).unwrap().children, vec![div]);
    }

    #[test]
    fn reparent_removes_child_from_old_parent() {
        let mut dom = Dom::new();
        let parent_a = dom.create_element("div");
        let parent_b = dom.create_element("section");
        let child = dom.create_element("span");

        dom.append_child(parent_a, child);
        dom.append_child(parent_b, child);

        assert!(dom.get(parent_a).unwrap().children.is_empty());
        assert_eq!(dom.get(parent_b).unwrap().children, vec![child]);
        assert_eq!(dom.get(child).unwrap().parent, Some(parent_b));
    }

    #[test]
    fn remove_frees_slot_and_invalidates_old_id_via_generation() {
        let mut dom = Dom::new();
        let root = dom.root();
        let div = dom.create_element("div");
        dom.append_child(root, div);

        dom.remove(div);

        assert!(dom.get(div).is_none());
        assert!(dom.get(root).unwrap().children.is_empty());

        // The freed slot gets reused with a bumped generation, so the old id
        // must never alias the new node.
        let reused = dom.create_text("hi");
        assert_ne!(dom.get(reused).unwrap().id, div);
    }

    #[test]
    fn remove_subtree_removes_all_descendants() {
        let mut dom = Dom::new();
        let root = dom.root();
        let parent = dom.create_element("ul");
        let child1 = dom.create_element("li");
        let child2 = dom.create_element("li");
        dom.append_child(root, parent);
        dom.append_child(parent, child1);
        dom.append_child(parent, child2);

        dom.remove(parent);

        assert!(dom.get(parent).is_none());
        assert!(dom.get(child1).is_none());
        assert!(dom.get(child2).is_none());
        assert!(dom.get(root).unwrap().children.is_empty());
    }

    #[test]
    fn find_by_id_locates_nested_element() {
        let mut dom = Dom::new();
        let root = dom.root();
        let wrapper = dom.create_element("div");
        let target = dom.create_element("span");
        dom.append_child(root, wrapper);
        dom.append_child(wrapper, target);
        dom.set_attribute(target, "id", "target");

        assert_eq!(dom.find_by_id("target"), Some(target));
        assert_eq!(dom.find_by_id("missing"), None);
    }

    #[test]
    fn text_content_concatenates_descendant_text_nodes() {
        let mut dom = Dom::new();
        let root = dom.root();
        let p = dom.create_element("p");
        let t1 = dom.create_text("hello ");
        let span = dom.create_element("span");
        let t2 = dom.create_text("world");
        dom.append_child(root, p);
        dom.append_child(p, t1);
        dom.append_child(p, span);
        dom.append_child(span, t2);

        assert_eq!(dom.text_content(p), "hello world");
    }

    #[test]
    fn set_text_content_replaces_all_children() {
        let mut dom = Dom::new();
        let root = dom.root();
        let p = dom.create_element("p");
        let old_child = dom.create_element("span");
        dom.append_child(root, p);
        dom.append_child(p, old_child);

        dom.set_text_content(p, "replaced");

        assert!(dom.get(old_child).is_none());
        assert_eq!(dom.text_content(p), "replaced");
        assert_eq!(dom.get(p).unwrap().children.len(), 1);
    }
}
