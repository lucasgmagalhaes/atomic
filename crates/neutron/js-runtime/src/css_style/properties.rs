//! [`KEBAB_PROPERTIES`] + `kebab_to_camel` — split out from
//! `css_style/mod.rs`.

/// Every inline-style property this module gives a named camelCase
/// accessor, in the exact kebab-case spelling `layout-engine::style`
/// recognizes — keep in sync with that resolver's own property list.
pub(super) const KEBAB_PROPERTIES: &[&str] = &[
    "display",
    "position",
    "top",
    "right",
    "bottom",
    "left",
    "width",
    "height",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "background-color",
    "background",
    "color",
    "font-size",
    "border-width",
    "border-style",
    "border-color",
    "overflow",
    "float",
    "clear",
    "flex-direction",
    "justify-content",
    "align-items",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
];

/// `background-color` -> `backgroundColor`, the same mapping
/// `dom_bindings::kebab_to_camel` already does for `dataset` — duplicated
/// locally rather than shared, matching this crate's convention of small
/// per-module helpers over a shared-utility module (see e.g.
/// `location.rs`/`history.rs`'s own local `read_string`/`new_string`).
pub(super) fn kebab_to_camel(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut capitalize = false;
    for ch in name.chars() {
        if ch == '-' {
            capitalize = true;
            continue;
        }
        if capitalize {
            result.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            result.push(ch);
        }
    }
    result
}
