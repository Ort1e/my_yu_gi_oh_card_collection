//! The security headers of every answer (pages, JSON, files).
//!
//! The Content-Security-Policy lets the pages load only the app's own files,
//! plus what they use from outside: scripts from jsDelivr (Chart.js on the
//! dashboard and value pages) and the
//! Pokémon finder's thumbnails from TCGdex. Styles allow `'unsafe-inline'`:
//! the deck builder sets one `style` attribute (a chart's height). No
//! inline script anywhere (CLAUDE.md), so none is allowed.

use axum::http::{header, HeaderName, HeaderValue};
use axum::response::Response;

const CSP: &str = "default-src 'self'; \
    script-src 'self' https://cdn.jsdelivr.net; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data: blob: https://assets.tcgdex.net; \
    connect-src 'self'; \
    object-src 'none'; \
    base-uri 'self'; \
    form-action 'self'; \
    frame-ancestors 'none'";

/// Add the headers an answer doesn't set itself.
pub async fn security(mut response: Response) -> Response {
    let headers = response.headers_mut();
    let defaults = [
        (header::CONTENT_SECURITY_POLICY, CSP),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "same-origin"),
        (
            HeaderName::from_static("permissions-policy"),
            "geolocation=(), microphone=(), payment=()",
        ),
    ];
    for (name, value) in defaults {
        headers
            .entry(name)
            .or_insert(HeaderValue::from_static(value));
    }
    response
}
