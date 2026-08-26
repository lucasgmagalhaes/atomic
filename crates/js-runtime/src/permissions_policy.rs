//! A real, narrowly-scoped `Permissions-Policy` primitive — the other half
//! of item 7's "permissions policy/trusted types" gap alongside
//! [`crate::trusted_types`]. Real browsers use this mostly to control
//! feature *delegation* into cross-origin iframes, which is structurally
//! out of scope here (no iframes exist anywhere in `dom`) — but a
//! `Permissions-Policy` also restricts the top-level document itself, and
//! that half is real and checkable: gates the two real, already-implemented
//! APIs a policy can meaningfully restrict in this engine, `navigator.
//! clipboard` (`clipboard-read`/`clipboard-write`) and `Notification`
//! (`notifications`).
//!
//! Syntax subset: `feature=(self)`/`feature=(*)`/`feature=()` — allowlist
//! tokens beyond `self`/`*` (an explicit origin list) aren't parsed, same
//! "only `'self'`/`*`/`'none'`-shaped sources" scope `csp.rs` already has
//! for `connect-src`. A feature not mentioned in the policy at all is
//! allowed (real spec default for a top-level document is `*` unless the
//! policy says otherwise).
fn find_directive<'a>(policy: &'a str, feature: &str) -> Option<&'a str> {
  policy.split(',').find_map(|part| {
    let part = part.trim();
    let (name, rest) = part.split_once('=')?;
    name
      .trim()
      .eq_ignore_ascii_case(feature)
      .then(|| rest.trim())
  })
}

/// Whether `feature` (e.g. `"clipboard-write"`, `"notifications"`) is
/// allowed for the top-level document under `policy`. `policy: None`
/// (a plain `Context::with_dom`, or a host that never calls
/// `Context::set_permissions_policy`) always allows — same
/// degrade-gracefully pattern `csp`/`cors` already use for missing state.
pub(crate) fn is_feature_allowed(policy: Option<&str>, feature: &str) -> bool {
  let Some(policy) = policy else {
    return true;
  };
  let Some(allowlist) = find_directive(policy, feature) else {
    return true;
  };
  let allowlist = allowlist.trim_matches(|c| c == '(' || c == ')').trim();
  if allowlist.is_empty() {
    return false;
  }
  allowlist
    .split_whitespace()
    .any(|token| token == "*" || token.trim_matches('"') == "self")
}

/// Checks the response policy attached to `ctx`'s document. Keeping this at
/// the native capability boundary means JavaScript cannot bypass a policy by
/// caching `navigator.clipboard` or `Notification` before the host installs
/// it. Contexts without a DOM-host state have no response policy and remain
/// unrestricted, matching `csp`'s existing degradation behavior.
pub(crate) unsafe fn is_allowed(ctx: *mut quickjs_sys::JSContext, feature: &str) -> bool {
  let state = crate::host_state::get(ctx);
  state.is_null() || is_feature_allowed((*state).permissions_policy.as_deref(), feature)
}
