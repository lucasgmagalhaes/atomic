#![allow(dead_code)]

/// A `<body><canvas></canvas></body>` document with no `width`/`height`
/// attributes (spec defaults `300`/`150` apply) - shared by every
/// `canvas_*_test.rs` file that doesn't need a specific canvas size.
pub(crate) fn dom_with_canvas() -> (dom::Dom, dom::NodeId) {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.append_child(body, canvas);
    (d, canvas)
}
