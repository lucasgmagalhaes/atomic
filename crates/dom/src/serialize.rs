use crate::{Dom, NodeData, NodeId};

impl Dom {
    /// HTML-escapes text content: `&`/`<`/`>` at minimum, enough for a round
    /// trip through `crates/html`'s parser to reproduce the same text.
    fn escape_text(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    /// HTML-escapes an attribute value for double-quoted emission — only
    /// `"`/`&` matter since we always quote with `"`.
    fn escape_attr(value: &str) -> String {
        value.replace('&', "&amp;").replace('"', "&quot;")
    }

    /// Serializes one node's own representation (tag+attributes+children for
    /// an `Element`, escaped text/comment markup otherwise) into `out`.
    /// Shared by [`Dom::serialize_children`]/[`Dom::serialize_node`] so
    /// neither duplicates the other's per-kind logic.
    fn serialize_into(&self, id: NodeId, out: &mut String) {
        let Some(node) = self.get(id) else { return };
        match &node.data {
            NodeData::Text(text) => out.push_str(&Self::escape_text(text)),
            NodeData::Comment(text) => {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
            NodeData::Element {
                tag, attributes, ..
            } => {
                out.push('<');
                out.push_str(tag);
                // HashMap has no ordering; sort by name for determinism —
                // same convention as `dom_bindings::sync_attributes`. Real
                // HTML preserves authored order, which this storage can't.
                let mut names: Vec<&String> = attributes.keys().collect();
                names.sort_unstable();
                for name in names {
                    out.push(' ');
                    out.push_str(name);
                    out.push_str("=\"");
                    out.push_str(&Self::escape_attr(&attributes[name]));
                    out.push('"');
                }
                out.push('>');
                for &child in &node.children {
                    self.serialize_into(child, out);
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
            NodeData::Document | NodeData::DocumentFragment => {
                for &child in &node.children {
                    self.serialize_into(child, out);
                }
            }
        }
    }

    /// Serializes every child of `id` as HTML, in document order — the read
    /// side of `element.innerHTML`. No void-element list exists anywhere in
    /// this crate, so an element always gets both open and close tags (e.g.
    /// `<br></br>`), a documented scope cut rather than real HTML semantics.
    pub fn serialize_children(&self, id: NodeId) -> String {
        let mut out = String::new();
        if let Some(node) = self.get(id) {
            for &child in &node.children {
                self.serialize_into(child, &mut out);
            }
        }
        out
    }

    /// Serializes `id` itself, tag/attributes included — the read side of
    /// `element.outerHTML`. For a `Text`/`Comment` node this is the same
    /// markup `serialize_children` would produce for it as a child.
    pub fn serialize_node(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.serialize_into(id, &mut out);
        out
    }
}
