//! `dispatch_drag_start`/`dispatch_drop_at` — split out from
//! `input_commands.rs`. Real coordinate-driven drag-and-drop: mirrors
//! `CLICK_AT`/`MOUSE_MOVE`'s hit-test convention. See this crate's own
//! `WorkerState::drag_source` doc for the state this pair threads
//! through.

use crate::page::Page;

use super::helpers::{js_string_literal, nearest_id_ancestor};

/// This pair's own "the listener canceled the event" signal - same
/// reasoning `input_commands::keyboard::CANCELED_SENTINEL` already
/// documents (a NUL byte doesn't survive the QuickJS-completion-value ->
/// `String` FFI boundary intact).
const CANCELED_SENTINEL: &str = "__ATOMIC_DRAG_CANCELED__";

/// Real `"dragstart"`: hit-tests `(x, y)` via `Page::hit_test_at` (same
/// primitive `CLICK_AT`/`MOUSE_MOVE` use), dispatches a real cancelable
/// `DragEvent` with a working `.dataTransfer` (`setData`/`getData`
/// closures, single `"text/plain"` format - same scope cut
/// `dispatch_copy_or_cut`'s `clipboardData` already documents) on the
/// nearest id-addressable ancestor. If the listener calls
/// `preventDefault()`, real spec: the whole drag is canceled - `Ok(())`
/// with `*drag_source` left untouched (any prior in-progress drag is
/// NOT touched by a canceled *new* `dragstart` either - real semantics:
/// a canceled `dragstart` never began, so an unrelated already-running
/// drag stays as it was). Otherwise stores `(source_id, data)` for a
/// later `dispatch_drop_at` to use.
pub(crate) fn dispatch_drag_start(
    page: &mut Page,
    width: u32,
    height: u32,
    x: f64,
    y: f64,
    scroll_top: f64,
    drag_source: &mut Option<(String, String)>,
) -> Result<(), String> {
    let node = page
        .hit_test_at(width, height, x, y, scroll_top)
        .ok_or("no element at that point")?;
    let source_id = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        nearest_id_ancestor(dom_ref, node)
            .ok_or("no id-addressable element at or above that point")?
    };

    let script = format!(
        "(function(){{ \
            var el = document.getElementById({id}); \
            if (el === null) throw {missing}; \
            var carried = \"\"; \
            var ev = new DragEvent(\"dragstart\", {{ bubbles: true, cancelable: true }}); \
            ev.dataTransfer = {{ \
                setData: function(fmt, text) {{ if (fmt === \"text/plain\") carried = text; }}, \
                getData: function() {{ return \"\"; }} \
            }}; \
            var notCanceled = el.dispatchEvent(ev); \
            if (!notCanceled) return {canceled_sentinel}; \
            return carried; \
        }})();",
        id = js_string_literal(&source_id),
        missing = js_string_literal(&format!("no element with id \"{source_id}\"")),
        canceled_sentinel = js_string_literal(CANCELED_SENTINEL),
    );
    let result = page.ctx.eval(&script, "<pane drag>").map_err(|_| {
        format!("no element with id \"{source_id}\" (or its dragstart handler threw)")
    })?;
    if result != CANCELED_SENTINEL {
        *drag_source = Some((source_id, result));
    }
    Ok(())
}

/// Real `"dragover"`/`"drop"`/`"dragend"`. Requires a real drag already
/// in progress (`Err` otherwise - matches this engine's other "nothing
/// focused"-shaped preconditions, e.g. `KEY` without a prior `CLICK_AT`).
/// Hit-tests `(x, y)`, dispatches a real cancelable `"dragover"` at the
/// target - real spec: a drop is only valid when a `dragover` listener
/// calls `preventDefault()`, so `Ok(false)` (no error - a real, valid
/// outcome) means the target rejected the drop. `Ok(true)` means a real
/// `"drop"` (cancelable, `dataTransfer.getData` returns the text
/// `dispatch_drag_start` captured) fired on the target, followed by a
/// real, non-cancelable `"dragend"` on the source - `drag_source` is
/// cleared either way this function returns `Ok`, since a `DROP_AT` call
/// (valid or not) always ends the in-progress drag, same as a real mouse
/// button release does.
pub(crate) fn dispatch_drop_at(
    page: &mut Page,
    width: u32,
    height: u32,
    x: f64,
    y: f64,
    scroll_top: f64,
    drag_source: &mut Option<(String, String)>,
) -> Result<bool, String> {
    let Some((source_id, data)) = drag_source.take() else {
        return Err("no drag in progress - call DRAG_START first".to_string());
    };

    let node = page
        .hit_test_at(width, height, x, y, scroll_top)
        .ok_or("no element at that point")?;
    let target_id = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        nearest_id_ancestor(dom_ref, node)
            .ok_or("no id-addressable element at or above that point")?
    };

    let dragover_script = format!(
        "(function(){{ \
            var el = document.getElementById({id}); \
            if (el === null) throw {missing}; \
            var ev = new DragEvent(\"dragover\", {{ bubbles: true, cancelable: true }}); \
            ev.dataTransfer = {{ getData: function() {{ return \"\"; }}, setData: function() {{}} }}; \
            return el.dispatchEvent(ev); \
        }})();",
        id = js_string_literal(&target_id),
        missing = js_string_literal(&format!("no element with id \"{target_id}\"")),
    );
    let not_canceled = page
        .ctx
        .eval(&dragover_script, "<pane drag>")
        .map_err(|_| {
            format!("no element with id \"{target_id}\" (or its dragover handler threw)")
        })?;
    if not_canceled != "false" {
        return Ok(false);
    }

    let drop_script = format!(
        "(function(){{ \
            var el = document.getElementById({id}); \
            if (el === null) throw {missing}; \
            var ev = new DragEvent(\"drop\", {{ bubbles: true, cancelable: true }}); \
            ev.dataTransfer = {{ getData: function(fmt) {{ return fmt === \"text/plain\" ? {data} : \"\"; }}, setData: function() {{}} }}; \
            el.dispatchEvent(ev); \
        }})();",
        id = js_string_literal(&target_id),
        missing = js_string_literal(&format!("no element with id \"{target_id}\"")),
        data = js_string_literal(&data),
    );
    page.ctx
        .eval(&drop_script, "<pane drag>")
        .map_err(|_| format!("no element with id \"{target_id}\" (or its drop handler threw)"))?;

    let dragend_script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el !== null) el.dispatchEvent(new DragEvent(\"dragend\", {{ bubbles: true, cancelable: false }})); }})();",
        id = js_string_literal(&source_id),
    );
    page.ctx
        .eval(&dragend_script, "<pane drag>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{source_id}\""))?;

    Ok(true)
}
