//! Convex-only filled paths - `beginPath`/`moveTo`/`lineTo`/`closePath`/
//! `fill`. See [`Canvas2D::fill`]'s own doc for exactly why only convex
//! polygons render correctly.
use super::helpers::{color_to_f32, point_to_ndc};
use super::pipeline::Vertex;
use super::Canvas2D;

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
    /// `quadraticCurveTo`) aren't supported - only straight `lineTo`
    /// segments. Does nothing on fewer than 3 points.
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
