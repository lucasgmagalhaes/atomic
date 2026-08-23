//! The per-profile child process `profile::Profile` spawns. Hosts the
//! html+dom+css+layout-engine+render stack for one profile and publishes
//! rendered frames over `ipc::FrameWriter`. Parses real HTML via `html`
//! (html5ever-backed) — no longer a hardcoded DOM tree. Still not a
//! working browser tab: the HTML source is a fixed demo string, not
//! fetched from a URL (`net` isn't wired in here), and there's no
//! `<style>`/`<link>` extraction — the stylesheet is applied separately,
//! not read out of the parsed document.
//!
//! Command protocol over stdin (newline-delimited, one command per line):
//! - `PING` -> replies `PONG` on stdout (liveness check)
//! - `RELOAD` -> re-parses/re-renders the page and re-publishes a frame
//! - `QUIT` -> exits cleanly
//! - anything else -> ignored (unrecognized commands are not an error;
//!   real input/navigation commands aren't implemented yet)
use std::io::{self, BufRead, Write};

use css::parse_stylesheet;
use layout_engine::{build_box_tree, layout_block};
use render::{build_display_list, build_glyph_list, composite_glyphs, GpuRenderer};

const DEMO_HTML: &str = r#"
<div id="container">
  <p>Nimble profile worker</p>
  <p>Rendering real HTML via html5ever.</p>
</div>
"#;

fn render_frame(width: u32, height: u32) -> Vec<u8> {
    let (dom, html_el) = html::parse_to_html_element(DEMO_HTML);
    let sheet = parse_stylesheet(
        "#container { background-color: #1a1c2b; padding: 20px; } \
         p { color: #ffffff; font-size: 24px; }",
    );
    let mut tree = build_box_tree(&dom, html_el, &sheet).expect("parsed HTML always produces a box");
    layout_block(&mut tree, width as f64, 0.0, 0.0);

    let rects = build_display_list(&tree);
    let glyphs = build_glyph_list(&tree);

    let renderer = GpuRenderer::new();
    let mut pixels = renderer.render_to_rgba(&rects, width, height, [0.08, 0.09, 0.13, 1.0]);
    composite_glyphs(&mut pixels, width, height, &glyphs);
    pixels
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: profile-worker <shmem-name> <width> <height>");
        std::process::exit(2);
    }
    let shmem_name = &args[1];
    let width: u32 = args[2].parse().expect("width must be a positive integer");
    let height: u32 = args[3].parse().expect("height must be a positive integer");

    let mut writer = ipc::FrameWriter::new(shmem_name, width, height).expect("failed to create/open shared memory");
    writer.publish(&render_frame(width, height));

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        match line.trim() {
            "PING" => {
                let _ = writeln!(stdout, "PONG");
                let _ = stdout.flush();
            }
            "RELOAD" => {
                writer.publish(&render_frame(width, height));
                let _ = writeln!(stdout, "RELOADED");
                let _ = stdout.flush();
            }
            "QUIT" => break,
            _ => {}
        }
    }
}
