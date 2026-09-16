//! CEF-backed replacement for `profile::bin::profile_worker` — same CLI
//! contract (`shmem_name width height [proxy] [dns_server] [gpu_adapter]`)
//! and the same newline-delimited stdin/stdout command protocol
//! `crates/profile/src/{navigation,interaction,scripting}.rs` speak, so
//! `crates/profile`'s `Profile` struct can spawn this binary as a drop-in
//! replacement for the old engine's worker (see `spec/ROADMAP.md` P1).
//!
//! Real CEF underneath, not a reimplementation: `PING`/`NAVIGATE`/`RELOAD`
//! use CEF's own load pipeline (`LoadHandler::on_load_end`/`on_load_error`);
//! `CLICK`/`FILL`/`EVAL` all go through the Chrome DevTools Protocol's
//! `Runtime.evaluate` via `CefBrowserHost::execute_dev_tools_method` — CDP
//! is CEF's own generic bidirectional JS-eval-with-return-value channel,
//! and using it for `CLICK`/`FILL` too mirrors the *old* engine's own
//! design (`crates/profile`'s top-level doc: "click builds and evals one
//! hardcoded dispatch snippet, fill a hardcoded assignment"), just against
//! CEF's real DOM instead of a `#id`-only one — a real capability
//! improvement (any CSS selector works now), not a regression.
//! `CONSOLE` subscribes to CDP's `Runtime.consoleAPICalled` event.
//!
//! Scope cut, honest not silent: `CLICK_AT`/`MOUSE_MOVE`/`DRAG_START`/
//! `DROP_AT`/`CONTEXT_MENU_AT`/`COMPOSITION_*`/`KEY`/`TAB`/`TAB_REVERSE`/
//! `SCROLL`/`RESIZE`/`SET_FPS_CAP`/`PAUSE`/`RESUME` aren't implemented yet
//! — each replies `ERROR not yet implemented in the CEF-backed worker`
//! rather than silently no-opping or faking success. See
//! `spec/ROADMAP.md` P1 for the plan to close these (mostly real
//! `CefBrowserHost` input-injection APIs — `send_mouse_click_event`,
//! `send_key_event`, `ime_commit_text`, etc. — already exist in the CEF
//! API, just not wired here yet).
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use cef::rc::Rc as _;
use cef::{args::Args, *};

/// One request from the stdin-reading thread to the CEF-owning main/UI
/// thread, paired with the channel to send exactly one reply line back.
struct WorkerCommand {
    line: String,
    reply: mpsc::Sender<String>,
}

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
struct WorkerRenderHandler {
    width: i32,
    height: i32,
    captured: Rc<RefCell<Option<Vec<u8>>>>,
}

cef::wrap_render_handler! {
    pub struct RenderHandlerBuilder {
        handler: WorkerRenderHandler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                rect.width = self.handler.width;
                rect.height = self.handler.height;
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
            *self.handler.captured.borrow_mut() = Some(bytes);
        }
    }
}

impl RenderHandlerBuilder {
    fn build(handler: WorkerRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

/// Resolved by `on_load_end`/`on_load_error` for whichever navigation is
/// currently in flight — only one `NAVIGATE`/`RELOAD` is ever pending at a
/// time (the protocol is synchronous: a caller waits for one reply before
/// sending the next command), so a single slot is enough.
type PendingNav = Arc<Mutex<Option<mpsc::Sender<Result<(), String>>>>>;

#[derive(Clone)]
struct WorkerLoadHandler {
    pending: PendingNav,
}

cef::wrap_load_handler! {
    pub struct LoadHandlerBuilder {
        handler: WorkerLoadHandler,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: ::std::os::raw::c_int,
        ) {
            if let Some(frame) = frame {
                if frame.is_main() == 0 {
                    return;
                }
            }
            if let Some(tx) = self.handler.pending.lock().unwrap().take() {
                let _ = tx.send(Ok(()));
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if let Some(frame) = frame {
                if frame.is_main() == 0 {
                    return;
                }
            }
            // CEF reports a benign "aborted" pseudo-error for any
            // navigation a later one supersedes - not a real failure a
            // caller should see as ERROR (a fast NAVIGATE-then-NAVIGATE
            // would otherwise always report the first as failed).
            let text = error_text.map(|s| s.to_string()).unwrap_or_default();
            if text.contains("ERR_ABORTED") {
                return;
            }
            let url = failed_url.map(|s| s.to_string()).unwrap_or_default();
            if let Some(tx) = self.handler.pending.lock().unwrap().take() {
                let _ = tx.send(Err(format!("{text} ({url})")));
            }
        }
    }
}

impl LoadHandlerBuilder {
    fn build(handler: WorkerLoadHandler) -> LoadHandler {
        Self::new(handler)
    }
}

/// One in-flight `execute_dev_tools_method` call, correlated by CDP's own
/// integer message id.
type PendingEvals = Arc<Mutex<HashMap<i32, mpsc::Sender<Result<String, String>>>>>;
/// Drained by `CONSOLE` — appended to by `on_dev_tools_event` whenever a
/// `Runtime.consoleAPICalled` event arrives.
type ConsoleLog = Arc<Mutex<Vec<(String, String)>>>;

#[derive(Clone)]
struct WorkerDevTools {
    pending: PendingEvals,
    console: ConsoleLog,
}

cef::wrap_dev_tools_message_observer! {
    pub struct DevToolsObserverBuilder {
        handler: WorkerDevTools,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: ::std::os::raw::c_int,
            success: ::std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            let Some(tx) = self.handler.pending.lock().unwrap().remove(&message_id) else {
                return;
            };
            let body: serde_json::Value = result
                .and_then(|bytes| serde_json::from_slice(bytes).ok())
                .unwrap_or(serde_json::Value::Null);
            if success == 0 {
                let _ = tx.send(Err(format!("devtools call failed: {body}")));
                return;
            }
            if let Some(message) = eval_exception_message(&body) {
                let _ = tx.send(Err(message));
                return;
            }
            let _ = tx.send(Ok(eval_result_to_string(&body)));
        }

        fn on_dev_tools_event(
            &self,
            _browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            let Some(method) = method else { return };
            if method.to_string() != "Runtime.consoleAPICalled" {
                return;
            }
            let Some(params) = params else { return };
            let Ok(body) = serde_json::from_slice::<serde_json::Value>(params) else {
                return;
            };
            let level = body.get("type").and_then(|v| v.as_str()).unwrap_or("log").to_string();
            let text = body
                .get("args")
                .and_then(|a| a.as_array())
                .map(|args| {
                    args.iter()
                        .map(|a| {
                            a.get("value")
                                .map(json_scalar_to_string)
                                .or_else(|| a.get("description").and_then(|d| d.as_str()).map(str::to_string))
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            self.handler.console.lock().unwrap().push((level, text));
        }
    }
}

impl DevToolsObserverBuilder {
    fn build(handler: WorkerDevTools) -> DevToolsMessageObserver {
        Self::new(handler)
    }
}

/// `Runtime.evaluate`'s response shape: `{"result": {"type": ..., "value":
/// ...}, "exceptionDetails": {...}}`. A thrown exception is reported as an
/// `Err` string (from `exceptionDetails`); otherwise the completion
/// value's own JSON `value` is stringified — the closest real match to
/// the old engine's "stringified completion value" contract.
fn eval_result_to_string(body: &serde_json::Value) -> String {
    if let Some(value) = body.get("result").and_then(|r| r.get("value")) {
        return json_scalar_to_string(value);
    }
    String::new()
}

fn json_scalar_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn eval_exception_message(body: &serde_json::Value) -> Option<String> {
    body.get("exceptionDetails").map(|details| {
        details
            .get("exception")
            .and_then(|e| e.get("description").or_else(|| e.get("value")))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| details.to_string())
    })
}

#[derive(Clone)]
struct SmokeClient {
    render_handler: RenderHandler,
    load_handler: LoadHandler,
}

cef::wrap_client! {
    pub(crate) struct ClientBuilder {
        client: SmokeClient,
    }

    impl Client {
        fn render_handler(&self) -> Option<cef::RenderHandler> {
            Some(self.client.render_handler.clone())
        }

        fn load_handler(&self) -> Option<cef::LoadHandler> {
            Some(self.client.load_handler.clone())
        }
    }
}

impl ClientBuilder {
    fn build(client: SmokeClient) -> Client {
        Self::new(client)
    }
}

/// Real per-request-context isolation: two workers with different
/// `shmem_name`s (always the case - see `crates/profile`'s own doc on why
/// two profiles must never share one) get different `cache_path`
/// directories, so cookies/localStorage/IndexedDB/HTTP cache never cross
/// between them. See `spec/ROADMAP.md` P0's still-open item on the
/// "Cannot create profile at path" log CEF emits for this - tracked, not
/// silently assumed fine here either.
fn profile_cache_dir(shmem_name: &str) -> std::path::PathBuf {
    let safe_name: String = shmem_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!("atomic-profile-{safe_name}"))
}

/// Sends `expression` via CDP `Runtime.evaluate` and returns immediately
/// with the receiver its result (or failure) arrives on — deliberately
/// non-blocking: `on_dev_tools_method_result` is only ever delivered
/// *through* `do_message_loop_work()`, so a caller must keep pumping that
/// while waiting on the receiver, exactly like [`NAVIGATE`'s own
/// wait loop in `main`] — blocking here directly (an earlier version of
/// this function did, via `rx.recv_timeout`) starves the message loop and
/// every `EVAL`/`CLICK`/`FILL` call hangs until its own timeout, a real
/// bug this fixes (see `crates/automation`'s own `execute_pending_job`
/// ordering gotcha for the same class of mistake).
fn send_devtools_eval(
    host: &BrowserHost,
    next_message_id: &mut i32,
    pending: &PendingEvals,
    expression: &str,
) -> Result<mpsc::Receiver<Result<String, String>>, String> {
    let id = *next_message_id;
    *next_message_id += 1;
    let (tx, rx) = mpsc::channel();
    pending.lock().unwrap().insert(id, tx);

    let mut params = dictionary_value_create().expect("dictionary_value_create");
    params.set_string(Some(&"expression".into()), Some(&expression.into()));
    params.set_bool(Some(&"returnByValue".into()), true as _);
    params.set_bool(Some(&"awaitPromise".into()), false as _);

    let ok = host.execute_dev_tools_method(id, Some(&"Runtime.evaluate".into()), Some(&mut params));
    if ok == 0 {
        pending.lock().unwrap().remove(&id);
        return Err("execute_dev_tools_method failed to send".to_string());
    }
    Ok(rx)
}

/// Pumps `do_message_loop_work()` until `rx` resolves or `timeout`
/// elapses — the shared wait primitive `NAVIGATE`/`RELOAD`/`EVAL`/`CLICK`/
/// `FILL` all use, since every one of them completes via a callback only
/// delivered through the message loop.
fn pump_until<T>(rx: &mpsc::Receiver<Result<T, String>>, timeout: Duration) -> Result<T, String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        do_message_loop_work();
        match rx.try_recv() {
            Ok(result) => return result,
            Err(mpsc::TryRecvError::Empty) => {
                if std::time::Instant::now() > deadline {
                    return Err("timed out".to_string());
                }
                std::thread::sleep(Duration::from_millis(8));
            }
            Err(mpsc::TryRecvError::Disconnected) => return Err("handler dropped".to_string()),
        }
    }
}

fn js_string_literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// What `handle_command` produced: either a reply ready to send back
/// immediately, or a request that's been sent to CEF and needs the
/// message loop pumped (see [`pump_until`]) before a real reply exists.
enum CommandOutcome {
    Immediate(String),
    PendingNav(mpsc::Receiver<Result<(), String>>),
    /// `bool` is whether a success should be reported as `EVALUATED
    /// {value}` (real `EVAL`) or a bare `OK` (`CLICK`/`FILL`, which don't
    /// surface their eval's own return value to the wire protocol).
    PendingEval(mpsc::Receiver<Result<String, String>>, bool),
}

#[allow(clippy::too_many_arguments)]
fn handle_command(
    line: &str,
    browser: &Browser,
    host: &BrowserHost,
    frame: &mut Frame,
    next_message_id: &mut i32,
    pending: &PendingEvals,
    console: &ConsoleLog,
    pending_nav: &PendingNav,
) -> CommandOutcome {
    let mut parts = line.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");

    match cmd {
        "PING" => CommandOutcome::Immediate("PONG".to_string()),
        "NAVIGATE" | "RELOAD" => {
            let (tx, rx) = mpsc::channel();
            *pending_nav.lock().unwrap() = Some(tx);
            if cmd == "NAVIGATE" {
                frame.load_url(Some(&rest.into()));
            } else {
                browser.reload();
            }
            CommandOutcome::PendingNav(rx)
        }
        "EVAL" => match send_devtools_eval(host, next_message_id, pending, rest) {
            Ok(rx) => CommandOutcome::PendingEval(rx, true),
            Err(message) => CommandOutcome::Immediate(format!("ERROR {message}")),
        },
        "CLICK" => {
            let selector = js_string_literal(rest);
            let expr = format!(
                "(function(){{var el=document.querySelector({selector});if(!el)throw new Error('no such element: '+{selector});el.click();return true;}})()"
            );
            match send_devtools_eval(host, next_message_id, pending, &expr) {
                Ok(rx) => CommandOutcome::PendingEval(rx, false),
                Err(message) => CommandOutcome::Immediate(format!("ERROR {message}")),
            }
        }
        "FILL" => {
            let mut fill_parts = rest.splitn(2, ' ');
            let selector = fill_parts.next().unwrap_or("");
            let value = fill_parts.next().unwrap_or("");
            let selector_lit = js_string_literal(selector);
            let value_lit = js_string_literal(value);
            let expr = format!(
                "(function(){{var el=document.querySelector({selector_lit});if(!el)throw new Error('no such element: '+{selector_lit});if('value' in el && (el.tagName==='INPUT'||el.tagName==='TEXTAREA')){{el.value={value_lit};}}else{{el.textContent={value_lit};}}return true;}})()"
            );
            match send_devtools_eval(host, next_message_id, pending, &expr) {
                Ok(rx) => CommandOutcome::PendingEval(rx, false),
                Err(message) => CommandOutcome::Immediate(format!("ERROR {message}")),
            }
        }
        "CONSOLE" => {
            let mut log = console.lock().unwrap();
            let reply = if log.is_empty() {
                "MESSAGES".to_string()
            } else {
                let joined = log
                    .drain(..)
                    .map(|(level, text)| format!("{level}:{text}"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("MESSAGES {joined}")
            };
            CommandOutcome::Immediate(reply)
        }
        _ => CommandOutcome::Immediate(
            "ERROR not yet implemented in the CEF-backed worker".to_string(),
        ),
    }
}

/// Matches `profile::bin::profile_worker::document_load::DEMO_HTML`'s
/// structure (a `#counter` element in particular — several tests and
/// `crates/atomic`'s own automation examples fill/click it) so a freshly
/// spawned pane shows *something* real before the caller's first real
/// `NAVIGATE`, same as the old engine's own default. A `data:` URL, not a
/// second in-process HTML string this worker parses itself — CEF handles
/// it as a genuine navigation like any other.
const DEMO_URL: &str = "data:text/html,%3Cdiv%20id%3D%22container%22%3E%3Cp%3EAtomic%20profile%20worker%3C%2Fp%3E%3Cp%3ERendering%20real%20HTML%20via%20a%20real%20Chromium%2C%20not%20html5ever.%3C%2Fp%3E%3Cp%20id%3D%22counter%22%3Etick%200%3C%2Fp%3E%3C%2Fdiv%3E";

fn bgra_to_rgba(bgra: &mut [u8]) {
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

fn main() -> std::process::ExitCode {
    // Must run before any other `cef::*` call - including `Args::new()`'s
    // `as_cmd_line()` a few lines down - or CEF's own C-API ABI
    // negotiation state stays unset and the very first real call aborts
    // (see `crates/cef/README.md`'s "known gotcha" section, hit once
    // already writing `cef_smoke`).
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
        // A CEF subprocess re-execs this same binary with its own
        // `--type=...`/`--user-data-dir=...`/etc argv - parsing *our*
        // positional argv contract (shmem_name/width/height/...) before
        // this check misread a subprocess's CEF-internal flags as ours in
        // testing (harmless in practice since this branch never uses
        // them, but noisy and wrong) - argv parsing below only runs for
        // the real browser process now.
        assert!(ret >= 0, "cef subprocess failed to launch");
        return std::process::ExitCode::from(0);
    }
    assert_eq!(ret, -1, "cef refused to run this as the browser process");

    // Matches `profile::Profile::spawn_full`'s exact positional argv
    // contract: shmem_name width height [proxy] [dns_server] [gpu_adapter].
    // `proxy`/`dns_server`/`gpu_adapter` are accepted but not yet wired to
    // real CEF behavior - see spec/ROADMAP.md P1's still-open proxy/DNS/GPU
    // items; flagged here, not silently ignored without a trace.
    let cli_args: Vec<String> = std::env::args().collect();
    let shmem_name = cli_args.get(1).cloned();
    let width: i32 = cli_args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let height: i32 = cli_args.get(3).and_then(|s| s.parse().ok()).unwrap_or(768);
    if let Some(proxy) = cli_args.get(4).filter(|s| !s.is_empty()) {
        eprintln!("cef_profile_worker: proxy arg ({proxy}) accepted but not yet implemented");
    }
    if let Some(dns) = cli_args.get(5).filter(|s| !s.is_empty()) {
        eprintln!("cef_profile_worker: dns_server arg ({dns}) accepted but not yet implemented");
    }

    let Some(shmem_name) = shmem_name else {
        eprintln!(
            "usage: cef_profile_worker <shmem_name> <width> <height> [proxy] [dns_server] [gpu_adapter]"
        );
        return std::process::ExitCode::from(2);
    };

    // The *real* bug behind both the "Cannot create profile at path" log
    // and an eventual "GPU process isn't usable. Goodbye" fatal crash when
    // more than one worker ran concurrently: `RequestContextSettings`'s
    // `cache_path` (set below) only names a profile *within* a top-level
    // CEF user-data directory - it's not itself the top-level directory.
    // Leaving `Settings::cache_path`/`root_cache_path` unset here made
    // every worker process default to the *same* shared
    // `%LOCALAPPDATA%\CEF\User Data`, so their GPU disk caches collided
    // (a real, reproducible lock-contention crash, not a cosmetic log
    // line). Each worker is already its own OS process (this project's
    // per-profile isolation unit - see spec/architecture/isolation-and-
    // perf.md), so giving each one its own top-level `cache_path` here is
    // both the fix and the correct isolation boundary; `RequestContext`
    // then just uses the default profile under it instead of naming a
    // second, redundant path.
    let cache_dir = profile_cache_dir(&shmem_name);
    let settings = Settings {
        windowless_rendering_enabled: true as _,
        no_sandbox: true as _,
        cache_path: CefString::from(cache_dir.to_string_lossy().as_ref()),
        persist_session_cookies: 1,
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

    let mut context: Option<RequestContext> = request_context_create_context(None, None);

    let captured = Rc::new(RefCell::new(None));
    let render_handler = RenderHandlerBuilder::build(WorkerRenderHandler {
        width,
        height,
        captured: captured.clone(),
    });
    let pending_nav: PendingNav = Arc::new(Mutex::new(None));
    let load_handler = LoadHandlerBuilder::build(WorkerLoadHandler {
        pending: pending_nav.clone(),
    });
    let mut client = ClientBuilder::build(SmokeClient {
        render_handler,
        load_handler,
    });

    let window_info = WindowInfo {
        windowless_rendering_enabled: true as _,
        ..Default::default()
    };
    let browser_settings = BrowserSettings {
        windowless_frame_rate: 60,
        ..Default::default()
    };
    let browser = browser_host_create_browser_sync(
        Some(&window_info),
        Some(&mut client),
        Some(&DEMO_URL.into()),
        Some(&browser_settings),
        None,
        context.as_mut(),
    );
    assert!(
        browser.is_some(),
        "browser_host_create_browser_sync returned None"
    );
    let browser = browser.unwrap();

    let pending_evals: PendingEvals = Arc::new(Mutex::new(HashMap::new()));
    let console_log: ConsoleLog = Arc::new(Mutex::new(Vec::new()));
    let dev_tools = DevToolsObserverBuilder::build(WorkerDevTools {
        pending: pending_evals.clone(),
        console: console_log.clone(),
    });
    let mut dev_tools_observer: DevToolsMessageObserver = dev_tools;
    let Some(host) = browser.host() else {
        eprintln!("cef_profile_worker: browser has no host");
        return std::process::ExitCode::from(1);
    };
    let _registration = host.add_dev_tools_message_observer(Some(&mut dev_tools_observer));
    let mut next_message_id: i32 = 1;
    // Real console capture needs `Runtime.enable` first - CDP domains are
    // opt-in, matching how a real devtools client (Playwright/Puppeteer
    // included) must enable the domain before its events start flowing.
    {
        let mut params = dictionary_value_create().expect("dictionary_value_create");
        let id = next_message_id;
        next_message_id += 1;
        let _ =
            host.execute_dev_tools_method(id, Some(&"Runtime.enable".into()), Some(&mut params));
    }

    let mut frame_writer: Option<ipc::FrameWriter> = None;

    let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCommand>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        while let Some(Ok(line)) = lines.next() {
            let (reply_tx, reply_rx) = mpsc::channel();
            if cmd_tx
                .send(WorkerCommand {
                    line,
                    reply: reply_tx,
                })
                .is_err()
            {
                break;
            }
            match reply_rx.recv() {
                Ok(reply) => {
                    let mut stdout = std::io::stdout();
                    let _ = writeln!(stdout, "{reply}");
                    let _ = stdout.flush();
                }
                Err(_) => break,
            }
        }
    });

    let mut quitting = false;
    'outer: loop {
        do_message_loop_work();

        if let Some(mut pixels) = captured.borrow_mut().take() {
            bgra_to_rgba(&mut pixels);
            if frame_writer.is_none() {
                frame_writer = ipc::FrameWriter::new(&shmem_name, width as u32, height as u32).ok();
            }
            if let Some(writer) = frame_writer.as_mut() {
                writer.publish(&pixels);
            }
        }

        while let Ok(command) = cmd_rx.try_recv() {
            let line = command.line.trim().to_string();
            if line == "QUIT" {
                let _ = command.reply.send("OK".to_string());
                quitting = true;
                break;
            }
            let outcome = {
                let Some(mut frame) = browser.main_frame() else {
                    let _ = command.reply.send("ERROR no main frame".to_string());
                    continue;
                };
                handle_command(
                    &line,
                    &browser,
                    &host,
                    &mut frame,
                    &mut next_message_id,
                    &pending_evals,
                    &console_log,
                    &pending_nav,
                )
            };
            // Every pending variant resolves via a callback only delivered
            // through the message loop `pump_until` keeps pumping - see
            // that function's own doc for why this can't just block on the
            // receiver directly (a real bug an earlier version of this
            // file had for EVAL/CLICK/FILL specifically).
            let reply = match outcome {
                CommandOutcome::Immediate(reply) => reply,
                CommandOutcome::PendingNav(rx) => match pump_until(&rx, Duration::from_secs(30)) {
                    Ok(()) => "OK".to_string(),
                    Err(message) => format!("ERROR {message}"),
                },
                CommandOutcome::PendingEval(rx, report_value) => {
                    match pump_until(&rx, Duration::from_secs(10)) {
                        Ok(value) if report_value => format!("EVALUATED {value}"),
                        Ok(_) => "OK".to_string(),
                        Err(message) => format!("ERROR {message}"),
                    }
                }
            };
            let _ = command.reply.send(reply);
        }

        if quitting {
            break 'outer;
        }
        std::thread::sleep(Duration::from_millis(8));
    }

    if let Some(host) = browser.host() {
        host.close_browser(true as _);
    }
    drop(browser);
    for _ in 0..60 {
        do_message_loop_work();
        std::thread::sleep(Duration::from_millis(16));
    }
    shutdown();
    std::process::ExitCode::from(0)
}
