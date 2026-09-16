//! `rewrite_glsl_es_version` — split out from `context.rs`. See the
//! parent module doc for why this rewrite exists.

use std::borrow::Cow;

/// Rewrites a leading `#version <N> es` directive (GLSL ES 3.00/3.10/3.20)
/// to `#version 450 core`, the one naga's GLSL frontend actually accepts —
/// see the module doc for why nothing past that line needs to change.
/// Source with no ES version directive (already desktop GLSL, or malformed
/// - naga will report the real error either way) passes through untouched.
pub(super) fn rewrite_glsl_es_version(source: &str) -> Cow<'_, str> {
    let Some(first_line) = source.lines().next() else {
        return Cow::Borrowed(source);
    };
    let trimmed = first_line.trim();
    let Some(rest) = trimmed.strip_prefix("#version") else {
        return Cow::Borrowed(source);
    };
    let Some(number) = rest.trim().strip_suffix("es").map(str::trim) else {
        return Cow::Borrowed(source);
    };
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return Cow::Borrowed(source);
    }

    let mut rewritten = String::with_capacity(source.len());
    rewritten.push_str("#version 450 core");
    rewritten.push_str(&source[first_line.len()..]);
    Cow::Owned(rewritten)
}
