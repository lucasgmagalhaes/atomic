//! Nested-dispatch depth guard — split out from `dispatch.rs`.

use std::cell::RefCell;
use std::collections::HashMap;

use quickjs_sys as sys;

const MAX_NESTED_DISPATCH: usize = 32;

thread_local! {
    static DISPATCH_DEPTHS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

pub(super) struct DispatchGuard {
    ctx: usize,
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        DISPATCH_DEPTHS.with(|depths| {
            let mut depths = depths.borrow_mut();
            let depth = depths
                .get_mut(&self.ctx)
                .expect("dispatch depth must exist");
            *depth -= 1;
            if *depth == 0 {
                depths.remove(&self.ctx);
            }
        });
    }
}

pub(super) fn begin_dispatch(ctx: *mut sys::JSContext) -> Option<DispatchGuard> {
    let ctx = ctx as usize;
    DISPATCH_DEPTHS.with(|depths| {
        let mut depths = depths.borrow_mut();
        let depth = depths.entry(ctx).or_default();
        if *depth >= MAX_NESTED_DISPATCH {
            return None;
        }
        *depth += 1;
        Some(DispatchGuard { ctx })
    })
}
