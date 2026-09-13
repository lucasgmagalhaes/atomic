//! `html_escape`/`js_string_literal` — split out from `chrome_engine.rs`.

/// Minimal HTML-text escaping for interpolating a Rust string (a workspace
/// name) into the innerHTML this module builds — not a general sanitizer,
/// just enough that a name containing `<`/`&` can't break the markup
/// structure `sync_toolbar_state` assembles.
pub(super) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A valid JS double-quoted string literal for `s` — same escaping
/// `profile_worker::input_commands::js_string_literal` uses for embedding
/// an arbitrary Rust string into a small `eval`ed JS snippet.
pub(super) fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
