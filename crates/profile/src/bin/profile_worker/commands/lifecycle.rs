//! `PING`/`PAUSE`/`RESUME`/`SET_FPS_CAP`/`RESIZE`/`QUIT` — split out from
//! `commands/mod.rs`.

use std::io::Write;
use std::time::{Duration, Instant};

use super::{Handled, WorkerState};

impl<'rt> WorkerState<'rt> {
    pub(super) fn handle_lifecycle(&mut self, stdout: &mut impl Write, line: &str) -> Handled {
        if line == "PING" {
            let _ = writeln!(stdout, "PONG");
            let _ = stdout.flush();
        } else if line == "PAUSE" {
            self.paused = true;
            let _ = writeln!(stdout, "PAUSED");
            let _ = stdout.flush();
        } else if line == "RESUME" {
            self.paused = false;
            self.next_tick = Instant::now() + self.frame_interval;
            let _ = writeln!(stdout, "RESUMED");
            let _ = stdout.flush();
        } else if let Some(rest) = line.strip_prefix("SET_FPS_CAP ") {
            match rest.trim().parse::<u32>() {
                Ok(fps) if fps >= 1 => {
                    self.frame_interval = Duration::from_nanos(1_000_000_000 / fps as u64);
                    let _ = writeln!(stdout, "FPS_CAP_SET {fps}");
                }
                _ => {
                    let _ = writeln!(stdout, "ERROR fps cap must be a positive integer");
                }
            }
            let _ = stdout.flush();
        } else if let Some(rest) = line.strip_prefix("RESIZE ") {
            let mut parts = rest.trim().splitn(2, ' ');
            let dims = parts
                .next()
                .zip(parts.next())
                .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
            match dims {
                Some((new_width, new_height)) => {
                    self.resize_count += 1;
                    let new_shmem_name = format!("{}-r{}", self.shmem_name, self.resize_count);
                    match ipc::FrameWriter::new(&new_shmem_name, new_width, new_height) {
                        Ok(new_writer) => {
                            self.writer = new_writer;
                            self.width = new_width;
                            self.height = new_height;
                            self.page
                                .ctx
                                .set_viewport_size(self.width as f64, self.height as f64);
                            self.page.ctx.fire_resize();
                            self.sync_scroll();
                            self.writer.publish(&self.page.render(
                                &self.renderer,
                                self.width,
                                self.height,
                                self.scroll_top,
                            ));
                            let _ = writeln!(stdout, "RESIZED {new_shmem_name}");
                        }
                        Err(e) => {
                            let _ =
                                writeln!(stdout, "ERROR failed to open resized frame buffer: {e}");
                        }
                    }
                }
                None => {
                    let _ = writeln!(stdout, "ERROR RESIZE requires two positive integers");
                }
            }
            let _ = stdout.flush();
        } else if line == "QUIT" {
            return Handled::Quit;
        } else {
            return Handled::No;
        }
        Handled::Handled
    }
}
