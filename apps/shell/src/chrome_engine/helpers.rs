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

/// A JS array-of-objects literal (`[{name:"...",active:true},...]`) for
/// `workspaces` — the argument `toolbar.js`'s `renderWorkspaces` expects.
/// Same "Rust builds a valid JS literal, `eval` runs it" approach
/// `js_string_literal` already uses, just for structured data instead of a
/// single string; no `serde_json`/JSON crate needed since this never
/// leaves the process as text meant to be re-parsed as JSON, only
/// evaluated as JS source.
pub(super) fn js_workspace_array(workspaces: &[(String, bool)]) -> String {
    let mut out = String::from("[");
    for (i, (name, active)) in workspaces.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{name:{},active:{active}}}",
            js_string_literal(name)
        ));
    }
    out.push(']');
    out
}

/// A JS array-of-strings literal (`["a","b"]`) — the argument shape
/// `renderCredentials`/`renderBookmarks`/`renderAdapters`' first argument
/// expect. Same non-JSON "valid JS literal, `eval` runs it" approach
/// [`js_workspace_array`] uses.
pub(super) fn js_string_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&js_string_literal(item));
    }
    out.push(']');
    out
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
