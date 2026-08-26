use webgl::{ShaderType, VertexAttribute, WebGl};

// Real GLSL ES 3.00 source - `#version 300 es`, mandatory precision
// qualifiers, `in`/`out` - the actual language WebGL2 requires, fed
// through unmodified. This is what `context_test.rs`'s desktop-GLSL
// shaders explicitly said they weren't; these are.
const ES_VERTEX_SRC: &str = r#"#version 300 es
precision highp float;
layout(location = 0) in vec2 a_position;
void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
}
"#;

const ES_FRAGMENT_SRC: &str = r#"#version 300 es
precision mediump float;
layout(location = 0) out vec4 fragColor;
void main() {
    fragColor = vec4(0.0, 0.0, 1.0, 1.0);
}
"#;

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
  let idx = ((y * width + x) * 4) as usize;
  [
    pixels[idx],
    pixels[idx + 1],
    pixels[idx + 2],
    pixels[idx + 3],
  ]
}

#[test]
fn compiles_real_glsl_es_300_shaders() {
  let gl = WebGl::new();
  assert!(gl.create_shader(ShaderType::Vertex, ES_VERTEX_SRC).is_ok());
  assert!(gl
    .create_shader(ShaderType::Fragment, ES_FRAGMENT_SRC)
    .is_ok());
}

#[test]
fn draws_with_a_real_glsl_es_program_and_reads_back_the_fill_color() {
  let gl = WebGl::new();
  let vs = gl.create_shader(ShaderType::Vertex, ES_VERTEX_SRC).unwrap();
  let fs = gl
    .create_shader(ShaderType::Fragment, ES_FRAGMENT_SRC)
    .unwrap();
  let program = gl
    .create_program(
      &vs,
      &fs,
      &[VertexAttribute {
        location: 0,
        components: 2,
        offset: 0,
      }],
      8, // 2 x f32
    )
    .unwrap();

  let vertices: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
  let pixels = gl.draw_triangles(&program, &vertices, 3, [0.0, 0.0, 0.0, 1.0], 8, 8);

  assert_eq!(pixel(&pixels, 8, 4, 4), [0, 0, 255, 255]);
}

#[test]
fn accepts_es_310_and_320_versions_too() {
  let gl = WebGl::new();
  let vs_310 = ES_VERTEX_SRC.replacen("300 es", "310 es", 1);
  let vs_320 = ES_VERTEX_SRC.replacen("300 es", "320 es", 1);
  assert!(gl.create_shader(ShaderType::Vertex, &vs_310).is_ok());
  assert!(gl.create_shader(ShaderType::Vertex, &vs_320).is_ok());
}

#[test]
fn still_reports_real_errors_in_es_source() {
  let gl = WebGl::new();
  let broken =
    "#version 300 es\nprecision highp float;\nvoid main() { this is not valid glsl {{{ }";
  let result = gl.create_shader(ShaderType::Vertex, broken);
  assert!(result.is_err());
  assert!(!result.unwrap_err().is_empty());
}

#[test]
fn plain_desktop_glsl_without_an_es_directive_is_unaffected() {
  let gl = WebGl::new();
  let desktop = "#version 450 core\nlayout(location = 0) out vec4 fragColor;\nvoid main() { fragColor = vec4(1.0); }\n";
  assert!(gl.create_shader(ShaderType::Fragment, desktop).is_ok());
}
