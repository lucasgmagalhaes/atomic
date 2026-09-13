//! `RELOAD`/`NAVIGATE`/`EVAL`/`CONSOLE` — split out from `commands/mod.rs`.

use std::io::Write;

use super::{Handled, PageSource, WorkerState};

impl<'rt> WorkerState<'rt> {
    pub(super) fn handle_page_nav(&mut self, stdout: &mut impl Write, line: &str) -> Handled {
        if line == "RELOAD" {
            if !self.page.ctx.fire_before_unload() {
                let _ = writeln!(stdout, "ERROR navigation canceled by beforeunload");
                let _ = stdout.flush();
                return Handled::Handled;
            }
            let (loaded, error) = super::super::document_load::load_source(
                self.runtime,
                &self.current_source,
                self.width as f64,
                &self.storage_root,
                self.proxy.as_ref(),
                self.dns_server,
            );
            self.page = loaded;
            self.focused_id = None;
            self.scroll_top = 0.0;
            self.sync_scroll();
            self.writer.publish(&self.page.render(
                &self.renderer,
                self.width,
                self.height,
                self.scroll_top,
            ));
            match error {
                Some(message) => {
                    let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                }
                None => {
                    let _ = writeln!(stdout, "RELOADED");
                }
            }
            let _ = stdout.flush();
        } else if let Some(url) = line.strip_prefix("NAVIGATE ") {
            if !self.page.ctx.fire_before_unload() {
                let _ = writeln!(stdout, "ERROR navigation canceled by beforeunload");
                let _ = stdout.flush();
                return Handled::Handled;
            }
            self.current_source = PageSource::Url(url.trim().to_string());
            let (loaded, error) = super::super::document_load::load_source(
                self.runtime,
                &self.current_source,
                self.width as f64,
                &self.storage_root,
                self.proxy.as_ref(),
                self.dns_server,
            );
            self.page = loaded;
            self.focused_id = None;
            self.scroll_top = 0.0;
            self.sync_scroll();
            self.writer.publish(&self.page.render(
                &self.renderer,
                self.width,
                self.height,
                self.scroll_top,
            ));
            match error {
                Some(message) => {
                    let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                }
                None => {
                    let _ = writeln!(stdout, "NAVIGATED");
                }
            }
            let _ = stdout.flush();
        } else if let Some(script) = line.strip_prefix("EVAL ") {
            let script = script.trim();
            // Rendered in both outcomes - a script that mutated the DOM
            // (or an adopted stylesheet) before throwing still changed
            // real state worth painting, same convention CLICK's handler
            // follows when the dispatch itself fails. A NUL byte is
            // rejected up front rather than passed to `Context::eval`,
            // whose `CString::new` conversion panics on one - a hostile/
            // buggy stdin writer must not be able to crash this process.
            if script.contains('\0') {
                let _ = writeln!(stdout, "ERROR script must not contain NUL bytes");
                let _ = stdout.flush();
            } else {
                let result = self.page.ctx.eval(script, "<pane eval>");
                self.sync_scroll();
                self.writer.publish(&self.page.render(
                    &self.renderer,
                    self.width,
                    self.height,
                    self.scroll_top,
                ));
                self.navigate_if_requested();
                match result {
                    Ok(value) => {
                        let _ = writeln!(stdout, "EVALUATED {}", value.replace('\n', " "));
                    }
                    Err(e) => {
                        // The real stringified exception (an Error's own
                        // message, a thrown string verbatim) - a devtools
                        // console shows what threw, not just that
                        // something did.
                        let _ = writeln!(stdout, "ERROR {}", e.0.replace('\n', " "));
                    }
                }
                let _ = stdout.flush();
            }
        } else if line == "CONSOLE" {
            // Drains every `console.*` message the page has produced so
            // far - load-time scripts' output AND anything EVAL'd since
            // (uncaught script errors land in the same buffer, reported
            // by `Context::eval` itself). Read-only as far as rendering
            // goes: no re-render, no frame publish. Draining means a
            // second CONSOLE only returns messages logged after the
            // first - devtools-console semantics, and what bounds this
            // from growing forever across a long session (the buffer is
            // additionally capped inside js-runtime). Message text is
            // flattened (newlines -> spaces, our separator -> slashes)
            // because the reply must be one protocol line.
            let joined = self
                .page
                .ctx
                .take_console_messages()
                .iter()
                .map(|m| {
                    format!(
                        "{}:{}",
                        m.level.as_str(),
                        m.text.replace('\n', " ").replace('|', "/")
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            if joined.is_empty() {
                let _ = writeln!(stdout, "MESSAGES");
            } else {
                let _ = writeln!(stdout, "MESSAGES {joined}");
            }
            let _ = stdout.flush();
        } else {
            return Handled::No;
        }
        Handled::Handled
    }
}
