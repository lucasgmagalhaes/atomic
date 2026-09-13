// Static add-profile modal built once at load time with the shared `El`/
// `Button`/`Row` kit (`lib/ui.js`) — field values are read directly off the
// DOM by Rust (`ChromeEngine::input_value`), this script only wires the
// two buttons. `create` reuses the shared `.btn.active` look (same blue
// `#create` always had) via the `active` prop instead of a bundle-local
// class.
const modal = document.getElementById('modal');

const field = (id, label) => Row({}, [Label({}, [label]), Input({ id, type: 'text' })]);

modal.appendChild(field('field-name', 'Name'));
modal.appendChild(field('field-start-url', 'Start URL'));
modal.appendChild(field('field-email', 'Email'));
modal.appendChild(field('field-password', 'Password'));
modal.appendChild(field('field-proxy', 'Proxy'));
modal.appendChild(Div({ id: 'error', className: 'error' }, []));
modal.appendChild(
    Div({ id: 'buttons' }, [
        Button({ id: 'create', active: true, onClick: () => atomic.createProfileSubmit() }, ['Create']),
        Button({ id: 'cancel', onClick: () => atomic.cancelAddProfile() }, ['Cancel']),
    ])
);
