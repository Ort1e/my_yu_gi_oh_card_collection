// The live tournament pages (/tournaments/live, /tournaments/:id/live) as an
// installable app: their service worker (static/js/live_sw.js, served at
// /tournaments/live-sw.js) keeps them for the times without network.
// Browsers only allow it on HTTPS (or localhost): elsewhere nothing happens.

(function () {
    if (!('serviceWorker' in navigator)) {
        return;
    }

    navigator.serviceWorker.register('/tournaments/live-sw.js', { scope: '/tournaments/' }).catch((error) => {
        console.warn('live notes: no offline copy', error);
    });
})();
