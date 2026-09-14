// Downloads/history-panel component — encapsulated as a function
// returning a real DOM node, mounted onto the shared `#root` container
// (see `../../root.html`). Keeps its own `#panel` id so
// `downloads_history.css`'s selectors need no change from before this
// component-folder split. The submit button's click is dispatched by
// `ChromeEngine::handle_click` same as everything else, but the URL text
// itself never passes through JS at all — the host reads the input's live
// `.value` directly off the DOM (see `ChromeEngine::input_value`) before
// dispatching this click, since that's simpler than adding a native
// function purely to hand a string back that Rust already has direct
// access to. This listener only clears the field after submit.
function DownloadsHistory() {
  return Div(
    { id: "panel" },
    Row(
      { id: "download-row" },
      Input({ id: "download-url", type: "text", placeholder: "URL to download" }),
      Button(
        {
          id: "download-submit",
          onClick: () => {
            document.getElementById("download-url").value = "";
          },
        },
        "Download",
      ),
    ),
    Div({ id: "downloads-section" }, Div({ className: "section-title" }, "Downloads"), Div({ id: "downloads-list" })),
    Div({ id: "history-section" }, Div({ className: "section-title" }, "History"), Div({ id: "history-list" })),
  );
}

mount("root", DownloadsHistory());

// Called by `ChromeEngine::sync_downloads_history` with already-formatted
// display lines (the caller decides formatting, same as `side_panel_ui.rs`'s
// egui version does per-record).
function renderDownloads(lines) {
  mount(
    "downloads-list",
    List({ items: lines, emptyText: "No downloads yet.", renderItem: (line) => Row({}, line) }),
  );
}

function renderHistory(lines) {
  mount(
    "history-list",
    List({ items: lines, emptyText: "No history yet.", renderItem: (line) => Row({}, line) }),
  );
}
