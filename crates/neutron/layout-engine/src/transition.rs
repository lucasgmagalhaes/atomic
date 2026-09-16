//! Real CSS transitions (`transition-property`/`transition-duration`),
//! scoped to the two paint-time-only properties this crate already
//! threads through `render`'s three display-list builders per primitive
//! (`opacity`, `transform`) - the same properties `position: fixed`/
//! `sticky` already get "tag every primitive, no layout change" treatment
//! for, since animating them needs no re-layout, only re-tagging. A
//! transitioning `width`/`height`/`color`/border/etc. isn't modeled - it
//! would need layout itself to re-run every animation frame, not just the
//! paint step, a materially bigger change this pass doesn't take.
//!
//! `transition-timing-function` is always linear (no `ease`/cubic-bezier
//! curves - see [`ScalarRun::value`]/[`VectorRun::value`]) and
//! `transition-delay` isn't modeled. A transition only ever engages on a
//! value *change* after an element's own first paint - real CSS never
//! animates an element's initial style application, and the first time
//! [`TransitionStates::apply`] sees a given [`dom::NodeId`] is the only
//! signal this crate has for "first paint", so that case is handled by
//! recording the target as already-settled rather than animating into it.
//!
//! This module has no idea what a frame or a render loop is - it's a pure
//! `(tree, now) -> mutated tree` step, called once per real paint by
//! `profile-worker`'s `Page::render` (see that call site's own doc for how
//! it also uses this module's return value to bypass paint/layer caching
//! for whatever's still mid-animation, since neither cache's key includes
//! wall-clock time).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dom::NodeId;

use crate::style::TransitionProperty;
use crate::tree::LayoutBox;

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[derive(Debug, Clone, Copy)]
struct ScalarRun {
    from: f64,
    to: f64,
    start: Instant,
    duration: Duration,
}

impl ScalarRun {
    fn progress(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            1.0
        } else {
            (now.duration_since(self.start).as_secs_f64() / self.duration.as_secs_f64())
                .clamp(0.0, 1.0)
        }
    }

    fn value(&self, now: Instant) -> f64 {
        lerp(self.from, self.to, self.progress(now))
    }

    /// `from == to` means this run was recorded as already-settled (the
    /// "first paint" case, or a real transition that finished and was
    /// never retargeted since) - never active regardless of how much of
    /// its nominal `duration` window `now` falls inside, since there is
    /// nothing left to interpolate towards.
    fn is_active(&self, now: Instant) -> bool {
        self.from != self.to && now < self.start + self.duration
    }
}

#[derive(Debug, Clone, Copy)]
struct VectorRun {
    from: (f64, f64),
    to: (f64, f64),
    start: Instant,
    duration: Duration,
}

impl VectorRun {
    fn progress(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            1.0
        } else {
            (now.duration_since(self.start).as_secs_f64() / self.duration.as_secs_f64())
                .clamp(0.0, 1.0)
        }
    }

    fn value(&self, now: Instant) -> (f64, f64) {
        let t = self.progress(now);
        (
            lerp(self.from.0, self.to.0, t),
            lerp(self.from.1, self.to.1, t),
        )
    }

    /// See [`ScalarRun::is_active`]'s own doc - same reasoning.
    fn is_active(&self, now: Instant) -> bool {
        self.from != self.to && now < self.start + self.duration
    }
}

fn step_scalar(
    map: &mut HashMap<NodeId, ScalarRun>,
    node: NodeId,
    target: f64,
    now: Instant,
    duration: Duration,
) -> bool {
    match map.get(&node).copied() {
        None => {
            map.insert(
                node,
                ScalarRun {
                    from: target,
                    to: target,
                    start: now,
                    duration,
                },
            );
            false
        }
        Some(run) if run.to != target => {
            // Real "interrupt and retarget": the new transition starts
            // from wherever the old one currently sits, not from its own
            // old target - a mid-flight value change never jumps.
            let from = run.value(now);
            map.insert(
                node,
                ScalarRun {
                    from,
                    to: target,
                    start: now,
                    duration,
                },
            );
            true
        }
        Some(run) => run.is_active(now),
    }
}

fn step_vector(
    map: &mut HashMap<NodeId, VectorRun>,
    node: NodeId,
    target: (f64, f64),
    now: Instant,
    duration: Duration,
) -> bool {
    match map.get(&node).copied() {
        None => {
            map.insert(
                node,
                VectorRun {
                    from: target,
                    to: target,
                    start: now,
                    duration,
                },
            );
            false
        }
        Some(run) if run.to != target => {
            let from = run.value(now);
            map.insert(
                node,
                VectorRun {
                    from,
                    to: target,
                    start: now,
                    duration,
                },
            );
            true
        }
        Some(run) => run.is_active(now),
    }
}

/// Per-page, cross-frame transition state - owned by whatever repeatedly
/// re-lays-out and re-paints a page (`profile-worker`'s `Page`), since a
/// transition's own "where did it start from" has to survive across many
/// [`TransitionStates::apply`] calls, not just one.
#[derive(Debug, Default)]
pub struct TransitionStates {
    opacity: HashMap<NodeId, ScalarRun>,
    transform: HashMap<NodeId, VectorRun>,
}

impl TransitionStates {
    pub fn new() -> Self {
        Self::default()
    }

    /// Walks `box_`'s subtree, overwriting `opacity`/`transform` in place
    /// for any box whose `transition_property`/`transition_duration` call
    /// for it (see the module doc for exactly how a transition starts/
    /// retargets/settles). Returns every [`NodeId`] with an in-flight
    /// transition at `now`, so a caller can bypass its own paint/layer
    /// caching for exactly those nodes - see `profile-worker`'s
    /// `Page::render` for the one real consumer.
    pub fn apply(&mut self, box_: &mut LayoutBox, now: Instant) -> Vec<NodeId> {
        let mut active = Vec::new();
        self.apply_inner(box_, now, &mut active);
        active
    }

    fn apply_inner(&mut self, box_: &mut LayoutBox, now: Instant, active: &mut Vec<NodeId>) {
        let duration_secs = box_.style.transition_duration;
        if duration_secs > 0.0 {
            let duration = Duration::from_secs_f64(duration_secs);
            let animates_opacity = matches!(
                box_.style.transition_property,
                TransitionProperty::Opacity | TransitionProperty::All
            );
            let animates_transform = matches!(
                box_.style.transition_property,
                TransitionProperty::Transform | TransitionProperty::All
            );

            if animates_opacity {
                let target = box_.style.opacity;
                let is_active = step_scalar(&mut self.opacity, box_.node, target, now, duration);
                if let Some(run) = self.opacity.get(&box_.node) {
                    box_.style.opacity = run.value(now);
                }
                if is_active {
                    active.push(box_.node);
                }
            }
            if animates_transform {
                let target = box_.style.transform;
                let is_active = step_vector(&mut self.transform, box_.node, target, now, duration);
                if let Some(run) = self.transform.get(&box_.node) {
                    box_.style.transform = run.value(now);
                }
                if is_active {
                    active.push(box_.node);
                }
            }
        }

        for child in &mut box_.children {
            self.apply_inner(child, now, active);
        }
    }
}
