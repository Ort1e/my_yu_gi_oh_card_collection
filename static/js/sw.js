/// <reference lib="webworker" />
// The app's service worker (the site installed as an app: static/app.webmanifest),
// served at /sw.js (embedded in the binary by common::web::app) so it controls
// every page. It lets the installed app open with no network: the pages and
// pictures already seen are kept in the cache, as last seen.
//
//   pages, /static/…         network first; the cached copy when offline or
//                            when the network takes more than WAIT_MS
//   /tournaments/live        offline: the last live tournament page opened
//   card/book pictures       the cached copy, else the network (PICTURES_MAX kept)
//   /api, /login, /media…    never cached (the live page keeps its own queue)
//
// Saving still needs the network, except on the live tournament page (its
// script queues the rounds). Bump VERSION when this file's rules change: the
// old caches are dropped. /logout clears them (Clear-Site-Data).

const VERSION = 'v1';
const PAGES = 'pages-' + VERSION;
const STATIC = 'static-' + VERSION;
const PICTURES = 'pictures-' + VERSION;

/** How long the network has before the cached copy is shown instead. */
const WAIT_MS = 3000;

/** How many pages and pictures are kept: the oldest go first. */
const PAGES_MAX = 300;
const PICTURES_MAX = 1500;

/** Where the last live tournament page's address is kept, in the pages' cache. */
const LAST_LIVE = '/sw/last-live';

/** What every page loads, fetched at install. */
const PRELOAD = [
    '/static/css/style.css',
    '/static/css/header.css',
    '/static/css/form.css',
    '/static/css/books.css',
    '/static/js/nav.js',
    '/static/js/back_link.js',
    '/static/js/confirm.js',
    '/static/js/app_install.js',
    '/static/app.webmanifest',
    '/static/img/app/icon-192.png',
    '/static/img/app/icon-512.png',
];

/** Never cached: answers that change or are personal. */
const NEVER = [/^\/api\//, /^\/books\/api\//, /^\/pokemon\/api\//, /^\/login/, /^\/logout/, /^\/media\//, /^\/sw\.js$/];

/** The pictures: one address per picture, never changed. */
const PICTURE = [/^\/card-image\//, /^\/card-picture\//, /^\/pokemon\/card-image\//, /^\/books\/\d+\/cover$/];

const sw = /** @type {ServiceWorkerGlobalScope} */ (/** @type {unknown} */ (self));

sw.addEventListener('install', (event) => {
    event.waitUntil(
        (async () => {
            const cache = await caches.open(STATIC);

            // one file missing doesn't stop the others
            await Promise.all(PRELOAD.map((file) => cache.add(file).catch(() => undefined)));
            await sw.skipWaiting();
        })(),
    );
});

sw.addEventListener('activate', (event) => {
    event.waitUntil(
        (async () => {
            const current = [PAGES, STATIC, PICTURES];

            for (const name of await caches.keys()) {
                if (!current.includes(name)) {
                    await caches.delete(name);
                }
            }

            await sw.clients.claim();
        })(),
    );
});

/** Keep at most `max` entries in `cache`: the oldest put go. @param {Cache} cache @param {number} max */
async function trim(cache, max) {
    const keys = await cache.keys();

    for (const key of keys.slice(0, Math.max(0, keys.length - max))) {
        await cache.delete(key);
    }
}

/**
 * The network's answer, but the cached copy when the network fails or is
 * slower than WAIT_MS (a venue's bad network); the answer still arriving then
 * refreshes the copy for the next time.
 * @param {Cache} cache
 * @param {Request} request
 * @param {(response: Response) => Promise<void>} keep what to do with a good answer
 * @returns {Promise<Response | undefined>} undefined: no network and no copy
 */
async function networkFirst(cache, request, keep) {
    const network = fetch(request).then(async (response) => {
        // a redirect (to /login) or an error page isn't kept
        if (response.ok && response.type === 'basic') {
            await keep(response.clone());
        }

        return response;
    });
    const cached = await cache.match(request);

    if (!cached) {
        return network.catch(() => undefined);
    }

    /** @type {Promise<undefined>} */
    const late = new Promise((resolve) => setTimeout(() => resolve(undefined), WAIT_MS));

    try {
        const response = await Promise.race([network, late]);

        if (response) {
            return response;
        }
    } catch {
        // no network: the copy
    }

    network.catch(() => undefined);

    return cached;
}

/** @param {Request} request @returns {Promise<Response>} */
async function page(request) {
    const cache = await caches.open(PAGES);
    const path = new URL(request.url).pathname;

    const response = await networkFirst(cache, request, async (fresh) => {
        await cache.delete(request);
        await cache.put(request, fresh);

        if (/^\/tournaments\/\d+\/live\/?$/.test(path)) {
            await cache.put(LAST_LIVE, new Response(path));
        }

        await trim(cache, PAGES_MAX);
    });

    if (response) {
        return response;
    }

    // today's tournament: the last one opened
    const last = path === '/tournaments/live' ? await cache.match(LAST_LIVE) : undefined;

    if (last) {
        return Response.redirect(new URL(await last.text(), request.url).href, 302);
    }

    return offline();
}

/** @param {Request} request @returns {Promise<Response>} */
async function staticFile(request) {
    const cache = await caches.open(STATIC);
    const response = await networkFirst(cache, request, (fresh) => cache.put(request, fresh));

    return response || Response.error();
}

/** @param {Request} request @returns {Promise<Response>} */
async function picture(request) {
    const cache = await caches.open(PICTURES);
    const cached = await cache.match(request);

    if (cached) {
        return cached;
    }

    const response = await fetch(request);

    if (response.ok && response.type === 'basic') {
        await cache.put(request, response.clone());
        await trim(cache, PICTURES_MAX);
    }

    return response;
}

/** @returns {Response} */
function offline() {
    const html =
        '<!DOCTYPE html><meta charset="utf-8"><meta name="viewport" content="width=device-width">' +
        '<title>Offline</title><p>No network, and this page was never opened here with it. ' +
        'Pages opened once with the network can be read offline.</p><p><a href="/">Home</a></p>';

    return new Response(html, { status: 503, headers: { 'Content-Type': 'text/html; charset=utf-8' } });
}

sw.addEventListener('fetch', (event) => {
    const request = event.request;
    const url = new URL(request.url);

    if (request.method !== 'GET' || url.origin !== sw.location.origin) {
        return;
    }

    const path = url.pathname;

    if (NEVER.some((pattern) => pattern.test(path))) {
        return;
    }

    if (request.mode === 'navigate') {
        event.respondWith(page(request));
    } else if (path.startsWith('/static/')) {
        event.respondWith(staticFile(request));
    } else if (PICTURE.some((pattern) => pattern.test(path))) {
        event.respondWith(picture(request));
    }
});
