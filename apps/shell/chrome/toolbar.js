// Static toolbar chrome built once at load time with the shared `El`/
// `Button` kit (`lib/ui.js`) — each button attaches its own real `click`
// listener at creation, so unlike the old delegated-`if/else` version
// there's no dispatch table to keep in sync with the markup. Only
// `#workspaces`' contents change after load (`renderWorkspaces`, called by
// `ChromeEngine::sync_toolbar_state`) — same reason it stays a stable
// `<span>` container here instead of being rebuilt with the rest.
const toolbar = document.getElementById('toolbar');
toolbar.appendChild(El('span', { id: 'workspaces' }, []));
toolbar.appendChild(El('span', { id: 'label' }, ['Panes:']));
toolbar.appendChild(Button({ id: 'count-1', onClick: () => atomic.setPaneCount(1) }, ['1']));
toolbar.appendChild(Button({ id: 'count-2', onClick: () => atomic.setPaneCount(2) }, ['2']));
toolbar.appendChild(Button({ id: 'count-4', onClick: () => atomic.setPaneCount(4) }, ['4']));
toolbar.appendChild(Button({ id: 'count-6', onClick: () => atomic.setPaneCount(6) }, ['6']));
toolbar.appendChild(Button({ id: 'add-profile', onClick: () => atomic.addProfile() }, ['+ Add Profile']));
toolbar.appendChild(Button({ id: 'locale-en', onClick: () => atomic.setLocale('en') }, ['EN']));
toolbar.appendChild(Button({ id: 'locale-pt', onClick: () => atomic.setLocale('pt') }, ['PT']));

// Called by `ChromeEngine::sync_toolbar_state` with `[{name, active}, ...]`
// every time `AtomicApp`'s workspace list changes — replaces `#workspaces`'
// children wholesale, same refresh strategy the old `innerHTML` write used,
// just building real nodes (with real listeners) instead of an HTML string.
function renderWorkspaces(workspaces) {
    const buttons = workspaces.map((ws, i) =>
        Button({ id: `workspace-${i}`, active: ws.active, onClick: () => atomic.switchWorkspace(i) }, [ws.name])
    );
    buttons.push(Button({ id: 'new-workspace', onClick: () => atomic.createWorkspace() }, ['+ New']));
    document.getElementById('workspaces').replaceChildren(...buttons);
}
