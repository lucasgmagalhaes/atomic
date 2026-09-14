//! `CLICK`/`CLICK_AT`/`KEY`/`TAB`/`TAB_REVERSE`/`FILL`/`SCROLL` — split out
//! from `commands/mod.rs`.

use std::io::Write;

use super::super::input_commands::{
    dispatch_click, dispatch_click_at, dispatch_mouse_move, fill_element, tab_focus, type_key,
    TabOutcome,
};
use super::{Handled, WorkerState};

impl<'rt> WorkerState<'rt> {
    pub(super) fn handle_input(&mut self, stdout: &mut impl Write, line: &str) -> Handled {
        if let Some(rest) = line.strip_prefix("CLICK ") {
            let result = dispatch_click(&self.page.ctx, rest.trim());
            self.sync_scroll();
            self.writer.publish(&self.page.render(
                &self.renderer,
                self.width,
                self.height,
                self.scroll_top,
            ));
            self.navigate_if_requested();
            match result {
                Ok(()) => {
                    let _ = writeln!(stdout, "CLICKED");
                }
                Err(message) => {
                    let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                }
            }
            let _ = stdout.flush();
        } else if let Some(rest) = line.strip_prefix("CLICK_AT ") {
            let mut parts = rest.split_whitespace();
            let coords = parts
                .next()
                .zip(parts.next())
                .and_then(|(x, y)| Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?)));
            match coords {
                Some((x, y)) => {
                    let result = dispatch_click_at(
                        &mut self.page,
                        self.width,
                        self.height,
                        x,
                        y,
                        self.scroll_top,
                        &mut self.last_click,
                    );
                    self.sync_scroll();
                    self.writer.publish(&self.page.render(
                        &self.renderer,
                        self.width,
                        self.height,
                        self.scroll_top,
                    ));
                    self.navigate_if_requested();
                    match result {
                        Ok(focus) => {
                            self.focused_id = focus;
                            let _ = writeln!(stdout, "CLICKED");
                        }
                        Err(message) => {
                            let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                        }
                    }
                }
                None => {
                    let _ = writeln!(stdout, "ERROR CLICK_AT requires two numeric coordinates");
                }
            }
            let _ = stdout.flush();
        } else if let Some(rest) = line.strip_prefix("MOUSE_MOVE ") {
            let mut parts = rest.split_whitespace();
            let coords = parts
                .next()
                .zip(parts.next())
                .and_then(|(x, y)| Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?)));
            match coords {
                Some((x, y)) => {
                    let result = dispatch_mouse_move(
                        &mut self.page,
                        self.width,
                        self.height,
                        x,
                        y,
                        self.scroll_top,
                    );
                    self.sync_scroll();
                    self.writer.publish(&self.page.render(
                        &self.renderer,
                        self.width,
                        self.height,
                        self.scroll_top,
                    ));
                    self.navigate_if_requested();
                    match result {
                        Ok(()) => {
                            let _ = writeln!(stdout, "MOVED");
                        }
                        Err(message) => {
                            let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                        }
                    }
                }
                None => {
                    let _ = writeln!(stdout, "ERROR MOUSE_MOVE requires two numeric coordinates");
                }
            }
            let _ = stdout.flush();
        } else if let Some(key) = line.strip_prefix("KEY ") {
            match &self.focused_id {
                Some(id) => {
                    let result = type_key(&self.page.ctx, id, key);
                    self.sync_scroll();
                    self.writer.publish(&self.page.render(
                        &self.renderer,
                        self.width,
                        self.height,
                        self.scroll_top,
                    ));
                    match result {
                        Ok(()) => {
                            let _ = writeln!(stdout, "TYPED");
                        }
                        Err(message) => {
                            let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                        }
                    }
                }
                None => {
                    let _ = writeln!(
                        stdout,
                        "ERROR no element focused - CLICK_AT an <input>/<textarea> first"
                    );
                }
            }
            let _ = stdout.flush();
        } else if line == "TAB" || line == "TAB_REVERSE" {
            let reverse = line == "TAB_REVERSE";
            let result = tab_focus(&mut self.page, reverse);
            self.sync_scroll();
            self.writer.publish(&self.page.render(
                &self.renderer,
                self.width,
                self.height,
                self.scroll_top,
            ));
            match result {
                Ok(TabOutcome::Moved(id)) => {
                    self.focused_id = Some(id);
                    let _ = writeln!(stdout, "TABBED");
                }
                Ok(TabOutcome::NothingFocusable) => {
                    let _ = writeln!(stdout, "ERROR no focusable element with an id on this page");
                }
                Ok(TabOutcome::DefaultPrevented) => {
                    let _ = writeln!(stdout, "ERROR default action prevented");
                }
                Err(message) => {
                    let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                }
            }
            let _ = stdout.flush();
        } else if let Some(rest) = line.strip_prefix("FILL ") {
            let mut parts = rest.splitn(2, ' ');
            let selector = parts.next().unwrap_or("").trim();
            let value = parts.next().unwrap_or("");
            let result = fill_element(&self.page.ctx, selector, value);
            self.sync_scroll();
            self.writer.publish(&self.page.render(
                &self.renderer,
                self.width,
                self.height,
                self.scroll_top,
            ));
            match result {
                Ok(()) => {
                    let _ = writeln!(stdout, "FILLED");
                }
                Err(message) => {
                    let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                }
            }
            let _ = stdout.flush();
        } else if let Some(rest) = line.strip_prefix("SCROLL ") {
            match rest.trim().parse::<f64>() {
                Ok(dy) => {
                    self.page.ctx.set_scroll_y(self.page.ctx.scroll_y() + dy);
                    self.sync_scroll();
                    self.writer.publish(&self.page.render(
                        &self.renderer,
                        self.width,
                        self.height,
                        self.scroll_top,
                    ));
                    let _ = writeln!(stdout, "SCROLLED");
                }
                Err(_) => {
                    let _ = writeln!(stdout, "ERROR SCROLL requires a numeric delta");
                }
            }
            let _ = stdout.flush();
        } else {
            return Handled::No;
        }
        Handled::Handled
    }
}
