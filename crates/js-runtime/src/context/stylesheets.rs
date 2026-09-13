//! `document.adoptedStyleSheets` version/text — split out from
//! `context/mod.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use crate::cssom_stylesheet;

use super::Context;

impl<'rt> Context<'rt> {
    /// Every rule text from every real `CSSStyleSheet` instance currently
    /// in `document.adoptedStyleSheets`, concatenated with newlines — a
    /// host (`profile-worker`'s `Page::layout`) parses this and merges it
    /// into the real cascade before laying out, so a page's own
    /// `sheet.insertRule(...)`/`deleteRule(...)` calls actually change what
    /// gets rendered on the next frame. A non-`CSSStyleSheet` entry in the
    /// array (nothing validates what a page assigns there) is silently
    /// skipped. Empty string on a plain [`Context::new`]/`with_dom` with
    /// nothing adopted.
    /// A cheap cache key for [`adopted_stylesheet_text`](Self::adopted_stylesheet_text):
    /// combines every adopted sheet's own mutation-version counter (bumped
    /// by `insertRule`/`deleteRule`) with the adopted-sheets array's own
    /// length, without touching any rule's actual text. A host
    /// (`profile-worker`'s `Page::layout`) calls this on every layout pass
    /// to decide whether anything adopted changed *before* paying for the
    /// full text rebuild `adopted_stylesheet_text` does — that method used
    /// to be the only way to detect a change, forcing a real per-frame
    /// string rebuild+compare purely to serve as a cache key (see
    /// `spec/RULES.md`'s cache rule). Combining length in means removing
    /// or adding a sheet changes the key even though no single sheet's own
    /// version moved.
    pub fn adopted_stylesheet_version(&self) -> u64 {
        unsafe {
            let global = sys::JS_GetGlobalObject(self.ptr);
            let doc_name = CString::new("document").unwrap();
            let document = sys::JS_GetPropertyStr(self.ptr, global, doc_name.as_ptr());
            sys::JS_FreeValue(self.ptr, global);
            let sheets_name = CString::new("adoptedStyleSheets").unwrap();
            let sheets = sys::JS_GetPropertyStr(self.ptr, document, sheets_name.as_ptr());
            sys::JS_FreeValue(self.ptr, document);

            let mut len: i64 = 0;
            sys::JS_GetLength(self.ptr, sheets, &mut len);
            let mut key = len.max(0) as u64;
            for i in 0..len.max(0) as u32 {
                let sheet = sys::JS_GetPropertyUint32(self.ptr, sheets, i);
                if let Some(version) = cssom_stylesheet::sheet_version(self.ptr, sheet) {
                    // A simple mix, not a real hash - collisions are
                    // harmless here (worst case: one stale frame before
                    // the next real change is caught), and this must stay
                    // cheap since it runs every layout pass.
                    key = key
                        .wrapping_mul(1_000_003)
                        .wrapping_add(version)
                        .wrapping_add(1);
                }
                sys::JS_FreeValue(self.ptr, sheet);
            }
            sys::JS_FreeValue(self.ptr, sheets);
            key
        }
    }

    pub fn adopted_stylesheet_text(&self) -> String {
        unsafe {
            let global = sys::JS_GetGlobalObject(self.ptr);
            let doc_name = CString::new("document").unwrap();
            let document = sys::JS_GetPropertyStr(self.ptr, global, doc_name.as_ptr());
            sys::JS_FreeValue(self.ptr, global);
            let sheets_name = CString::new("adoptedStyleSheets").unwrap();
            let sheets = sys::JS_GetPropertyStr(self.ptr, document, sheets_name.as_ptr());
            sys::JS_FreeValue(self.ptr, document);

            let mut len: i64 = 0;
            sys::JS_GetLength(self.ptr, sheets, &mut len);
            let mut text = String::new();
            for i in 0..len.max(0) as u32 {
                let sheet = sys::JS_GetPropertyUint32(self.ptr, sheets, i);
                if let Some(rules) = cssom_stylesheet::rules_of(self.ptr, sheet) {
                    for rule in rules {
                        text.push_str(&rule);
                        text.push('\n');
                    }
                }
                sys::JS_FreeValue(self.ptr, sheet);
            }
            sys::JS_FreeValue(self.ptr, sheets);
            text
        }
    }
}
