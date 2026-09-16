//! Convex-only filled paths - `beginPath`/`moveTo`/`lineTo`/`arc`/
//! `closePath`/`fill`. See [`Canvas2D::fill`]'s own doc for exactly why
//! only convex polygons render correctly, and [`Canvas2D::arc`]'s own
//! doc for why arcs are a straight-line polyline approximation, not a
//! true curve.
use super::helpers::{color_to_f32, point_to_ndc};
use super::pipeline::Vertex;
use super::Canvas2D;

/// Straight-line segment count [`Canvas2D::bezier_curve_to`]/
/// [`Canvas2D::quadratic_curve_to`] sample a curve into. No natural
/// "angle span" to scale against the way [`Canvas2D::arc`] does (a
/// Bézier curve has no fixed center/radius), so this is a single fixed
/// constant — smooth enough at this project's target UI scale, not
/// adaptive to curve length like `arc`'s own segment count is.
const CURVE_SEGMENTS: usize = 32;

impl Canvas2D {
    /// `ctx.beginPath()` — discards any points accumulated since the last
    /// call.
    pub fn begin_path(&mut self) {
        self.path_points.clear();
    }

    /// `ctx.moveTo(x, y)` — see `Canvas2D::path_points`'s own doc for why
    /// this behaves identically to [`Self::line_to`] in this crate.
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.path_points.push((x, y));
    }

    /// `ctx.lineTo(x, y)`.
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.path_points.push((x, y));
    }

    /// `ctx.arc(x, y, radius, startAngle, endAngle, anticlockwise)` —
    /// appends a circular arc to the current path as a straight-line
    /// polyline approximation (this crate's [`Self::fill`] only knows how
    /// to fan-triangulate straight `path_points`, no real curve
    /// primitive), same "narrower than spec, clearly documented"
    /// convention as [`Self::fill`]'s own convex-only cut. Segment count
    /// scales with the angle span (up to 64 for a full circle, fewer for
    /// a smaller arc) - visually smooth at typical UI radii, not a true
    /// curve. If the path already has points, the first arc point simply
    /// gets appended after them - `path_points` has no `moveTo`/`lineTo`
    /// distinction (see that field's own doc), so the "implicit straight
    /// line from the current point to the arc's start" real spec draws
    /// falls out for free from the existing fan-triangulation, not
    /// special-cased here. `anticlockwise` flips the sweep direction,
    /// matching real spec.
    pub fn arc(
        &mut self,
        x: f32,
        y: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        anticlockwise: bool,
    ) {
        const TWO_PI: f32 = std::f32::consts::PI * 2.0;
        let mut span = end_angle - start_angle;
        if anticlockwise {
            while span > 0.0 {
                span -= TWO_PI;
            }
        } else {
            while span < 0.0 {
                span += TWO_PI;
            }
        }
        let segments = ((span.abs() / TWO_PI) * 64.0).ceil().max(2.0) as usize;
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = start_angle + span * t;
            self.path_points
                .push((x + radius * angle.cos(), y + radius * angle.sin()));
        }
    }

    /// `ctx.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y)` — appends a cubic
    /// Bézier curve from the current point (`path_points.last()`) through
    /// the two control points to `(x, y)`, sampled as a straight-line
    /// polyline (fixed `CURVE_SEGMENTS` count) into `path_points`, same
    /// "narrower than spec, clearly documented" convention as
    /// [`Self::arc`]'s own polyline approximation. Unlike `arc`, a cubic
    /// curve's shape is defined *relative to the current point* rather
    /// than a center/radius, so (matching real spec's own requirement of
    /// an existing subpath) this is a no-op when `path_points` is empty —
    /// no implicit starting point exists to curve from.
    pub fn bezier_curve_to(&mut self, cp1x: f32, cp1y: f32, cp2x: f32, cp2y: f32, x: f32, y: f32) {
        let Some(&p0) = self.path_points.last() else {
            return;
        };
        let p1 = (cp1x, cp1y);
        let p2 = (cp2x, cp2y);
        let p3 = (x, y);
        for i in 1..=CURVE_SEGMENTS {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let mt = 1.0 - t;
            let (a, b, c, d) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
            self.path_points.push((
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            ));
        }
    }

    /// `ctx.quadraticCurveTo(cpx, cpy, x, y)` — same shape as
    /// [`Self::bezier_curve_to`], one fewer control point. No-op when
    /// `path_points` is empty, same reasoning.
    pub fn quadratic_curve_to(&mut self, cpx: f32, cpy: f32, x: f32, y: f32) {
        let Some(&p0) = self.path_points.last() else {
            return;
        };
        let p1 = (cpx, cpy);
        let p2 = (x, y);
        for i in 1..=CURVE_SEGMENTS {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let mt = 1.0 - t;
            let (a, b, c) = (mt * mt, 2.0 * mt * t, t * t);
            self.path_points.push((
                a * p0.0 + b * p1.0 + c * p2.0,
                a * p0.1 + b * p1.1 + c * p2.1,
            ));
        }
    }

    /// `ctx.closePath()` — a no-op: [`Self::fill`]'s fan triangulation
    /// already treats `path_points` as an implicitly closed polygon (its
    /// last point connects back to the first), so there's no separate
    /// "closing segment" state to track.
    pub fn close_path(&mut self) {}

    /// `ctx.fill()` — fills the current path with the plain `fill_style`
    /// color via fan triangulation from `path_points[0]` (`(p0, p[i],
    /// p[i+1])` for each `i` in `1..len-1`). **Exact only for convex
    /// polygons** - a concave path will paint the wrong region (some
    /// fan triangles fall outside the intended shape), since this crate
    /// has no general polygon triangulator (ear-clipping or similar). No
    /// gradient fill for paths (unlike `fillRect` - a documented, narrower
    /// cut to avoid duplicating the gradient-projection logic for
    /// arbitrary triangle geometry). Curves (`arc`/`bezierCurveTo`/
    /// `quadraticCurveTo`) are pre-sampled into straight `path_points`
    /// segments by their own methods before ever reaching this fan
    /// triangulation — no curve-aware fill logic lives here. Does nothing
    /// on fewer than 3 points.
    pub fn fill(&mut self) {
        if self.path_points.len() < 3 {
            return;
        }
        let (tx, ty) = (self.translate_x, self.translate_y);
        let (vw, vh) = (self.width as f32, self.height as f32);
        let color = color_to_f32(self.fill_style);
        let p0 = self.path_points[0];
        let mut vertices = Vec::with_capacity((self.path_points.len() - 2) * 3);
        for pair in self.path_points[1..].windows(2) {
            let (p1, p2) = (pair[0], pair[1]);
            for p in [p0, p1, p2] {
                vertices.push(Vertex {
                    position: point_to_ndc(p.0 + tx, p.1 + ty, vw, vh),
                    color,
                });
            }
        }
        self.submit_vertices(&vertices, false);
    }
}
