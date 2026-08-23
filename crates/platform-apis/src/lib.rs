/// Fills `buf` with cryptographically secure random bytes from the OS
/// (BCryptGenRandom on Windows, getrandom(2)/arc4random on Linux/macOS via
/// the `getrandom` crate). Backs `crypto.getRandomValues` in `js-runtime`.
pub fn fill_random(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    getrandom::fill(buf)
}

/// Real OS clipboard access via `arboard` (Win32 clipboard / X11-or-
/// Wayland / NSPasteboard under the hood) — backs the future
/// `navigator.clipboard` binding in `js-runtime`, not wired up yet.
/// Text only, matching `Clipboard.readText`/`writeText`; no images/HTML.
pub fn clipboard_write_text(text: &str) -> Result<(), arboard::Error> {
    arboard::Clipboard::new()?.set_text(text)
}

pub fn clipboard_read_text() -> Result<String, arboard::Error> {
    arboard::Clipboard::new()?.get_text()
}
