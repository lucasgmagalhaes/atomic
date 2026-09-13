// Static settings chrome built once at load time with the shared `El`/
// `Button`/`Row` kit (`lib/ui.js`) — same reasoning `toolbar.js` documents:
// each button owns its own real `click` listener, so `ChromeEngine`'s
// `dispatch_click` (a real `dispatchEvent("click")` on the resolved
// element) reaches it directly, no delegated root listener needed.
//
// `apply-gpu` and `close-settings` intentionally have no `onClick` here,
// same as the bundle they replace — neither was ever wired to an
// `atomic.*` call in the original `settings.js`.
const settings = document.getElementById('settings');

settings.appendChild(El('div', { className: 'section-title' }, ['Performance']));
settings.appendChild(
    Row({}, [
        El('label', {}, ['Max live panes']),
        Button({ id: 'panes-minus', onClick: () => atomic.stepMaxPanes(-1) }, ['-']),
        El('span', { id: 'panes-value' }, ['1']),
        Button({ id: 'panes-plus', onClick: () => atomic.stepMaxPanes(1) }, ['+']),
    ])
);
settings.appendChild(
    Row({}, [
        El('label', {}, ['Cap frame rate']),
        Button({ id: 'fps-cap-off', onClick: () => atomic.setFpsCapEnabled(0) }, ['Off']),
        Button({ id: 'fps-cap-on', onClick: () => atomic.setFpsCapEnabled(1) }, ['On']),
        Button({ id: 'fps-minus', onClick: () => atomic.stepFpsCap(-1) }, ['-']),
        El('span', { id: 'fps-value' }, ['-']),
        Button({ id: 'fps-plus', onClick: () => atomic.stepFpsCap(1) }, ['+']),
    ])
);
settings.appendChild(El('div', { id: 'error', className: 'error' }, []));

settings.appendChild(El('div', { className: 'section-title' }, ['GPU adapter']));
settings.appendChild(El('div', { id: 'adapter-list' }, []));
settings.appendChild(Button({ id: 'apply-gpu' }, ['Apply (respawns all panes)']));

settings.appendChild(El('div', { className: 'section-title' }, ['Credentials']));
settings.appendChild(
    Row({}, [
        El('label', {}, ['Use OS keychain']),
        Button({ id: 'keychain-off', onClick: () => atomic.setUseKeychain(0) }, ['Off']),
        Button({ id: 'keychain-on', onClick: () => atomic.setUseKeychain(1) }, ['On']),
    ])
);
settings.appendChild(El('div', { id: 'vault-error', className: 'error' }, []));
settings.appendChild(
    Row({}, [
        El('input', { id: 'field-cred-key', type: 'text', placeholder: 'key' }, []),
        El('input', { id: 'field-cred-value', type: 'text', placeholder: 'value' }, []),
        Button({ id: 'add-credential', onClick: () => atomic.addCredential() }, ['Add']),
    ])
);
settings.appendChild(El('div', { id: 'credential-list' }, []));

settings.appendChild(El('div', { className: 'section-title' }, ['Import from Chrome']));
settings.appendChild(
    Row({}, [
        Button({ id: 'import-history', onClick: () => atomic.importHistory() }, ['Import History']),
        Button({ id: 'import-bookmarks', onClick: () => atomic.importBookmarks() }, ['Import Bookmarks']),
        Button({ id: 'import-cookies', onClick: () => atomic.importCookies() }, ['Import Cookies']),
        Button({ id: 'import-passwords', onClick: () => atomic.importPasswords() }, ['Import Passwords']),
    ])
);
settings.appendChild(El('div', { id: 'import-result' }, []));
settings.appendChild(El('div', { id: 'bookmark-list' }, []));

settings.appendChild(Button({ id: 'close-settings' }, ['Close']));

// Called by `ChromeEngine::sync_settings_state` with the adapter name list
// plus the currently-selected index (`null` = default) — replaces
// `#adapter-list`'s children, folding the old "active-if-none" special
// case for the default button into a plain `active` prop like every other
// button here.
function renderAdapters(adapters, selected) {
    const buttons = [
        Button({ id: 'adapter-default', active: selected === null, onClick: () => atomic.applyGpuAdapter(-1) }, [
            'Default',
        ]),
    ];
    adapters.forEach((name, i) => {
        buttons.push(
            Button({ id: `adapter-${i}`, active: selected === i, onClick: () => atomic.applyGpuAdapter(i) }, [name])
        );
    });
    document.getElementById('adapter-list').replaceChildren(...buttons);
}

// Called with the vault's current credential key list.
function renderCredentials(keys) {
    document.getElementById('credential-list').replaceChildren(
        List({
            items: keys,
            emptyText: 'No stored credentials.',
            renderItem: (key, i) =>
                Row({}, [
                    El('span', {}, [key]),
                    Button({ id: `cred-remove-${i}`, onClick: () => atomic.removeCredential(i) }, ['Remove']),
                ]),
        })
    );
}

// Called with already-formatted bookmark display lines — no empty-state
// message, same as the `bookmark_html` it replaces (which just mapped an
// empty array to an empty string).
function renderBookmarks(lines) {
    document.getElementById('bookmark-list').replaceChildren(...lines.map((line) => Row({}, [line])));
}
