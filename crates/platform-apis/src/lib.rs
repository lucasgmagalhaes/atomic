/// Fills `buf` with cryptographically secure random bytes from the OS
/// (BCryptGenRandom on Windows, getrandom(2)/arc4random on Linux/macOS via
/// the `getrandom` crate). Backs `crypto.getRandomValues` in `js-runtime`.
pub fn fill_random(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    getrandom::fill(buf)
}
