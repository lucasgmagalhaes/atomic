// Shared chrome UI kit — evaluated into every bundle's JS context before its
// own script runs (see `ChromeEngine::new`'s `UI_JS` eval). Plain function
// composition over the engine's real DOM API, not a virtual DOM / JSX
// runtime: every component here returns an actual DOM node built with
// `document.createElement`/`setAttribute`/`addEventListener` — the same
// calls a bundle would make by hand, just organized into reusable
// functions. No template parser, no build step: `Button({...}, [...])` is
// itself valid ES6, nothing needs translating before the engine runs it.

// The primitive every component below is a thin wrapper over: creates
// `tag`, applies `props` (an `on*` function becomes a real listener, not an
// attribute; `className` sets `.className`; anything else becomes an
// attribute unless it's `null`/`false`), and appends `children` (a string
// becomes a text node, anything else is assumed to already be a DOM node).
function El(tag, props, children) {
    const el = document.createElement(tag);
    for (const key in props || {}) {
        const value = props[key];
        if (key.startsWith('on') && typeof value === 'function') {
            el.addEventListener(key.slice(2).toLowerCase(), value);
        } else if (key === 'className') {
            el.className = value;
        } else if (value != null && value !== false) {
            el.setAttribute(key, value === true ? '' : value);
        }
    }
    (children || []).forEach((child) => {
        el.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
    });
    return el;
}

// `props.active` toggles the shared `.btn.active` look (theme.css) — same
// highlight every bundle already used per-button (`ws-active`/`active`),
// now a single class name across all of them.
function Button(props, children) {
    return El(
        'button',
        {
            id: props.id,
            className: props.active ? 'btn active' : 'btn',
            onClick: props.onClick,
        },
        children
    );
}

function Row(props, children) {
    return El('div', { id: props && props.id, className: 'row' }, children);
}

// An on/off button pair sharing one boolean — the `fps-cap-off`/`-on` and
// `keychain-off`/`-on` pattern `settings.js` used to hand-roll twice.
function Toggle(props) {
    return Row({}, [
        Button({ id: props.offId, active: !props.isOn, onClick: props.onOff }, [props.offLabel || 'Off']),
        Button({ id: props.onId, active: props.isOn, onClick: props.onOn }, [props.onLabel || 'On']),
    ]);
}

// A list with a shared empty-state look (`.empty`, theme.css) — the
// adapter/credential/bookmark/downloads/history "either rows or a
// placeholder span" pattern `sync_settings_state`/`sync_downloads_history`
// used to build as escaped HTML strings.
function List(props) {
    if (!props.items.length) {
        return El('span', { className: 'empty' }, [props.emptyText]);
    }
    return El(
        'div',
        {},
        props.items.map((item, i) => props.renderItem(item, i))
    );
}
