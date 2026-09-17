//! The pane grid: click/scroll/keyboard routing into a pane's real DOM,
//! the per-pane context menu, texture paint, and the resource overlay —
//! now composited on top of the chrome browser's own texture (see
//! `spec/architecture/chrome-ui.md`) instead of a bare `egui` background,
//! at rects the chrome page's own CSS grid actually laid out
//! (`ChromeUi::pane_rects`) rather than `tiling::grid_layout`'s own
//! auto-tiling algorithm - the chrome page is the single source of truth
//! for where a pane goes now, same reasoning that module's own doc
//! documents for why Rust doesn't hand-compute the CSS grid math.

use atomic::i18n;
use atomic::tiling;

use super::resource_overlay::draw_resource_overlay;
use super::{AtomicApp, CHROME_HEIGHT, CHROME_WIDTH, PANE_HEIGHT, PANE_WIDTH};

impl AtomicApp {
    pub(super) fn draw_pane_grid(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            let available = ui.available_rect_before_wrap();

            // The chrome texture is rendered at a fixed CHROME_WIDTH x
            // CHROME_HEIGHT (see ChromeUi::spawn's own doc on why - no
            // live resize yet) but displayed scaled to fill whatever the
            // real window size is - every rect the chrome page itself
            // reports via `pane_rects()` is in its own unscaled CSS-pixel
            // space, so it must be scaled by the same factor before it
            // means anything in this window's real screen coordinates.
            if let Some(texture) = self.chrome.poll_texture(ui.ctx()) {
                ui.put(available, egui::Image::new((texture.id(), available.size())));
            }
            let scale_x = available.width() / CHROME_WIDTH as f32;
            let scale_y = available.height() / CHROME_HEIGHT as f32;
            let to_screen = |r: atomic::chrome_ui::PaneRect| tiling::Rect {
                x: available.min.x + r.x * scale_x,
                y: available.min.y + r.y * scale_y,
                width: r.width * scale_x,
                height: r.height * scale_y,
            };

            // Only the active workspace's panes - see
            // `active_workspace_pane_indices`'s doc. A pane belonging to a
            // different (inactive) workspace keeps running but isn't
            // drawn or ticked here.
            let visible = self.active_workspace_pane_indices();
            let chrome_rects = self.chrome.pane_rects();
            let cells: Vec<tiling::Rect> = if chrome_rects.len() >= visible.len() && !visible.is_empty() {
                chrome_rects.into_iter().take(visible.len()).map(to_screen).collect()
            } else {
                // The chrome page hasn't rendered a matching pane count
                // yet (e.g. this frame's state push hasn't taken effect
                // in the page's own render cycle) - fall back to the old
                // auto-tiling algorithm for this one frame rather than
                // drawing nothing.
                let container = tiling::Rect { x: available.min.x, y: available.min.y, width: available.width(), height: available.height() };
                tiling::grid_layout(container, visible.len())
            };

            // Real click/scroll/key routing to the chrome browser itself:
            // this background response covers the whole panel *before*
            // the per-pane loop below draws its own responses on top, so
            // a pointer event lands here only when it doesn't hit any
            // pane's own (smaller, later-drawn) response area - the
            // toolbar/sidebar/account-list/screen chrome around the
            // panes, in other words. Coordinates are translated back into
            // the chrome's own unscaled CSS-pixel space (the inverse of
            // `to_screen` above) before forwarding.
            let background = ui.interact(available, ui.id().with("chrome-background"), egui::Sense::click());
            if let Some(pos) = background.interact_pointer_pos() {
                if background.clicked() {
                    let cx = ((pos.x - available.min.x) / scale_x) as f64;
                    let cy = ((pos.y - available.min.y) / scale_y) as f64;
                    self.chrome.click_at(cx, cy);
                }
            }
            if background.hovered() {
                let raw_scroll = ui.input(|i| i.smooth_scroll_delta);
                if raw_scroll.y != 0.0 {
                    self.chrome.scroll_by(-raw_scroll.y as f64 / scale_y as f64);
                }
            }

            // Deliberately not a `for (pane, cell) in self.panes.iter_mut()...`
            // loop: the context menu below needs `&self.workspace` (to list
            // move-to-workspace targets) at the same time other branches
            // need `&mut self` (close/duplicate/move a pane) - an
            // `iter_mut()` borrow spanning the whole loop body would
            // conflict with those `self.method(...)` calls. Indexing
            // `self.panes[index]` fresh each time avoids holding a borrow
            // across them.
            for (slot, cell) in cells.into_iter().enumerate() {
                let index = visible[slot];
                let cell_rect = egui::Rect::from_min_size(egui::pos2(cell.x, cell.y), egui::vec2(cell.width, cell.height));
                let mut cell_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(cell_rect)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );

                let response = cell_ui.allocate_response(cell_rect.size(), egui::Sense::click());
                if response.clicked() {
                    self.selected = index;
                    response.request_focus();
                    // Real coordinate-to-DOM routing (mockup/rendering-engine-gaps.md's
                    // "input real" gap, closed for mouse): the frame is
                    // always rendered at PANE_WIDTH x PANE_HEIGHT (every
                    // spawn call in this file uses those constants) and
                    // displayed scaled + centered within `cell_rect` -
                    // mirrors the same scale/center math the texture-paint
                    // code below uses via `poll_texture`, computed here
                    // ahead of it since the click needs it first. A click
                    // landing in the letterboxed margin around the image
                    // (when the pane's aspect ratio doesn't match the
                    // cell's) is correctly ignored, not clamped onto an
                    // edge.
                    let image_size = egui::vec2(PANE_WIDTH as f32, PANE_HEIGHT as f32);
                    let scale = (cell_rect.width() / image_size.x).min(cell_rect.height() / image_size.y).min(1.0);
                    let displayed_size = image_size * scale;
                    let image_rect = egui::Rect::from_center_size(cell_rect.center(), displayed_size);
                    if let Some(pos) = response.interact_pointer_pos() {
                        if image_rect.contains(pos) {
                            let local = pos - image_rect.min;
                            let px = (local.x / displayed_size.x) as f64 * PANE_WIDTH as f64;
                            let py = (local.y / displayed_size.y) as f64 * PANE_HEIGHT as f64;
                            let _ = self.panes[index].browser.click_at(px, py);
                        }
                    }
                }
                if response.hovered() {
                    // Real mouse-wheel-to-scroll routing (mockup's "Scroll"
                    // gap): whichever pane the cursor is actually over gets
                    // the real wheel delta forwarded to
                    // `profile::Profile::scroll_by`, matching a real
                    // browser's "scroll whatever's under the cursor" rule
                    // (not whatever has focus - hover, not `has_focus()`,
                    // is the gate here). `smooth_scroll_delta.y` is egui's own
                    // "content moves down" convention (already normalized
                    // out of points/lines/pages by egui itself); this
                    // worker's `SCROLL <dy>` protocol uses the opposite
                    // sign (`dy` is how far the *viewport* moves down
                    // through the document), hence the negation.
                    let raw_scroll = cell_ui.input(|i| i.smooth_scroll_delta);
                    if raw_scroll.y != 0.0 {
                        let _ = self.panes[index].browser.scroll_by(-raw_scroll.y as f64);
                    }
                }
                if response.has_focus() {
                    // Without this, egui's own focus-cycling steals `Tab`
                    // the moment this pane has focus - it never reaches
                    // `i.events` below, and focus silently jumps to
                    // whatever egui widget it considers "next" (not
                    // necessarily another pane). `EventFilter { tab: true,
                    // .. }` tells egui this widget wants real `Tab`
                    // keypresses delivered as normal key events instead -
                    // same mechanism `egui::TextEdit` itself uses (see its
                    // own `set_focus_lock_filter` call). Called every
                    // frame the pane has focus, matching `TextEdit`'s own
                    // convention - `set_focus_lock_filter` no-ops unless
                    // the widget already had focus last frame too, so a
                    // freshly-focused pane takes one extra frame before
                    // its own `Tab` handling below actually fires (an
                    // imperceptible ~16ms at a real frame rate).
                    cell_ui.memory_mut(|mem| {
                        mem.set_focus_lock_filter(response.id, egui::EventFilter { tab: true, ..Default::default() });
                    });
                    // Real keyboard-to-DOM routing: whichever pane's
                    // click most recently called `request_focus()` above
                    // gets typed characters/backspace forwarded to
                    // `profile::Profile::type_key` - a no-op on the
                    // worker side (see its own `focused_id` doc) unless
                    // that pane's last `click_at` actually landed on a
                    // real `<input>`/`<textarea>`. `Tab`/`Shift+Tab` move
                    // real focus between the page's own focusable
                    // elements instead (`BrowserView::tab`, real
                    // `neutron::dom::Dom::tab_order` on the worker side) - not
                    // typed as a character.
                    for event in cell_ui.input(|i| i.events.clone()) {
                        match event {
                            egui::Event::Text(text) => {
                                for ch in text.chars() {
                                    let _ = self.panes[index].browser.type_key(&ch.to_string());
                                }
                            }
                            egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                                let _ = self.panes[index].browser.type_key("Backspace");
                            }
                            egui::Event::Key { key: egui::Key::Tab, pressed: true, modifiers, .. } => {
                                let _ = self.panes[index].browser.tab(modifiers.shift);
                            }
                            _ => {}
                        }
                    }
                }

                let mut close_clicked = false;
                let mut duplicate_clicked = false;
                let mut reload_clicked = false;
                let mut run_here_clicked = false;
                let mut move_to: Option<usize> = None;
                let mut move_to_new = false;
                let workspaces = self.workspace.workspaces();
                response.context_menu(|ui| {
                    if ui.button("Reload").clicked() {
                        reload_clicked = true;
                        ui.close();
                    }
                    if ui.button("Duplicate profile").clicked() {
                        duplicate_clicked = true;
                        ui.close();
                    }
                    if ui.button("Run auto login (shared script)").clicked() {
                        run_here_clicked = true;
                        ui.close();
                    }
                    // Web Audio exists now (neutron::js::web_audio -
                    // real OfflineAudioContext synthesis/mixing), but
                    // there's still no live AudioContext or OS audio
                    // output device anywhere in this engine - only
                    // headless rendering into an in-memory buffer. A
                    // "mute" toggle needs something actually playing
                    // sound to mute; still disabled, not faked.
                    ui.add_enabled(false, egui::Button::new("Mute audio")).on_disabled_hover_text("not implemented - no live audio output exists yet (only OfflineAudioContext's headless rendering)");
                    ui.menu_button("Move to workspace", |ui| {
                        for (workspace_index, workspace) in workspaces.iter().enumerate() {
                            if ui.button(&workspace.name).clicked() {
                                move_to = Some(workspace_index);
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("New workspace...").clicked() {
                            move_to_new = true;
                            ui.close();
                        }
                    });
                    // Real inspector doesn't exist in any phase of this
                    // engine yet - the spec itself flags "dev tools" as an
                    // open gap (CLAUDE.md), not something to fake here.
                    ui.add_enabled(false, egui::Button::new("Dev tools")).on_disabled_hover_text("not implemented - no inspector exists in this engine yet");
                    ui.separator();
                    if ui.button("Close pane").clicked() {
                        close_clicked = true;
                        ui.close();
                    }
                });

                if reload_clicked {
                    self.panes[index].browser.reload();
                }
                if run_here_clicked {
                    self.run_automation_script_for_pane(index);
                }
                if let Some(workspace_index) = move_to {
                    self.move_pane_to_workspace(index, workspace_index);
                }
                if move_to_new {
                    self.move_pane_to_new_workspace(index);
                }
                // Structural changes (pane count changes) invalidate
                // `cells`, which was computed for the pane count at the
                // top of this closure - stop drawing the rest of this
                // frame's grid rather than index a now-stale layout. Only
                // costs one frame; the next repaint (16ms later, see the
                // `request_repaint_after` above) re-lays-out from scratch.
                if duplicate_clicked {
                    self.duplicate_pane(index);
                    break;
                }
                if close_clicked {
                    self.close_pane(index);
                    break;
                }

                let pane = &mut self.panes[index];
                if let Some(texture) = pane.browser.poll_texture(cell_ui.ctx()) {
                    let image_size = texture.size_vec2();
                    let scale = (cell_rect.width() / image_size.x).min(cell_rect.height() / image_size.y).min(1.0);
                    cell_ui.put(cell_rect, egui::Image::new((texture.id(), image_size * scale)));
                } else if pane.browser.error().is_none() {
                    cell_ui.put(cell_rect, egui::Label::new(i18n::t(i18n::STARTING_PROFILE, self.locale)));
                }

                if let (Some(pid), Some(frame_generation)) = (pane.browser.pid(), pane.browser.frame_generation()) {
                    pane.monitor.tick(pid, frame_generation);
                }
                draw_resource_overlay(&cell_ui, cell_rect, &pane.monitor);

                let border_color = if index == self.selected { egui::Color32::LIGHT_BLUE } else { egui::Color32::DARK_GRAY };
                cell_ui.painter().rect_stroke(cell_rect, 0.0, egui::Stroke::new(2.0_f32, border_color), egui::StrokeKind::Outside);
            }
        });
    }
}
