//! `Profile::click`/`fill`/`click_at`/`type_key`/`tab`/`scroll_by` — split
//! out from `lib.rs`.

use std::io::{BufRead, Write};

use super::Profile;

impl Profile {
    /// Dispatches a real `"click"` event at the element with id `selector`
    /// (only `#id` is accepted — see `profile-worker`'s own doc on why).
    /// `Ok(Ok(()))` means it dispatched (a page-attached `"click"` listener,
    /// if any, actually ran); `Ok(Err(message))` means the worker is still
    /// alive but the click itself failed (no such id, or `selector` wasn't
    /// `#id`-shaped). The outer `io::Result` only covers the protocol
    /// itself failing (a dead worker, a broken pipe) — same convention as
    /// [`navigate`](Self::navigate).
    pub fn click(&mut self, selector: &str) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "CLICK {selector}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Sets the `#id` element's real `.value` (`dom::Dom::value`/
    /// `set_value`, independent of children/text) if it's an
    /// `<input>`/`<textarea>`, `textContent` otherwise — see
    /// `profile-worker`'s own doc on `FILL` for the exact rule. `value`
    /// must not contain a newline (this crate's stdin/stdout protocol is
    /// newline-delimited —
    /// see this struct's own doc comment); a value that does never reaches
    /// the worker, reported the same way a worker-side failure would be
    /// rather than corrupting the command stream.
    pub fn fill(&mut self, selector: &str, value: &str) -> std::io::Result<Result<(), String>> {
        if value.contains('\n') {
            return Ok(Err("fill value must not contain a newline".to_string()));
        }
        writeln!(self.stdin, "FILL {selector} {value}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real coordinate click: `x`/`y` are pixel coordinates in the
    /// profile's own frame (same space `latest_frame`'s pixels are in) -
    /// the worker hit-tests its current layout and dispatches a real
    /// `"click"` on the nearest id-addressable element (see
    /// `profile-worker`'s `nearest_id_ancestor`/`dispatch_click_at` for
    /// the real "only elements with an id, directly or via an ancestor,
    /// are reachable" limitation this implies). If the hit element is
    /// itself a real `<input>`/`<textarea>` with its own id, the worker
    /// also focuses it for a subsequent [`type_key`](Self::type_key) -
    /// that bookkeeping lives worker-side, not surfaced back through this
    /// return value; `Ok(Ok(()))` just means the click reached a real
    /// element. `Ok(Err(message))` means nothing was there to click (no
    /// element at that point, or nothing id-addressable above it).
    pub fn click_at(&mut self, x: f64, y: f64) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "CLICK_AT {x} {y}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real coordinate-driven hover: `x`/`y` are pixel coordinates in the
    /// profile's own frame, same space [`click_at`](Self::click_at) uses.
    /// The worker hit-tests its current layout, updates `dom::Dom`'s real
    /// `:hover` state, and — when the hovered element actually changes —
    /// dispatches a real bubbling `"mouseout"` on the previous
    /// id-addressable target before a real bubbling `"mouseover"` on the
    /// new one (see `profile-worker`'s `dispatch_mouse_move` for the exact
    /// scope cuts: `relatedTarget` stays `null`, and non-bubbling
    /// `mouseenter`/`mouseleave` aren't dispatched). `Ok(Ok(()))` covers
    /// both "moved onto a real element" and "moved off everything" -
    /// there's no real failure mode for a coordinate that hits nothing,
    /// unlike [`click_at`](Self::click_at).
    pub fn mouse_move(&mut self, x: f64, y: f64) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "MOUSE_MOVE {x} {y}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real coordinate-driven drag start: `x`/`y` are pixel coordinates in
    /// the profile's own frame, same space [`click_at`](Self::click_at)
    /// uses. The worker hit-tests its current layout and dispatches a
    /// real cancelable `"dragstart"` `DragEvent` with a working
    /// `.dataTransfer` on the nearest id-addressable ancestor - see
    /// `profile-worker`'s `dispatch_drag_start` for the exact scope cut
    /// (single `"text/plain"` format). If the listener calls
    /// `preventDefault()`, real spec: the drag never starts - `Ok(Ok(()))`
    /// either way (no real failure mode beyond "nothing at that point").
    /// A later [`drop_at`](Self::drop_at) call completes (or rejects) the
    /// drag this starts.
    pub fn drag_start(&mut self, x: f64, y: f64) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "DRAG_START {x} {y}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real coordinate-driven drop: requires a real drag already started
    /// via [`drag_start`](Self::drag_start) (`Ok(Err(message))` if none is
    /// in progress). Hit-tests `(x, y)`, dispatches a real cancelable
    /// `"dragover"` at the target - real spec: only a `dragover` listener
    /// that calls `preventDefault()` marks a valid drop target, so
    /// `Ok(Err(message))` (a real, valid outcome, not a protocol failure)
    /// means the target rejected the drop. `Ok(Ok(()))` means a real
    /// `"drop"` (`dataTransfer.getData` returns the text
    /// [`drag_start`](Self::drag_start) captured) fired on the target,
    /// followed by a real `"dragend"` on the source. Either outcome
    /// clears the in-progress drag - a `drop_at` call (valid or not)
    /// always ends it, same as a real mouse button release does.
    pub fn drop_at(&mut self, x: f64, y: f64) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "DROP_AT {x} {y}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real coordinate-driven right-click: `x`/`y` are pixel coordinates in
    /// the profile's own frame, same space [`click_at`](Self::click_at)
    /// uses. The worker hit-tests its current layout and dispatches a real
    /// bubbling, cancelable `"contextmenu"` `MouseEvent` (`button: 2`) on
    /// the nearest id-addressable ancestor - see `profile-worker`'s
    /// `dispatch_context_menu_at` for the exact scope cut (no focus
    /// side-effect, unlike [`click_at`](Self::click_at)). `Ok(Err(message))`
    /// means nothing was there to right-click.
    pub fn context_menu_at(&mut self, x: f64, y: f64) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "CONTEXT_MENU_AT {x} {y}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real IME composition start: dispatches a real bubbling, cancelable
    /// `"compositionstart"` `CompositionEvent` (`data: ""`) on whichever
    /// real `<input>`/`<textarea>` the most recent
    /// [`click_at`](Self::click_at) focused. `Ok(Err(message))` if nothing
    /// is currently focused.
    pub fn composition_start(&mut self) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "COMPOSITION_START")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real IME composition preview: dispatches a real bubbling,
    /// non-cancelable `"compositionupdate"` `CompositionEvent`
    /// (`data: text`) - no field mutation, matching a real IME only
    /// previewing candidate text mid-composition. `text` must not contain
    /// a newline (this crate's stdin/stdout protocol is
    /// newline-delimited).
    pub fn composition_update(&mut self, text: &str) -> std::io::Result<Result<(), String>> {
        if text.contains('\n') {
            return Ok(Err(
                "composition text must not contain a newline".to_string()
            ));
        }
        writeln!(self.stdin, "COMPOSITION_UPDATE {text}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real IME composition commit: dispatches a real bubbling,
    /// non-cancelable `"compositionend"` `CompositionEvent`
    /// (`data: text`), then the real default action always runs
    /// (real spec: `compositionend` can't be prevented) - appends `text`
    /// to the field's real `.value` and dispatches a real `"input"`
    /// `InputEvent` (`inputType: "insertCompositionText"`). `text` must
    /// not contain a newline, same as [`composition_update`](Self::composition_update).
    pub fn composition_end(&mut self, text: &str) -> std::io::Result<Result<(), String>> {
        if text.contains('\n') {
            return Ok(Err(
                "composition text must not contain a newline".to_string()
            ));
        }
        writeln!(self.stdin, "COMPOSITION_END {text}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Types `key` into whichever real `<input>`/`<textarea>` the most
    /// recent [`click_at`](Self::click_at) focused - `"Backspace"` is a
    /// real delete-last-character, anything else is appended as typed
    /// text, into the element's real `.value` (`dom::Dom::value`/
    /// `set_value`). `Ok(Err(message))` if nothing is currently focused
    /// (never clicked an `<input>`/`<textarea>`, or the page reloaded
    /// since - see `profile-worker`'s `focused_id` reset on
    /// `RELOAD`/`NAVIGATE`) or the focused id no longer exists.
    pub fn type_key(&mut self, key: &str) -> std::io::Result<Result<(), String>> {
        if key.contains('\n') {
            return Ok(Err("key must not contain a newline".to_string()));
        }
        writeln!(self.stdin, "KEY {key}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real `Tab` (`reverse: false`) / `Shift+Tab` (`reverse: true`) focus
    /// movement, per the real (scoped) tab order `dom::Dom::tab_order`
    /// computes worker-side (`<input>`/`<textarea>` plus any element with
    /// an explicit non-negative `tabindex`, positive-`tabindex` group
    /// first). Blurs whatever was focused before and focuses the next
    /// target through the real `.blur()`/`.focus()` JS bindings, so real
    /// `"blur"`/`"change"`/`"focus"` events fire the same as a
    /// [`click_at`](Self::click_at)-driven focus change - a caller can
    /// [`type_key`](Self::type_key) into the newly focused field right
    /// after. Dispatches a real, cancelable `"keydown"` `KeyboardEvent`
    /// (`key: "Tab"`) at the currently focused element (or `document`)
    /// first — `Ok(Err("default action prevented".into()))` if the
    /// page's own listener called `preventDefault()`, same as a real
    /// browser suppressing its own Tab handling. `Ok(Err(message))` also
    /// covers the tab order being empty or every element in it lacking a
    /// real `id` (see `profile-worker`'s `tab_focus` doc for why an `id`
    /// is required to reach a target through this protocol).
    pub fn tab(&mut self, reverse: bool) -> std::io::Result<Result<(), String>> {
        writeln!(
            self.stdin,
            "{}",
            if reverse { "TAB_REVERSE" } else { "TAB" }
        )?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Real page (viewport) scroll: shifts the worker's scroll offset by
    /// `dy` pixels (positive scrolls down, matching a mouse wheel's own
    /// sign convention), clamped worker-side to `[0, content_height -
    /// viewport_height]` (`0` if the page is shorter than the viewport -
    /// nothing to scroll). Every subsequent render/hit-test/`CLICK_AT`
    /// reflects the new offset until the next `SCROLL`, `RELOAD`, or
    /// `NAVIGATE` (which resets it to `0`, a fresh page always starts
    /// scrolled to the top - see `profile-worker`'s own `SCROLL` doc).
    /// No horizontal scroll - this engine's box model has no concept of
    /// content wider than its container to begin with.
    pub fn scroll_by(&mut self, dy: f64) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "SCROLL {dy}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }
}
