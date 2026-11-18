/// <reference lib="webworker" />
// The live tournament page's service worker, served at
// /tournaments/live-sw.js (embedded in the binary by routes/tournaments/live.rs)
// so it controls /tournaments/ only. It lets the installed app open with no
// network: the live pages and their files are kept in the cache.
//
//   /tournaments/<id>/live   network first, the cached copy when offline
//   /tournaments/live        offline: the last live page opened
//   FILES (CSS, scripts)     the cached copy at once, refreshed behind
//   /api/…                   never cached (the page keeps its own copy)
//
// Bump CACHE when this file's rules change: the old copies are dropped.
// /logout clears the cache (Clear-Site-Data).

const CACHE = 'live-v1';

/** Where the last live page's address is kept, in the cache. */
const LAST_LIVE = '/tournaments/live-sw/last';

/** What the live pages load, fetched at install. */
const FILES = [
    '/static/css/style.css',
    '/static/css/header.css',
    '/static/css/form.css',
    '/static/css/books.css',
    '/static/css/tournaments.css',
    '/static/js/nav.js',
    '/static/js/back_link.js',
    '/static/js/confirm.js',
    '/static/js/tournament_live.js',
    '/static/js/live_install.js',
    '/static/live.webmanifest',
    '/static/img/live/icon-192.png',
    '/static/img/live/icon-512.png',
];

const sw = /** @type {ServiceWorkerGlobalScope} */ (/** @type {unknown} */ (self));

sw.addEventListener('install', (event) => {
    event.waitUntil(
        (async () => {
            const cache = await caches.open(CACHE);

            // one file missing doesn't stop the others
            await Promise.all(FILES.map((file) => cache.add(file).catch(() => undefined)));
            await sw.skipWaiting();
        })(),
    );
});

sw.addEventListener('activate', (event) => {
    event.waitUntil(
        (async () => {
            for (const name of await caches.keys()) {
                if (name !== CACHE) {
                    await caches.delete(name);
                }
            }

            await sw.clients.claim();
        })(),
    );
});

/** @param {Request} request @returns {Promise<Response>} */
async function livePage(request) {
    const cache = await caches.open(CACHE);

    try {
        const response = await fetch(request);

        // a redirect (to /login) or an error page isn't kept
        if (response.ok && response.type === 'basic') {
            await cache.put(request, response.clone());
            await cache.put(LAST_LIVE, new Response(new URL(request.url).pathname));
        }

        return response;
    } catch {
        const cached = await cache.match(request, { ignoreSearch: true });

        return cached || offline();
    }
}

/** `/tournaments/live`: today's tournament, or the last one opened when offline. */
async function startPage(/** @type {Request} */ request) {
    try {
        return await fetch(request);
    } catch {
        const cache = await caches.open(CACHE);
        const last = await cache.match(LAST_LIVE);

        if (last) {
            return Response.redirect(new URL(await last.text(), request.url).href, 302);
        }

        return offline();
    }
}

/** @param {Request} request @returns {Promise<Response>} */
async function staticFile(request) {
    const cache = await caches.open(CACHE);
    const cached = await cache.match(request);
    const fresh = fetch(request).then(async (response) => {
        if (response.ok) {
            await cache.put(request, response.clone());
        }

        return response;
    });

    if (cached) {
        // refreshed for the next time, behind
        fresh.catch(() => undefined);

        return cached;
    }

    return fresh;
}

/** @returns {Response} */
function offline() {
    const html =
        '<!DOCTYPE html><meta charset="utf-8"><meta name="viewport" content="width=device-width">' +
        '<title>Offline</title><p>No network, and this page was never opened here with it. ' +
        'Open it once with the network: it then works offline.</p>';

    return new Response(html, { status: 503, headers: { 'Content-Type': 'text/html; charset=utf-8' } });
}

sw.addEventListener('fetch', (event) => {
    const request = event.request;
    const url = new URL(request.url);

    if (request.method !== 'GET' || url.origin !== sw.location.origin) {
        return;
    }

    if (request.mode === 'navigate' && /^\/tournaments\/\d+\/live\/?$/.test(url.pathname)) {
        event.respondWith(livePage(request));
    } else if (request.mode === 'navigate' && url.pathname === '/tournaments/live') {
        event.respondWith(startPage(request));
    } else if (FILES.includes(url.pathname)) {
        event.respondWith(staticFile(request));
    }
});
