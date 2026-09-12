document.getElementById('settings').addEventListener('click', function (e) {
    var id = e.target && e.target.id;
    if (!id) return;

    if (id === 'panes-minus') { nimble.stepMaxPanes(-1); return; }
    if (id === 'panes-plus') { nimble.stepMaxPanes(1); return; }
    if (id === 'fps-minus') { nimble.stepFpsCap(-1); return; }
    if (id === 'fps-plus') { nimble.stepFpsCap(1); return; }
    if (id === 'fps-cap-off') { nimble.setFpsCapEnabled(0); return; }
    if (id === 'fps-cap-on') { nimble.setFpsCapEnabled(1); return; }
    if (id === 'keychain-off') { nimble.setUseKeychain(0); return; }
    if (id === 'keychain-on') { nimble.setUseKeychain(1); return; }
    if (id === 'add-credential') { nimble.addCredential(); return; }
    if (id === 'import-history') { nimble.importHistory(); return; }
    if (id === 'import-bookmarks') { nimble.importBookmarks(); return; }
    if (id === 'import-cookies') { nimble.importCookies(); return; }
    if (id === 'import-passwords') { nimble.importPasswords(); return; }

    if (id === 'adapter-default') { nimble.applyGpuAdapter(-1); return; }
    if (id.indexOf('adapter-') === 0) {
        nimble.applyGpuAdapter(parseInt(id.substring('adapter-'.length), 10));
        return;
    }
    if (id.indexOf('cred-remove-') === 0) {
        nimble.removeCredential(parseInt(id.substring('cred-remove-'.length), 10));
        return;
    }
});
