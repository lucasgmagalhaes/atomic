//! `Profile::evaluate`/`console`/`set_fps_cap`/`pause`/`resume` — split out
//! from `lib.rs`.

use std::io::{BufRead, Write};

use super::Profile;

impl Profile {
    /// Evaluates arbitrary JS in the current page's own context — the
    /// generic "drive the page like a devtools console" command every
    /// other method here is a fixed special case of (`click` builds and
    /// evals one hardcoded dispatch snippet, `fill` a hardcoded assignment,
    /// etc.). The worker re-renders and republishes the frame before
    /// replying, so whatever the script mutated — DOM structure, an adopted
    /// stylesheet via `insertRule`, anything — is already visible in real
    /// painted pixels by the time this returns (see the adopted-stylesheet
    /// test in this crate's integration tests for exactly that assertion).
    /// `Ok(Ok(result))` carries the script's stringified completion value;
    /// `Ok(Err(message))` means evaluation threw (the worker is still alive
    /// and any mutations made before the throw are kept and rendered). Not
    /// gated by CSP — host-driven evaluation, not the page loading its own
    /// script. `script` must not contain a newline (this crate's stdin/
    /// stdout protocol is newline-delimited) or NUL bytes (which would
    /// panic the worker's string conversion); either is rejected here
    /// before it would corrupt or crash the worker, reported the same way
    /// a worker-side failure would be. Scripts are also length-capped at
    /// 64KB, matching this engine's other untrusted-string input bounds.
    /// The outer `io::Result` only covers the protocol itself failing (a
    /// dead worker, a broken pipe) — same convention as [`navigate`](Self::navigate).
    pub fn evaluate(&mut self, script: &str) -> std::io::Result<Result<String, String>> {
        if script.contains('\n') {
            return Ok(Err("script must not contain a newline".to_string()));
        }
        if script.contains('\0') {
            return Ok(Err("script must not contain NUL bytes".to_string()));
        }
        const MAX_SCRIPT_LENGTH: usize = 64 * 1024;
        if script.len() > MAX_SCRIPT_LENGTH {
            return Ok(Err("script exceeds the maximum length".to_string()));
        }
        writeln!(self.stdin, "EVAL {script}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(line
                .strip_prefix("EVALUATED ")
                .unwrap_or(line)
                .trim()
                .to_string())),
        }
    }

    /// Drains the page's accumulated `console.*` output (see
    /// `profile-worker`'s `CONSOLE` command) as `(level, text)` pairs -
    /// the same stream a devtools console would show, including uncaught
    /// script errors reported at `error` level by `Context::eval` itself.
    /// Draining: a second call only returns messages logged after this
    /// one.
    pub fn console(&mut self) -> std::io::Result<Vec<(String, String)>> {
        writeln!(self.stdin, "CONSOLE")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("MESSAGES ") {
            Some(rest) => Ok(rest
                .split(" | ")
                .filter_map(|entry| entry.split_once(':'))
                .map(|(level, text)| (level.to_string(), text.to_string()))
                .collect()),
            // Bare `MESSAGES` - nothing logged since the last drain.
            None => Ok(Vec::new()),
        }
    }

    /// Caps the worker's own vsync render loop at `fps` frames per second
    /// (the mockup's "Settings > Performance > frame cap" knob) - takes
    /// effect on the worker's very next tick, not a respawn (unlike the
    /// proxy/DNS settings, which are fixed at spawn time - the render loop
    /// itself has no equivalent "set once" constraint, so this is real live
    /// throttling of an already-running profile). `Ok(Err(message))` if
    /// `fps` isn't a positive integer the worker accepted.
    pub fn set_fps_cap(&mut self, fps: u32) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "SET_FPS_CAP {fps}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Pauses the worker's own vsync render loop entirely (no JS timer
    /// pumping, no re-render, no frame publish - genuinely idle, see
    /// `profile-worker`'s own `paused` doc) - the mockup's "background
    /// throttling" knob for a pane that isn't currently visible. `PING`/
    /// `SET_FPS_CAP`/`QUIT` still work while paused. No failure mode
    /// beyond the protocol itself, unlike `click`/`fill`/`navigate` -
    /// there's nothing about "pause" that can be rejected.
    pub fn pause(&mut self) -> std::io::Result<()> {
        writeln!(self.stdin, "PAUSE")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(())
    }

    /// Resumes a paused profile's render loop - a no-op (still answers
    /// `RESUMED`) if it wasn't paused.
    pub fn resume(&mut self) -> std::io::Result<()> {
        writeln!(self.stdin, "RESUME")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(())
    }
}
