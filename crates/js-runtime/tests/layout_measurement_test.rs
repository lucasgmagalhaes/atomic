use js_runtime::{Context, Rect, Runtime};

#[test]
fn get_bounding_client_rect_reflects_a_pushed_layout_rect() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(root, div);

  let rt = Runtime::new();
  let mut ctx = Context::with_dom(&rt, d);
  let mut rects = std::collections::HashMap::new();
  rects.insert(
    div,
    Rect {
      x: 10.0,
      y: 20.0,
      width: 100.0,
      height: 50.0,
    },
  );
  ctx.set_layout_rects(rects);

  let result = ctx
        .eval(
            "(() => { \
                const r = document.getElementById('box').getBoundingClientRect(); \
                return `${r.x},${r.y},${r.width},${r.height},${r.top},${r.left},${r.right},${r.bottom}`; \
            })()",
            "<test>",
        )
        .unwrap();
  assert_eq!(result, "10,20,100,50,20,10,110,70");
}

#[test]
fn offset_and_client_dimensions_reflect_the_same_pushed_rect() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(root, div);

  let rt = Runtime::new();
  let mut ctx = Context::with_dom(&rt, d);
  let mut rects = std::collections::HashMap::new();
  rects.insert(
    div,
    Rect {
      x: 5.0,
      y: 7.0,
      width: 200.0,
      height: 80.0,
    },
  );
  ctx.set_layout_rects(rects);

  let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('box'); \
                return `${el.offsetWidth},${el.offsetHeight},${el.offsetTop},${el.offsetLeft},${el.clientWidth},${el.clientHeight}`; \
            })()",
            "<test>",
        )
        .unwrap();
  assert_eq!(result, "200,80,7,5,200,80");
}

#[test]
fn a_node_never_laid_out_reads_an_all_zero_rect() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
    .eval(
      "(() => { \
                const r = document.getElementById('box').getBoundingClientRect(); \
                return `${r.x},${r.y},${r.width},${r.height}`; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "0,0,0,0");
}

#[test]
fn set_layout_rects_replaces_the_whole_map() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let a = d.create_element("div");
  d.set_attribute(a, "id", "a");
  d.append_child(root, a);
  let b = d.create_element("div");
  d.set_attribute(b, "id", "b");
  d.append_child(root, b);

  let rt = Runtime::new();
  let mut ctx = Context::with_dom(&rt, d);
  let mut first = std::collections::HashMap::new();
  first.insert(
    a,
    Rect {
      x: 1.0,
      y: 1.0,
      width: 1.0,
      height: 1.0,
    },
  );
  ctx.set_layout_rects(first);

  let mut second = std::collections::HashMap::new();
  second.insert(
    b,
    Rect {
      x: 2.0,
      y: 2.0,
      width: 2.0,
      height: 2.0,
    },
  );
  ctx.set_layout_rects(second);

  let result = ctx
    .eval(
      "(() => { \
                const ra = document.getElementById('a').getBoundingClientRect(); \
                const rb = document.getElementById('b').getBoundingClientRect(); \
                return `${ra.width},${rb.width}`; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(
    result, "0,2",
    "a fresh set_layout_rects call must replace the old map, not merge into it"
  );
}
