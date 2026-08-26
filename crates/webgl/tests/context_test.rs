use webgl::{ShaderType, VertexAttribute, WebGl};

// Plain desktop GLSL, not GLSL ES - see `glsl_es_test.rs` for real
// `#version 300 es` source exercising `rewrite_glsl_es_version`. Kept as
// desktop GLSL here since these tests are about program linking/drawing,
// not shader-language compatibility.
const VERTEX_SRC: &str = r#"#version 450 core
layout(location = 0) in vec2 a_position;
void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
}
"#;

const RED_FRAGMENT_SRC: &str = r#"#version 450 core
layout(location = 0) out vec4 fragColor;
void main() {
    fragColor = vec4(1.0, 0.0, 0.0, 1.0);
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
fn compiles_valid_glsl_shaders() {
  let gl = WebGl::new();
  assert!(gl.create_shader(ShaderType::Vertex, VERTEX_SRC).is_ok());
  assert!(gl
    .create_shader(ShaderType::Fragment, RED_FRAGMENT_SRC)
    .is_ok());
}

#[test]
fn invalid_glsl_fails_to_compile_with_an_error_message() {
  let gl = WebGl::new();
  let result = gl.create_shader(ShaderType::Vertex, "this is not glsl at all {{{");
  assert!(result.is_err());
  assert!(!result.unwrap_err().is_empty());
}

#[test]
fn draws_a_fullscreen_triangle_and_reads_back_the_fill_color() {
  let gl = WebGl::new();
  let vs = gl.create_shader(ShaderType::Vertex, VERTEX_SRC).unwrap();
  let fs = gl
    .create_shader(ShaderType::Fragment, RED_FRAGMENT_SRC)
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

  // A single triangle big enough to cover the whole clip-space square.
  let vertices: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
  let pixels = gl.draw_triangles(&program, &vertices, 3, [0.0, 0.0, 0.0, 1.0], 8, 8);

  assert_eq!(pixels.len(), 8 * 8 * 4);
  assert_eq!(pixel(&pixels, 8, 4, 4), [255, 0, 0, 255]);
  assert_eq!(pixel(&pixels, 8, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn zero_vertex_count_just_clears() {
  let gl = WebGl::new();
  let vs = gl.create_shader(ShaderType::Vertex, VERTEX_SRC).unwrap();
  let fs = gl
    .create_shader(ShaderType::Fragment, RED_FRAGMENT_SRC)
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
      8,
    )
    .unwrap();

  let vertices: [f32; 0] = [];
  let pixels = gl.draw_triangles(&program, &vertices, 0, [0.0, 1.0, 0.0, 1.0], 4, 4);

  assert_eq!(pixel(&pixels, 4, 0, 0), [0, 255, 0, 255]);
}

#[test]
fn create_program_rejects_swapped_shader_stages() {
  let gl = WebGl::new();
  let vs = gl.create_shader(ShaderType::Vertex, VERTEX_SRC).unwrap();
  let fs = gl
    .create_shader(ShaderType::Fragment, RED_FRAGMENT_SRC)
    .unwrap();

  let result = gl.create_program(&fs, &vs, &[], 0);
  assert!(result.is_err());
}
