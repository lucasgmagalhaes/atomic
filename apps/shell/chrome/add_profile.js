document.getElementById('modal').addEventListener('click', function (e) {
    var id = e.target && e.target.id;
    if (id === 'create') { atomic.createProfileSubmit(); return; }
    if (id === 'cancel') { atomic.cancelAddProfile(); return; }
});
