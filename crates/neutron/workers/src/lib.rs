//! Web Workers: a real OS thread per worker, each hosting its own
//! `js_runtime::Runtime`/`Context` — genuine parallelism and crash/panic
//! isolation from the caller's thread, not a cooperative/single-threaded
//! simulation.
//!
//! Deliberately simplified message model versus the real
//! `postMessage`/`onmessage` API:
//! - No async `postMessage` calls *from inside* a running handler — a
//!   worker's only way to talk back is the **return value** of its
//!   `onmessage(event)` function (`event.data` is the incoming message,
//!   coerced to a string; the handler's return value, coerced to a
//!   string, is sent back). Real Web Workers can call `postMessage` any
//!   number of times from anywhere, including async callbacks; wiring
//!   that up needs a native `postMessage` binding exposed to the
//!   worker's JS (like `crypto`/`performance` in `js-runtime`), which
//!   doesn't exist yet — this crate doesn't reach into `js-runtime`'s
//!   internals to add one.
//! - No structured clone — messages are plain strings, hand-escaped into
//!   a JS string literal (quotes/backslashes/newlines/tabs/carriage
//!   returns only, not full JSON escaping) rather than real
//!   serialization.
//! - No `Transferable`/`SharedArrayBuffer`, no worker-of-workers nesting
//!   tracked, no `importScripts`.
use std::sync::mpsc;
use std::thread;

use js_runtime::{Context, Runtime};

const TERMINATE_SENTINEL: &str = "\0__atomic_worker_terminate__\0";

fn escape_js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

pub struct Worker {
    inbox: mpsc::Sender<String>,
    outbox: mpsc::Receiver<String>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Worker {
    /// Spawns a new worker thread and evaluates `script` on it
    /// immediately (top-level code runs once, same as a real worker
    /// script - it's expected to define a global `onmessage` function to
    /// actually handle messages).
    pub fn spawn(script: &str) -> Worker {
        let (inbox_tx, inbox_rx) = mpsc::channel::<String>();
        let (outbox_tx, outbox_rx) = mpsc::channel::<String>();
        let script = script.to_string();

        let handle = thread::spawn(move || {
            let runtime = Runtime::new();
            let ctx = Context::new(&runtime);
            let _ = ctx.eval(&script, "<worker>");

            while let Ok(message) = inbox_rx.recv() {
                if message == TERMINATE_SENTINEL {
                    break;
                }
                let call = format!(
                    "(function() {{ \
                        if (typeof onmessage !== 'function') return ''; \
                        var event = {{ data: \"{}\" }}; \
                        var result = onmessage(event); \
                        return result === undefined ? '' : String(result); \
                    }})()",
                    escape_js_string(&message)
                );
                let reply = ctx.eval(&call, "<worker-message>").unwrap_or_default();
                if outbox_tx.send(reply).is_err() {
                    break; // host dropped its Worker handle without terminate()/Drop finishing
                }
            }
        });

        Worker {
            inbox: inbox_tx,
            outbox: outbox_rx,
            handle: Some(handle),
        }
    }

    /// Sends `data` to the worker's `onmessage` handler. Fire-and-forget
    /// from the caller's perspective — read the handler's reply via
    /// [`Worker::recv_message`]/[`Worker::try_recv_message`].
    pub fn post_message(&self, data: &str) {
        let _ = self.inbox.send(data.to_string());
    }

    /// Blocks until the worker's next `onmessage` return value arrives,
    /// or `None` if the worker thread has exited.
    pub fn recv_message(&self) -> Option<String> {
        self.outbox.recv().ok()
    }

    /// Non-blocking version of [`Worker::recv_message`].
    pub fn try_recv_message(&self) -> Option<String> {
        self.outbox.try_recv().ok()
    }

    /// Signals the worker to stop its message loop and waits for the
    /// thread to actually exit. Consumes `self`.
    pub fn terminate(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        let _ = self.inbox.send(TERMINATE_SENTINEL.to_string());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Mirrors profile::Profile's Drop guarantee: a Worker handle
        // going out of scope without an explicit terminate() still stops
        // the thread rather than leaking it.
        self.stop_and_join();
    }
}
