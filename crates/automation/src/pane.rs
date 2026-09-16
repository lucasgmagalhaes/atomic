//! `pane("name")` → a value with `goto(url)`, `click(selector)`, and
//! `fill(selector, value)`, all real (see this crate's top-level doc for
//! the two scope cuts that carry through from `profile-worker`'s own
//! protocol: `#id`-only selectors, `fill` setting `textContent` not a real
//! `.value`).
//!
//! Built on `rquickjs`'s safe bindings instead of hand-rolled `quickjs-sys`
//! FFI (the pre-CEF-pivot version of this module). Three flat native
//! functions (`__pane_goto`/`__pane_fill`/`__pane_click`, each closing over
//! its own `Rc<RefCell<Panes>>` clone directly — no thread-local registry
//! keyed by a raw `JSContext` pointer needed) back a tiny JS-side `pane()`
//! wrapper (see [`PRELUDE`]) that returns a plain object literal — simpler
//! than building a native `rquickjs::Object` with attached native-closure
//! methods per call, which runs into `rquickjs`'s invariant-lifetime rules
//! for values that must outlive the registering call.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rquickjs::{Ctx, Exception, Function, Result as JsResult};

pub type Panes = HashMap<String, Rc<RefCell<profile::Profile>>>;

/// Evaluated once after the three `__pane_*` natives are registered — see
/// this module's top doc for why the wrapper lives in JS rather than
/// as a native-built `Object`.
pub(crate) const PRELUDE: &str = r#"
globalThis.pane = function(name) {
    return {
        goto: (url) => __pane_goto(name, url),
        fill: (selector, value) => __pane_fill(name, selector, value),
        click: (selector) => __pane_click(name, selector),
    };
};
"#;

/// Flattens `with_pane`'s "no such pane" error, the worker-unreachable I/O
/// error, and the page-level failure message into one JS exception —
/// every `__pane_*` native needs the exact same flattening.
fn resolve<T>(
    ctx: &Ctx<'_>,
    action: &str,
    outcome: Result<std::io::Result<Result<T, String>>, String>,
) -> JsResult<T> {
    match outcome {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(message))) => Err(Exception::throw_message(
            ctx,
            &format!("{action} failed: {message}"),
        )),
        Ok(Err(io_err)) => Err(Exception::throw_message(
            ctx,
            &format!("{action}: worker unreachable: {io_err}"),
        )),
        Err(message) => Err(Exception::throw_message(ctx, &message)),
    }
}

fn with_pane<T>(
    panes: &Rc<RefCell<Panes>>,
    name: &str,
    f: impl FnOnce(&mut profile::Profile) -> std::io::Result<Result<T, String>>,
) -> Result<std::io::Result<Result<T, String>>, String> {
    match panes.borrow().get(name) {
        Some(cell) => Ok(f(&mut cell.borrow_mut())),
        None => Err(format!("no pane named \"{name}\"")),
    }
}

/// Registers the three `__pane_*` globals, closing over `panes`, plus
/// evaluates [`PRELUDE`] to install the JS-side `pane(name)` wrapper.
pub(crate) fn register(ctx: &Ctx<'_>, panes: Rc<RefCell<Panes>>) -> JsResult<()> {
    let globals = ctx.globals();

    let goto_panes = panes.clone();
    globals.set(
        "__pane_goto",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx, name: String, url: String| -> JsResult<()> {
                let outcome = with_pane(&goto_panes, &name, |profile| profile.navigate(&url));
                resolve(&ctx, "pane.goto", outcome)
            },
        ),
    )?;

    let fill_panes = panes.clone();
    globals.set(
        "__pane_fill",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx, name: String, selector: String, value: String| -> JsResult<()> {
                // Only `#id` selectors and a `textContent` assignment, not a
                // real `HTMLInputElement.value` — see `profile-worker`'s own
                // doc on the `FILL` command for why.
                let outcome = with_pane(&fill_panes, &name, |profile| {
                    profile.fill(&selector, &value)
                });
                resolve(&ctx, "pane.fill", outcome)
            },
        ),
    )?;

    let click_panes = panes.clone();
    globals.set(
        "__pane_click",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx, name: String, selector: String| -> JsResult<()> {
                let outcome = with_pane(&click_panes, &name, |profile| profile.click(&selector));
                resolve(&ctx, "pane.click", outcome)
            },
        ),
    )?;

    ctx.eval::<(), _>(PRELUDE)
        .expect("pane prelude is static and must not fail to eval");
    Ok(())
}
