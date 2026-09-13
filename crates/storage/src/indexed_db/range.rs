//! `CursorDirection`/`KeyRange` — split out from `indexed_db.rs`.

/// Which way a [`super::Cursor`] walks its (already sorted) key list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorDirection {
    Next,
    Prev,
}

/// A bound on which keys a [`super::Cursor`] visits — mirrors `IDBKeyRange`.
/// Keys compare lexicographically (this crate's keys are always strings,
/// unlike real IndexedDB's structured-clone key comparison algorithm —
/// documented in `crate::cookies`-adjacent modules as an acceptable cut
/// for a string-keyed store).
#[derive(Debug, Clone)]
pub struct KeyRange {
    lower: Option<(String, bool)>, // (bound, exclusive)
    upper: Option<(String, bool)>,
}

impl KeyRange {
    pub fn all() -> Self {
        KeyRange {
            lower: None,
            upper: None,
        }
    }

    pub fn only(key: impl Into<String>) -> Self {
        let key = key.into();
        KeyRange {
            lower: Some((key.clone(), false)),
            upper: Some((key, false)),
        }
    }

    pub fn lower_bound(lower: impl Into<String>, open: bool) -> Self {
        KeyRange {
            lower: Some((lower.into(), open)),
            upper: None,
        }
    }

    pub fn upper_bound(upper: impl Into<String>, open: bool) -> Self {
        KeyRange {
            lower: None,
            upper: Some((upper.into(), open)),
        }
    }

    pub fn bound(
        lower: impl Into<String>,
        upper: impl Into<String>,
        lower_open: bool,
        upper_open: bool,
    ) -> Self {
        KeyRange {
            lower: Some((lower.into(), lower_open)),
            upper: Some((upper.into(), upper_open)),
        }
    }

    pub(super) fn contains(&self, key: &str) -> bool {
        if let Some((lower, open)) = &self.lower {
            if *open {
                if key <= lower.as_str() {
                    return false;
                }
            } else if key < lower.as_str() {
                return false;
            }
        }
        if let Some((upper, open)) = &self.upper {
            if *open {
                if key >= upper.as_str() {
                    return false;
                }
            } else if key > upper.as_str() {
                return false;
            }
        }
        true
    }
}
