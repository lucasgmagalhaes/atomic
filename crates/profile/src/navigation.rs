//! `Profile::ping`/`reload`/`navigate`/`resize` — split out from `lib.rs`.

use std::io::{BufRead, Write};

use super::Profile;

impl Profile {
    /// Sends `PING`, waits for `PONG`. `Ok(true)` means the process is
    /// alive and responsive; `Ok(false)` means it responded with
    /// something else (shouldn't happen against a well-behaved worker).
    pub fn ping(&mut self) -> std::io::Result<bool> {
        writeln!(self.stdin, "PING")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(line.trim() == "PONG")
    }

    /// Asks the worker to re-render and publish a new frame.
    pub fn reload(&mut self) -> std::io::Result<()> {
        writeln!(self.stdin, "RELOAD")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(())
    }

    /// Asks the worker to fetch `url` (a real HTTP/HTTPS request via
    /// `net::get`, blocking the worker's render loop for the duration —
    /// see `profile-worker`'s doc on why that's an acceptable simplicity
    /// trade at this scope) and render the result, replacing the current
    /// page. `Ok(Ok(()))` means the page loaded; `Ok(Err(message))` means
    /// the worker is still alive and responsive but the fetch/parse
    /// itself failed (bad URL, network error, ...) — the worker renders
    /// a real in-page error message in that case rather than crashing or
    /// leaving the previous page up silently. The outer `io::Result`
    /// only covers the stdin/stdout protocol itself failing (a dead
    /// worker, a broken pipe).
    pub fn navigate(&mut self, url: &str) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "NAVIGATE {url}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Asks the worker to resize its viewport to `width` × `height` and
    /// re-render. Real live resize (`ROADMAP.md` P3 item 23's other half —
    /// [`spawn`](Self::spawn)'s `width`/`height` were fixed for the
    /// process's whole life until this): the worker republishes under a
    /// *new* shared-memory segment named `<original>-r<N>` (`N` a
    /// per-worker resize counter starting at 1) rather than attempting to
    /// grow the existing one in place — shared memory regions are
    /// fixed-size at creation, and recreating under a fresh name sidesteps
    /// that entirely instead of relying on riskier OS-specific resize/
    /// remap semantics. `Ok(Ok(()))` means the worker resized and this
    /// `Profile` has reopened its [`FrameReader`](ipc::FrameReader) against
    /// the new segment (subsequent `latest_frame()` calls see the new
    /// size); `Ok(Err(message))` means the worker rejected the request or
    /// this side failed to reopen the new segment — the outer `io::Result`
    /// only covers the stdin/stdout protocol itself failing, same
    /// convention as [`navigate`](Self::navigate).
    pub fn resize(&mut self, width: u32, height: u32) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "RESIZE {width} {height}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        if let Some(message) = line.strip_prefix("ERROR ") {
            return Ok(Err(message.to_string()));
        }
        let Some(new_name) = line.strip_prefix("RESIZED ") else {
            return Ok(Err(format!("unexpected resize response: {line}")));
        };
        match ipc::FrameReader::new(new_name, width, height) {
            Ok(reader) => {
                self.frame_reader = reader;
                Ok(Ok(()))
            }
            Err(e) => Ok(Err(format!("failed to reopen frame buffer: {e}"))),
        }
    }
}
