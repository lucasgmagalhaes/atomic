document.getElementById('settings').addEventListener('click', function (e) {
    var id = e.target && e.target.id;
    if (!id) return;

    if (id === 'panes-minus') { atomic.stepMaxPanes(-1); return; }
    if (id === 'panes-plus') { atomic.stepMaxPanes(1); return; }
    if (id === 'fps-minus') { atomic.stepFpsCap(-1); return; }
    if (id === 'fps-plus') { atomic.stepFpsCap(1); return; }
    if (id === 'fps-cap-off') { atomic.setFpsCapEnabled(0); return; }
    if (id === 'fps-cap-on') { atomic.setFpsCapEnabled(1); return; }
    if (id === 'keychain-off') { atomic.setUseKeychain(0); return; }
    if (id === 'keychain-on') { atomic.setUseKeychain(1); return; }
    if (id === 'add-credential') { atomic.addCredential(); return; }
    if (id === 'import-history') { atomic.importHistory(); return; }
    if (id === 'import-bookmarks') { atomic.importBookmarks(); return; }
    if (id === 'import-cookies') { atomic.importCookies(); return; }
    if (id === 'import-passwords') { atomic.importPasswords(); return; }

    if (id === 'adapter-default') { atomic.applyGpuAdapter(-1); return; }
    if (id.indexOf('adapter-') === 0) {
        atomic.applyGpuAdapter(parseInt(id.substring('adapter-'.length), 10));
        return;
    }
    if (id.indexOf('cred-remove-') === 0) {
        atomic.removeCredential(parseInt(id.substring('cred-remove-'.length), 10));
        return;
    }
});
