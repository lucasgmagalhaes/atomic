//! `html5ever::TreeSink` implementation building into a real `dom::Dom`
//! instead of html5ever's own example arena. Modeled closely on
//! html5ever's own `examples/arena.rs` reference sink, adapted to this
//! workspace's arena-based `dom` crate instead of `typed_arena`.
//!
//! Known scope cuts, all because `dom::NodeData` doesn't model them:
//! - `<!DOCTYPE ...>` is parsed (quirks-mode detection still runs) but
//!   discarded — `dom` has no Doctype node variant.
//! - Processing instructions (`<?...?>`, only relevant to XML, not real
//!   HTML) are stored as comment nodes.
//! - `<template>` "template contents" aren't a separate isolated
//!   fragment per spec — `get_template_contents` just returns the
//!   `<template>` element itself, so its content lives as normal
//!   children, visible to normal tree traversal instead of hidden until
//!   cloned. No template-instantiation consumer exists anywhere in this
//!   engine yet, so this doesn't currently matter in practice.
//! - MathML/SVG foreign-content handling isn't implemented
//!   (`is_mathml_annotation_xml_integration_point` always `false`) —
//!   this engine doesn't render SVG/MathML at all, so foreign-content
//!   quirks around them aren't worth modeling yet.
//! - Quirks mode is recorded but nothing downstream reads it.
use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use dom::{Dom, NodeId};
use elsa::FrozenMap;
use html5ever::interface::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::StrTendril;
use html5ever::{Attribute, QualName};

pub struct Sink {
    dom: RefCell<Dom>,
    /// html5ever needs `elem_name()` to return a real `&'a QualName`
    /// (namespace + local name), but `dom::NodeData::Element` only stores
    /// a plain tag `String` - no namespace/atom types. Rather than
    /// changing `dom`'s representation for every element just to satisfy
    /// this one caller, elements' `QualName`s live here instead, keyed by
    /// `NodeId`. `FrozenMap` (append-only, borrows outlive insertion)
    /// avoids the `RefCell<HashMap>` lifetime problem: `elem_name`'s
    /// signature needs to return a reference borrowed from `&'a self`,
    /// which a `Ref<'_, HashMap<..>>` guard can't provide.
    names: FrozenMap<NodeId, Box<QualName>>,
    quirks_mode: Cell<QuirksMode>,
}

impl Sink {
    pub fn new() -> Self {
        Sink {
            dom: RefCell::new(Dom::new()),
            names: FrozenMap::new(),
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
        }
    }

    fn previous_sibling(&self, node: NodeId) -> Option<NodeId> {
        let dom = self.dom.borrow();
        let parent = dom.get(node)?.parent?;
        let siblings = &dom.get(parent)?.children;
        let pos = siblings.iter().position(|&c| c == node)?;
        pos.checked_sub(1).map(|i| siblings[i])
    }

    fn last_child(&self, node: NodeId) -> Option<NodeId> {
        self.dom.borrow().get(node)?.children.last().copied()
    }

    fn is_text_node(&self, node: NodeId) -> bool {
        matches!(
            self.dom.borrow().get(node).map(|n| &n.data),
            Some(dom::NodeData::Text(_))
        )
    }

    /// Shared logic for `append`/`append_before_sibling`: merges adjacent
    /// text per the spec instead of creating a new text node every time,
    /// mirroring html5ever's own reference sink.
    fn append_common(
        &self,
        child: NodeOrText<NodeId>,
        previous: impl FnOnce(&Self) -> Option<NodeId>,
        insert: impl FnOnce(&Self, NodeId),
    ) {
        let new_node = match child {
            NodeOrText::AppendText(text) => {
                if let Some(prev) = previous(self) {
                    if self.is_text_node(prev) {
                        self.dom.borrow_mut().append_text(prev, &text);
                        return;
                    }
                }
                self.dom.borrow_mut().create_text(&text)
            }
            NodeOrText::AppendNode(node) => node,
        };
        insert(self, new_node);
    }
}

impl Default for Sink {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeSink for Sink {
    type Handle = NodeId;
    type Output = Dom;
    type ElemName<'a> = &'a QualName;

    fn finish(self) -> Dom {
        self.dom.into_inner()
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {
        // No error reporting yet - html5ever's own error recovery still
        // produces a usable tree, which is what this crate cares about.
    }

    fn get_document(&self) -> NodeId {
        self.dom.borrow().root()
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> &'a QualName {
        self.names
            .get(target)
            .expect("elem_name called on a non-element handle")
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> NodeId {
        let id = self.dom.borrow_mut().create_element(&name.local);
        for attr in &attrs {
            self.dom
                .borrow_mut()
                .set_attribute(id, &attr.name.local, &attr.value);
        }
        self.names.insert(id, Box::new(name));
        id
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.dom.borrow_mut().create_comment(&text)
    }

    fn create_pi(&self, _target: StrTendril, data: StrTendril) -> NodeId {
        self.dom.borrow_mut().create_comment(&data)
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        let parent = *parent;
        self.append_common(
            child,
            |s| s.last_child(parent),
            |s, node| s.dom.borrow_mut().append_child(parent, node),
        );
    }

    fn append_before_sibling(&self, sibling: &NodeId, child: NodeOrText<NodeId>) {
        let sibling = *sibling;
        self.append_common(
            child,
            |s| s.previous_sibling(sibling),
            |s, node| s.dom.borrow_mut().insert_before(sibling, node),
        );
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let has_parent = self
            .dom
            .borrow()
            .get(*element)
            .and_then(|n| n.parent)
            .is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // dom has no Doctype node - see module docs.
    }

    fn mark_script_already_started(&self, _node: &NodeId) {}

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        *target
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let existing: HashSet<String> = {
            let dom = self.dom.borrow();
            match &dom.get(*target).map(|n| &n.data) {
                Some(dom::NodeData::Element { attributes, .. }) => {
                    attributes.keys().cloned().collect()
                }
                _ => return,
            }
        };
        let mut dom = self.dom.borrow_mut();
        for attr in attrs {
            let name = attr.name.local.to_string();
            if !existing.contains(&name) {
                dom.set_attribute(*target, &name, &attr.value);
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.dom.borrow_mut().remove_from_parent(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let children = self
            .dom
            .borrow()
            .get(*node)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        let mut dom = self.dom.borrow_mut();
        for child in children {
            dom.append_child(*new_parent, child);
        }
    }
}
