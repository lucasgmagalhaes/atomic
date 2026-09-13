// Static downloads/history chrome built once at load time with the shared
// `El`/`Button`/`Row`/`List` kit (`lib/ui.js`). The submit button's click
// is dispatched by `ChromeEngine::handle_click` same as everything else,
// but the URL text itself never passes through JS at all — the host reads
// the input's live `.value` directly off the DOM (see
// `ChromeEngine::input_value`) before dispatching this click, since that's
// simpler than adding a native function purely to hand a string back that
// Rust already has direct access to. This listener only clears the field
// after submit.
const panel = document.getElementById('panel');
panel.appendChild(
    Row({ id: 'download-row' }, [
        El('input', { id: 'download-url', type: 'text', placeholder: 'URL to download' }, []),
        Button(
            {
                id: 'download-submit',
                onClick: () => {
                    document.getElementById('download-url').value = '';
                },
            },
            ['Download']
        ),
    ])
);
panel.appendChild(
    El('div', { id: 'downloads-section' }, [
        El('div', { className: 'section-title' }, ['Downloads']),
        El('div', { id: 'downloads-list' }, []),
    ])
);
panel.appendChild(
    El('div', { id: 'history-section' }, [
        El('div', { className: 'section-title' }, ['History']),
        El('div', { id: 'history-list' }, []),
    ])
);

// Called by `ChromeEngine::sync_downloads_history` with already-formatted
// display lines (the caller decides formatting, same as `side_panel_ui.rs`'s
// egui version does per-record).
function renderDownloads(lines) {
    document
        .getElementById('downloads-list')
        .replaceChildren(List({ items: lines, emptyText: 'No downloads yet.', renderItem: (line) => Row({}, [line]) }));
}

function renderHistory(lines) {
    document
        .getElementById('history-list')
        .replaceChildren(List({ items: lines, emptyText: 'No history yet.', renderItem: (line) => Row({}, [line]) }));
}
