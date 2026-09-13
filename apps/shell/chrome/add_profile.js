// Static add-profile modal built once at load time with the shared `El`/
// `Button`/`Row`/`mount` kit (`lib/ui.js`) — field values are read directly
// off the DOM by Rust (`ChromeEngine::input_value`), this script only
// wires the two buttons. `create` reuses the shared `.btn.active` look
// (same blue `#create` always had) via the `active` prop instead of a
// bundle-local class.
const field = (id, label) => Row({}, Label({}, label), Input({ id, type: 'text' }));

mount(
    'modal',
    field('field-name', 'Name'),
    field('field-start-url', 'Start URL'),
    field('field-email', 'Email'),
    field('field-password', 'Password'),
    field('field-proxy', 'Proxy'),
    Div({ id: 'error', className: 'error' }),
    Div(
        { id: 'buttons' },
        Button({ id: 'create', active: true, onClick: atomic.createProfileSubmit }, 'Create'),
        Button({ id: 'cancel', onClick: atomic.cancelAddProfile }, 'Cancel')
    )
);
