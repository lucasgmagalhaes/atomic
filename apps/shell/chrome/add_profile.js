document.getElementById('modal').addEventListener('click', function (e) {
    var id = e.target && e.target.id;
    if (id === 'create') { nimble.createProfileSubmit(); return; }
    if (id === 'cancel') { nimble.cancelAddProfile(); return; }
});
