//! Inline `style` attribute parsing/serialization/get/write — split out
//! from `css_style/mod.rs`.

/// Parses `text` (an inline `style` attribute value) into an ordered list
/// of `(property, value)` pairs — see the module doc for the plain
/// `;`/`:`-splitting scope cut. Malformed segments (no `:`, an empty name/
/// value) are silently skipped rather than erroring, matching how a real
/// browser tolerates garbage in a `style` attribute rather than throwing.
pub(super) fn parse_declarations(text: &str) -> Vec<(String, String)> {
    text.split(';')
        .filter_map(|part| {
            let mut split = part.splitn(2, ':');
            let name = split.next()?.trim();
            let value = split.next()?.trim();
            if name.is_empty() || value.is_empty() {
                None
            } else {
                Some((name.to_string(), value.to_string()))
            }
        })
        .collect()
}

pub(super) fn serialize_declarations(decls: &[(String, String)]) -> String {
    decls
        .iter()
        .map(|(name, value)| format!("{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The last declared value for `name` in `id`'s `style` attribute, or
/// `None` if absent — `rev()` so a later duplicate declaration for the same
/// property (real, if rare, CSS: `color: red; color: blue;`) wins, matching
/// cascade-within-one-declaration-block semantics.
pub(super) unsafe fn get_declaration(
    dom: *const dom::Dom,
    id: dom::NodeId,
    name: &str,
) -> Option<String> {
    let text = (*dom).attribute(id, "style").unwrap_or_default();
    parse_declarations(text)
        .into_iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v)
}

/// Sets (`Some(value)`) or removes (`None`) `name` in `id`'s `style`
/// attribute, rewriting the whole attribute — every existing declaration
/// for `name` is dropped first (not just the last one), so a garbled
/// `color:red;color:blue` collapses to a single clean declaration on the
/// next write.
pub(super) unsafe fn write_declaration(
    dom: *mut dom::Dom,
    id: dom::NodeId,
    name: &str,
    value: Option<&str>,
) {
    let text = (*dom)
        .attribute(id, "style")
        .unwrap_or_default()
        .to_string();
    let mut decls = parse_declarations(&text);
    decls.retain(|(n, _)| n != name);
    if let Some(value) = value {
        decls.push((name.to_string(), value.to_string()));
    }
    (*dom).set_attribute(id, "style", &serialize_declarations(&decls));
}
