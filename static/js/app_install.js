// The site as an installable app (static/app.webmanifest): its service worker
// (static/js/sw.js, served at /sw.js) keeps the pages and pictures seen, for
// the times without network. Loaded by base.html, never in the public copy.
// Browsers only allow it on HTTPS (or localhost): elsewhere nothing happens.

(function () {
    if (!('serviceWorker' in navigator)) {
        return;
    }

    navigator.serviceWorker.register('/sw.js', { scope: '/' }).catch((error) => {
        console.warn('app: no offline copy', error);
    });
})();
