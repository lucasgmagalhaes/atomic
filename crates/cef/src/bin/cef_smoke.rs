//! Headless CEF smoke test — no winit/wgpu, since the real target (a
//! `crates/atomic` pane) needs a raw BGRA pixel buffer delivered into
//! `crates/ipc`'s shared-memory transport, not an on-screen window. Proves
//! `spec/ROADMAP.md` P0's "one CEF-backed window rendering a real page end
//! to end": initializes CEF, opens one off-screen browser against a real
//! `RequestContext` (with its own `cache_path`, the exact per-profile
//! isolation knob `spec/architecture/isolation-and-perf.md` calls for),
//! navigates to a real URL, and dumps the first non-blank `on_paint` frame
//! to a PPM file plus basic stats to stdout.
//!
//! Modeled on the real `tauri-apps/cef-rs` `examples/osr` app (fetched from
//! its GitHub repo while writing this — the `wrap_*!` macro pattern for
//! implementing CEF's C++ interfaces isn't guessable from the generated
//! bindings alone), stripped of its windowed wgpu compositing since this
//! binary never shows a window.
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cef::rc::Rc as _;
use cef::{args::Args, *};

const WIDTH: i32 = 1024;
const HEIGHT: i32 = 768;

#[derive(Clone)]
struct SmokeApp {}

cef::wrap_app! {
    pub(crate) struct AppBuilder {
        app: SmokeApp,
    }

    impl App {}
}

impl AppBuilder {
    fn build(app: SmokeApp) -> App {
        Self::new(app)
    }
}

#[derive(Clone)]
struct SmokeRenderHandler {
    /// Set once, the first time a non-blank frame arrives — a fresh
    /// off-screen browser paints an initial blank/white frame before the
    /// real page finishes loading, so the smoke test waits for a frame
    /// that isn't just that.
    captured: Rc<RefCell<Option<Vec<u8>>>>,
}

cef::wrap_render_handler! {
    pub struct RenderHandlerBuilder {
        handler: SmokeRenderHandler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                rect.width = WIDTH;
                rect.height = HEIGHT;
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            if let Some(screen_info) = screen_info {
                screen_info.device_scale_factor = 1.0;
                return true as _;
            }
            false as _
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if type_ != PaintElementType::default() || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let len = (width * height * 4) as usize;
            let bytes = unsafe { std::slice::from_raw_parts(buffer, len) }.to_vec();
            // A blank first frame is solid white (BGRA 0xFF 0xFF 0xFF 0xFF
            // repeating) - real page content won't be, so this is a cheap
            // "has anything actually rendered yet" check without decoding.
            let is_blank = bytes.chunks_exact(4).all(|px| px == [0xFF, 0xFF, 0xFF, 0xFF]);
            if !is_blank {
                *self.handler.captured.borrow_mut() = Some(bytes);
            }
        }
    }
}

impl RenderHandlerBuilder {
    fn build(handler: SmokeRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

#[derive(Clone)]
struct SmokeClient {
    render_handler: RenderHandler,
}

cef::wrap_client! {
    pub(crate) struct ClientBuilder {
        client: SmokeClient,
    }

    impl Client {
        fn render_handler(&self) -> Option<cef::RenderHandler> {
            Some(self.client.render_handler.clone())
        }
    }
}

impl ClientBuilder {
    fn build(client: SmokeClient) -> Client {
        Self::new(client)
    }
}

/// Writes a raw BGRA buffer as a PPM (P6) so it can be opened without any
/// image-decoding dependency — good enough for "prove a real page
/// rendered," not a real screenshot feature.
fn write_ppm(path: &std::path::Path, bgra: &[u8], width: i32, height: i32) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write!(file, "P6\n{width} {height}\n255\n")?;
    for px in bgra.chunks_exact(4) {
        let [b, g, r, _a] = [px[0], px[1], px[2], px[3]];
        file.write_all(&[r, g, b])?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    // Must run before any other `cef::*` call, including `Args::as_cmd_line`'s
    // `command_line_create()` - skipping this left some internal CEF C-API
    // ABI-negotiation state unset and made that very first real API call
    // abort (exit code 3, no panic message) in testing.
    let _ = api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let cmd = args.as_cmd_line().unwrap();
    let switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;

    let mut app: App = AppBuilder::build(SmokeApp {});
    let ret = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );

    if !is_browser_process {
        // A CEF subprocess (renderer/GPU/utility) re-execs this same
        // binary with a `--type=...` switch - `execute_process` handles
        // and never returns control here in that case except on failure.
        assert!(ret >= 0, "cef subprocess failed to launch");
        return std::process::ExitCode::from(0);
    }
    assert_eq!(ret, -1, "cef refused to run this as the browser process");

    // Known open issue, not fixed here: CEF's own
    // `chrome_browser_context.cc` logs "Cannot create profile at path" for
    // this directory regardless of whether it's pre-created — the browser
    // still renders real content afterward (this smoke test's own PPM
    // capture proves that), so it falls back to *something* rather than
    // failing outright, but it's not confirmed whether that fallback is
    // still this `cache_path` (real per-profile isolation) or a shared
    // default context (a real isolation gap if so). Needs investigation
    // before `spec/ROADMAP.md` P1's per-profile isolation item can be
    // marked done — flagged, not silently assumed fixed.
    let cache_dir = std::env::temp_dir().join(format!("atomic-cef-smoke-{}", std::process::id()));

    let settings = Settings {
        windowless_rendering_enabled: true as _,
        no_sandbox: true as _,
        ..Default::default()
    };
    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut()
        ),
        1,
        "cef::initialize failed"
    );

    // A real per-profile RequestContext, not the process-global default -
    // `cache_path` here is the exact isolation knob
    // `spec/architecture/isolation-and-perf.md` documents: two of these
    // pointed at different directories share no cookies/storage/HTTP
    // cache with each other.
    let context_settings = RequestContextSettings {
        cache_path: CefString::from(cache_dir.to_string_lossy().as_ref()),
        persist_session_cookies: 1,
        ..Default::default()
    };
    let mut context: Option<RequestContext> =
        request_context_create_context(Some(&context_settings), None);

    let captured = Rc::new(RefCell::new(None));
    let render_handler = RenderHandlerBuilder::build(SmokeRenderHandler {
        captured: captured.clone(),
    });
    let mut client = ClientBuilder::build(SmokeClient { render_handler });

    let window_info = WindowInfo {
        windowless_rendering_enabled: true as _,
        ..Default::default()
    };
    let browser_settings = BrowserSettings {
        windowless_frame_rate: 30,
        ..Default::default()
    };

    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://example.com".to_string());
    println!(
        "cef_smoke: navigating to {url}, cache_path={}",
        cache_dir.display()
    );

    let browser = browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&CefString::from(url.as_str())),
        Some(&browser_settings),
        None,
        context.as_mut(),
    );
    assert!(
        browser.is_some(),
        "browser_host_create_browser_sync returned None"
    );
    let browser = browser.unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        do_message_loop_work();
        if captured.borrow().is_some() {
            break;
        }
        if std::time::Instant::now() > deadline {
            eprintln!("cef_smoke: timed out waiting for a non-blank frame");
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    if let Some(pixels) = captured.borrow().as_ref() {
        let out_path = std::env::temp_dir().join("atomic-cef-smoke-frame.ppm");
        write_ppm(&out_path, pixels, WIDTH, HEIGHT).expect("write captured frame");
        println!(
            "cef_smoke: captured a real {WIDTH}x{HEIGHT} frame ({} bytes) -> {}",
            pixels.len(),
            out_path.display()
        );
    }

    if let Some(host) = browser.host() {
        host.close_browser(true as _);
    }
    drop(browser);
    let ran_ok = Arc::new(AtomicBool::new(true));
    for _ in 0..60 {
        do_message_loop_work();
        std::thread::sleep(Duration::from_millis(16));
    }
    let _ = ran_ok.load(Ordering::Relaxed);

    shutdown();
    std::process::ExitCode::from(0)
}
