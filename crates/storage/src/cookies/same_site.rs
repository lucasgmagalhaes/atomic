//! `SameSite` — split out from `cookies.rs`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl Default for SameSite {
    /// Modern browsers treat an unspecified `SameSite` as `Lax`, not
    /// "no restriction" — matched here rather than defaulting to `None`.
    fn default() -> Self {
        SameSite::Lax
    }
}

impl SameSite {
    pub(super) fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "strict" => Some(SameSite::Strict),
            "lax" => Some(SameSite::Lax),
            "none" => Some(SameSite::None),
            _ => None,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            SameSite::Strict => "strict",
            SameSite::Lax => "lax",
            SameSite::None => "none",
        }
    }
}
