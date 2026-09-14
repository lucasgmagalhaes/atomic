//! `type_key` — split out from `input_commands.rs`.

use js_runtime::Context;

use super::helpers::js_string_literal;

/// `dispatch_copy_or_cut`'s "the listener canceled the event" signal.
/// A NUL character was tried first and rejected: the FFI boundary that
/// turns a QuickJS completion value into this crate's `String` truncates
/// at the first NUL (C-string convention), so a canceled dispatch's NUL
/// sentinel came back indistinguishable from a real empty clipboard value.
/// This ASCII marker survives that boundary intact, and is astronomically
/// unlikely to collide with real copied text.
const CANCELED_SENTINEL: &str = "__ATOMIC_COPY_CUT_CANCELED__";

/// Types `key` into whichever real `<input>`/`<textarea>` id `CLICK_AT`
/// most recently focused (see `dispatch_click_at`'s doc) - `"Backspace"`
/// removes the field's real last character, anything else is appended
/// verbatim as typed text. Writes the element's real `.value`
/// (`dom::Dom::value`/`set_value`) now, not `textContent` — a real
/// `"keydown"` event still dispatches first via the existing
/// `dispatchEvent` binding, so a page's own `keydown` listener genuinely
/// runs, same as a real browser firing the event before applying the
/// default action. `"Ctrl+C"`/`"Ctrl+X"`/`"Ctrl+V"` are recognized combo
/// strings that dispatch real clipboard events instead - see
/// `dispatch_copy_or_cut`/`dispatch_paste`.
pub(crate) fn type_key(ctx: &Context, focused_id: &str, key: &str) -> Result<(), String> {
    match key {
        "Ctrl+C" => return dispatch_copy_or_cut(ctx, focused_id, "copy"),
        "Ctrl+X" => return dispatch_copy_or_cut(ctx, focused_id, "cut"),
        "Ctrl+V" => return dispatch_paste(ctx, focused_id),
        _ => {}
    }
    let current = {
        let dom_ref = ctx.dom().ok_or("no DOM available")?;
        let node = dom_ref
            .find_by_id(focused_id)
            .ok_or_else(|| format!("no element with id \"{focused_id}\""))?;
        dom_ref.value(node)
    };
    let updated = if key == "Backspace" {
        let mut chars: Vec<char> = current.chars().collect();
        chars.pop();
        chars.into_iter().collect()
    } else {
        format!("{current}{key}")
    };
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(\"keydown\"); el.value = {value}; }})();",
        id = js_string_literal(focused_id),
        missing = js_string_literal(&format!("no element with id \"{focused_id}\"")),
        value = js_string_literal(&updated),
    );
    ctx.eval(&script, "<pane key>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{focused_id}\" (or its keydown handler threw)"))
}

/// Real `"copy"`/`"cut"`: dispatches a real cancelable plain `Event` with
/// a working `.clipboardData` (a plain object built inline in the
/// generated script - `setData`/`getData` closures, not a new
/// `ClipboardEvent` native class), captures whatever the listener passed
/// to `setData("text/plain", ...)` as this script's completion value, and
/// - only if the listener didn't cancel the event *and* never called
/// `setData` itself - falls back to the real default action (copying the
/// whole field's current value; scope cut: no partial-selection copy,
/// same as this engine's other documented "no user-driven selection"
/// cuts). Writes the final text to the real OS clipboard via
/// `platform_apis::clipboard_write_text` - same crate
/// `js-runtime`'s own `navigator.clipboard` uses, called directly here
/// since `KEY`'s round trip is synchronous (an async
/// `navigator.clipboard.writeText` Promise wouldn't resolve before this
/// function already replied). `"cut"` additionally clears the field after
/// a real, uncanceled dispatch.
fn dispatch_copy_or_cut(ctx: &Context, focused_id: &str, kind: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ \
            var el = document.getElementById({id}); \
            if (el === null) throw {missing}; \
            var copied = null; \
            var ev = new Event({kind}, {{ bubbles: true, cancelable: true }}); \
            ev.clipboardData = {{ \
                setData: function(fmt, text) {{ if (fmt === \"text/plain\") copied = text; }}, \
                getData: function() {{ return \"\"; }} \
            }}; \
            var notCanceled = el.dispatchEvent(ev); \
            if (!notCanceled) return {canceled_sentinel}; \
            if (copied === null) copied = el.value; \
            return copied; \
        }})();",
        id = js_string_literal(focused_id),
        missing = js_string_literal(&format!("no element with id \"{focused_id}\"")),
        kind = js_string_literal(kind),
        canceled_sentinel = js_string_literal(CANCELED_SENTINEL),
    );
    let result = ctx.eval(&script, "<pane clipboard>").map_err(|_| {
        format!("no element with id \"{focused_id}\" (or its {kind} handler threw)")
    })?;
    // `CANCELED_SENTINEL`'s own doc explains why this isn't a NUL byte.
    // Real `dispatchEvent` returns a bool, but `ctx.eval`'s return channel
    // is the script's stringified completion value, not a typed one (see
    // `Context::eval`'s own doc) - real default action (writing to the OS
    // clipboard, clearing on cut) only runs when the sentinel is absent.
    if result == CANCELED_SENTINEL {
        return Ok(());
    }
    let _ = platform_apis::clipboard_write_text(&result);
    if kind == "cut" {
        let clear_script = format!(
            "(function(){{ var el = document.getElementById({id}); if (el !== null) el.value = \"\"; }})();",
            id = js_string_literal(focused_id),
        );
        ctx.eval(&clear_script, "<pane clipboard>")
            .map(|_| ())
            .map_err(|_| format!("no element with id \"{focused_id}\""))?;
    }
    Ok(())
}

/// Real `"paste"`: reads the real OS clipboard text via
/// `platform_apis::clipboard_read_text` (empty string on any real read
/// error, e.g. an empty clipboard) *before* building the script, so
/// `.clipboardData.getData("text/plain")` returns the real value
/// synchronously - dispatches a real cancelable plain `Event`, and if not
/// canceled, appends that text to the field's `.value` (append, not
/// cursor-position insert - same convention `type_key`'s own regular
/// character handling already uses).
fn dispatch_paste(ctx: &Context, focused_id: &str) -> Result<(), String> {
    let clip_text = platform_apis::clipboard_read_text().unwrap_or_default();
    let script = format!(
        "(function(){{ \
            var el = document.getElementById({id}); \
            if (el === null) throw {missing}; \
            var clipText = {clip_text}; \
            var ev = new Event(\"paste\", {{ bubbles: true, cancelable: true }}); \
            ev.clipboardData = {{ \
                getData: function(fmt) {{ return fmt === \"text/plain\" ? clipText : \"\"; }}, \
                setData: function() {{}} \
            }}; \
            var notCanceled = el.dispatchEvent(ev); \
            if (notCanceled) el.value = el.value + clipText; \
        }})();",
        id = js_string_literal(focused_id),
        missing = js_string_literal(&format!("no element with id \"{focused_id}\"")),
        clip_text = js_string_literal(&clip_text),
    );
    ctx.eval(&script, "<pane clipboard>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{focused_id}\" (or its paste handler threw)"))
}
