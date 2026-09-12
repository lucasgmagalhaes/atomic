// The submit button's click is dispatched by ChromeEngine::handle_click
// same as everything else, but the URL text itself never passes through
// JS at all — the host reads the input's live .value directly off the DOM
// (see ChromeEngine::input_value) before dispatching this click, since
// that's simpler than adding a native function purely to hand a string
// back that Rust already has direct access to. This listener exists only
// so a script running inside the bundle could react too, if one ever
// needs to (e.g. clearing the field) — nothing currently does.
document.getElementById('panel').addEventListener('click', function (e) {
    var id = e.target && e.target.id;
    if (id === 'download-submit') {
        document.getElementById('download-url').value = '';
    }
});
