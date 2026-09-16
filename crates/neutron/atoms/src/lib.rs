//! Interned string handles (`Atom`) for hot-path identifiers — tag names,
//! attribute names, CSS property names — that are otherwise compared and
//! hashed as `String`/`&str` on every selector match, style cascade, and
//! DOM walk (`spec/architecture/performance.md`'s hot-path rule: no
//! repeated string comparison/hashing in layout/style/paint hot paths).
//!
//! A global, `RwLock`-backed interner (not `thread_local!`) because
//! `workers` spawns real OS threads that each parse/query their own DOM —
//! every thread must resolve the same tag/attribute vocabulary to the same
//! `Atom` ids, not a per-thread-local set that would collide across
//! threads if ids were ever compared or serialized between them.
//!
//! Scope cut (documented, not a placeholder): this crate only covers tag
//! names so far (`dom::NodeData::Element::tag`). Attribute names and CSS
//! property names stay `String` for now — migrating them is a separate,
//! later pass once this one is proven out, not bundled into one large
//! diff (see `.claude/plans/architecture-p5-foundations.plan.md` Stage 1).

use std::collections::HashMap;
use std::fmt;
use std::sync::{OnceLock, RwLock};

/// An interned string handle. `Copy`, cheap to compare/hash (a `u32`
/// under the hood) — the whole point of interning. Dereferences to `&str`
/// so existing code written against `&str`/`String` tag values (string
/// method calls, `matches!`, function calls expecting `&str`) keeps
/// compiling unchanged via deref coercion; equality against a literal
/// (`atom == "div"`) is additionally supported directly (see the
/// `PartialEq` impls below) since deref coercion alone doesn't cover
/// operators.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Atom(u32);

struct Interner {
    ids: HashMap<&'static str, u32>,
    strings: Vec<&'static str>,
}

fn interner() -> &'static RwLock<Interner> {
    static INTERNER: OnceLock<RwLock<Interner>> = OnceLock::new();
    INTERNER.get_or_init(|| {
        RwLock::new(Interner {
            ids: HashMap::new(),
            strings: Vec::new(),
        })
    })
}

impl Atom {
    /// Interns `s`, returning the same `Atom` for equal strings across
    /// every call/thread. Leaks the string once per unique value the
    /// process ever sees (bounded by the real vocabulary of tag names a
    /// page can use — not unbounded user data), the same "small, bounded,
    /// process-lifetime table" shape `class_registry`'s per-runtime class
    /// ID map already uses for a different kind of identifier.
    pub fn new(s: &str) -> Self {
        if let Some(&id) = interner().read().unwrap().ids.get(s) {
            return Atom(id);
        }
        let mut w = interner().write().unwrap();
        if let Some(&id) = w.ids.get(s) {
            return Atom(id);
        }
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        let id = w.strings.len() as u32;
        w.strings.push(leaked);
        w.ids.insert(leaked, id);
        Atom(id)
    }

    pub fn as_str(&self) -> &'static str {
        interner().read().unwrap().strings[self.0 as usize]
    }
}

impl std::ops::Deref for Atom {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for Atom {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for Atom {
    fn from(s: &str) -> Self {
        Atom::new(s)
    }
}

impl From<String> for Atom {
    fn from(s: String) -> Self {
        Atom::new(&s)
    }
}

impl PartialEq<str> for Atom {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Atom {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Atom> for str {
    fn eq(&self, other: &Atom) -> bool {
        self == other.as_str()
    }
}

impl fmt::Debug for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}
