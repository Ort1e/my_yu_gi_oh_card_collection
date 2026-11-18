//! The site installed as an app on a phone (a PWA): `static/app.webmanifest`
//! (linked by `base.html`, outside the public copy), and its service worker
//! `static/js/sw.js`, served here at `/sw.js`: a worker controls only the
//! pages under its own address, so it must be at the root to control them all.
//! Registered by `static/js/app_install.js`. See docs/tour.md, "The app on a
//! phone".

use axum::http::header;
use axum::response::{IntoResponse, Response};

/// The worker's code, embedded: one binary to deploy.
const SERVICE_WORKER: &str = include_str!("../../../../static/js/sw.js");

/// `GET /sw.js` (public, even in private mode: it holds no data).
pub async fn service_worker() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        SERVICE_WORKER,
    )
        .into_response()
}
