mod common;

use common::pixel;
use layout_engine::Color;
use render::{Canvas2D, FillGradient, LinearGradient, RadialGradient};

#[test]
fn linear_gradient_is_mostly_start_color_near_the_gradient_line_start() {
    // A wide canvas so the near-start pixel's own center is close enough to
    // t=0 that its blend-with-end-color fraction is negligible - the exact
    // corner vertex color (t=0) isn't what any *pixel* shows, since a pixel
    // center sits half a pixel in from the rect's true edge.
    let mut canvas = Canvas2D::new(200, 1);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 200.0,
        y1: 0.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    canvas.fill_rect(0.0, 0.0, 200.0, 1.0);

    let pixels = canvas.get_image_data();
    let [r, g, b, a] = pixel(&pixels, 200, 0, 0);
    assert!(r > 250, "expected near-start pixel mostly red, got r={r}");
    assert_eq!(g, 0);
    assert!(b < 10, "expected near-start pixel barely blue, got b={b}");
    assert_eq!(a, 255);
}

#[test]
fn linear_gradient_is_mostly_end_color_near_the_gradient_line_end() {
    let mut canvas = Canvas2D::new(200, 1);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 200.0,
        y1: 0.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    canvas.fill_rect(0.0, 0.0, 200.0, 1.0);

    let pixels = canvas.get_image_data();
    let [r, g, b, a] = pixel(&pixels, 200, 199, 0);
    assert!(b > 250, "expected near-end pixel mostly blue, got b={b}");
    assert_eq!(g, 0);
    assert!(r < 10, "expected near-end pixel barely red, got r={r}");
    assert_eq!(a, 255);
}

#[test]
fn set_fill_style_after_a_gradient_reverts_to_solid_color() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 4.0,
        y1: 0.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    assert!(canvas.fill_gradient().is_none());
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&pixels, 4, 3, 3), [0, 255, 0, 255]);
}

#[test]
fn restore_undoes_a_gradient_fill_style_set_since_the_matching_save() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.save();
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 4.0,
        y1: 0.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    canvas.restore();
    assert!(canvas.fill_gradient().is_none());
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 255, 0, 255]);
}

#[test]
fn radial_gradient_paints_start_color_at_its_own_center() {
    // A large radius so the sampled pixel's own center (half a pixel off
    // the true math center, same reason `linear_gradient_is_mostly_start_
    // color_near_the_gradient_line_start` needs a wide canvas) contributes
    // a negligible blend fraction.
    let mut canvas = Canvas2D::new(400, 400);
    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 200.0,
        cy: 200.0,
        radius: 200.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    canvas.fill_rect(0.0, 0.0, 400.0, 400.0);

    let pixels = canvas.get_image_data();
    let [r, g, b, a] = pixel(&pixels, 400, 200, 200);
    assert!(r > 250, "expected center pixel mostly red, got r={r}");
    assert_eq!(g, 0);
    assert!(b < 15, "expected center pixel barely blue, got b={b}");
    assert_eq!(a, 255);
}

#[test]
fn radial_gradient_paints_end_color_at_its_own_edge() {
    let mut canvas = Canvas2D::new(20, 20);
    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 10.0,
        cy: 10.0,
        radius: 10.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    canvas.fill_rect(0.0, 0.0, 20.0, 20.0);

    let pixels = canvas.get_image_data();
    // Corner (0,0) is at distance ~14.1 from center (10,10) - beyond the
    // radius, so clamped to the end color.
    let [r, g, b, a] = pixel(&pixels, 20, 0, 0);
    assert!(b > 200, "expected corner pixel mostly blue, got b={b}");
    assert_eq!(g, 0);
    assert!(r < 30, "expected corner pixel barely red, got r={r}");
    assert_eq!(a, 255);
}

#[test]
fn radial_gradient_is_a_true_circle_not_a_diamond() {
    // Regression guard for the exact-per-pixel-shader approach: sample
    // two points equidistant from the center along different axes (one
    // axis-aligned, one diagonal) and confirm they blend identically -
    // a naive 4-corner-color-interpolation approximation would not
    // (it'd be diamond-shaped, not circular).
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 20.0,
        cy: 20.0,
        radius: 20.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    canvas.fill_rect(0.0, 0.0, 40.0, 40.0);

    let pixels = canvas.get_image_data();
    // (30, 20) is 10px right of center; (27, 27) is ~9.9px from center
    // diagonally - both should be at roughly the same blend.
    let axis = pixel(&pixels, 40, 30, 20);
    let diagonal = pixel(&pixels, 40, 27, 27);
    let diff = (axis[2] as i32 - diagonal[2] as i32).abs();
    assert!(
        diff < 15,
        "expected a true circle (similar blend at similar distance), axis={axis:?} diagonal={diagonal:?}"
    );
}

#[test]
fn set_fill_gradient_accepts_both_linear_and_radial() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 10.0,
        y1: 0.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    assert!(matches!(
        canvas.fill_gradient(),
        Some(FillGradient::Linear(_))
    ));

    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 5.0,
        cy: 5.0,
        radius: 5.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    assert!(matches!(
        canvas.fill_gradient(),
        Some(FillGradient::Radial(_))
    ));
}
