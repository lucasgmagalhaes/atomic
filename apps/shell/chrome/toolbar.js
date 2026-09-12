// One delegated listener on the stable #toolbar container — #workspaces'
// inner buttons get replaced wholesale by sync_toolbar_state's innerHTML
// writes each frame, so a per-button addEventListener would need
// re-attaching after every rebuild. Delegation avoids that entirely: the
// listener itself never moves, only which child raised the event changes.
document.getElementById('toolbar').addEventListener('click', function (e) {
    var id = e.target && e.target.id;
    if (!id) return;
    if (id === 'add-profile') { atomic.addProfile(); return; }
    if (id === 'count-1') { atomic.setPaneCount(1); return; }
    if (id === 'count-2') { atomic.setPaneCount(2); return; }
    if (id === 'count-4') { atomic.setPaneCount(4); return; }
    if (id === 'count-6') { atomic.setPaneCount(6); return; }
    if (id === 'locale-en') { atomic.setLocale('en'); return; }
    if (id === 'locale-pt') { atomic.setLocale('pt'); return; }
    if (id === 'new-workspace') { atomic.createWorkspace(); return; }
    if (id.indexOf('workspace-') === 0) {
        atomic.switchWorkspace(parseInt(id.substring('workspace-'.length), 10));
        return;
    }
});
