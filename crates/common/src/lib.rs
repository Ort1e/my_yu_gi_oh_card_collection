//! What every collection uses (Yu-Gi-Oh!, books, Pokémon): the app's
//! [`AppState`], the web plumbing ([`web`]: login, errors, forms,
//! pagination, filters), the database ([`db`]: pool, `Money`, migrations),
//! photos, the base templates (`templates/`: `base.html`, `list_base.html`,
//! the navigation bars), and [`app`], which assembles the collections'
//! [`Routes`] into the server.
//!
//! Each collection crate returns its [`Routes`]; the `backend` binary gives
//! them all to [`app`], and each crate's tests give theirs to
//! `testing::TestApp` (feature `test-support`).

pub mod db;
pub mod http;
pub mod media;
pub mod photos;
pub mod schedule;
pub mod sql;
#[cfg(feature = "test-support")]
pub mod testing;
pub mod web;

use std::path::PathBuf;

use axum::http::{header, HeaderValue};
use axum::middleware::{from_fn_with_state, map_response};
use axum::routing::{get, post};
use axum::Router;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeader;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;
use web::auth;

/// Shared state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: db::Pool,
    pub http: reqwest::Client,
    /// Django's MEDIA_ROOT: card images and shipment files.
    pub media_dir: PathBuf,
    /// Shipment PDFs uploaded but not yet attached to a lot.
    pub uploads_dir: PathBuf,
    /// Our Cardmarket username, to tell purchases from sales in shipments.
    pub cardmarket_account: String,
    pub auth: auth::Auth,
}

/// A collection's routes, by who may use them.
#[derive(Default)]
pub struct Routes {
    /// Pages and JSON that only read: public.
    pub public: Router<AppState>,
    /// Every route that changes data (and the lookups that write a cache):
    /// behind the login.
    pub write: Router<AppState>,
    /// Private reads (the advent calendars): behind the login too.
    pub private: Router<AppState>,
    /// Files served by a handler (card pictures): public, kept out of the
    /// request log (a page loads dozens).
    pub files: Router<AppState>,
    /// Folders of `media/` with personal files, served behind the login at
    /// `/media/<folder>` (shipment PDFs, advent calendar photos).
    pub private_media: Vec<&'static str>,
}

impl Routes {
    /// These routes and `other`'s (a collection's and its advent calendars').
    pub fn merge(self, other: Routes) -> Routes {
        let mut private_media = self.private_media;
        private_media.extend(other.private_media);
        Routes {
            public: self.public.merge(other.public),
            write: self.write.merge(other.write),
            private: self.private.merge(other.private),
            files: self.files.merge(other.files),
            private_media,
        }
    }
}

/// The server: every collection's routes, the login (`/login`, `/logout`
/// and the layer on the write and private routes), the request log, the
/// static files and the media.
pub fn app(state: AppState, collections: Vec<Routes>) -> Router {
    let login_required = from_fn_with_state(state.clone(), auth::require_login);

    let mut public = Router::new();
    let mut write = Router::new();
    let mut private = Router::new();
    let mut files = Router::new();
    // personal files: the shipment PDFs (names, addresses) and the advent
    // calendar photos, cards and books (handwritten notes)
    let mut private_files = Router::new();
    for routes in collections {
        public = public.merge(routes.public);
        write = write.merge(routes.write);
        private = private.merge(routes.private);
        files = files.merge(routes.files);
        for folder in routes.private_media {
            private_files = private_files.nest_service(
                &format!("/media/{folder}"),
                ServeDir::new(state.media_dir.join(folder)),
            );
        }
    }

    // the login on the writes and the private reads, and in private mode
    // (`PUBLIC_READS=0`) on every page and file but the login page and the
    // CSS/JS (axum refuses a layer on a router without routes: a collection
    // may have none of one kind)
    let public_reads = state.auth.public_reads();
    let mut protected = write.merge(private);
    if !public_reads {
        protected = protected.merge(public);
        public = Router::new();
    }
    if protected.has_routes() {
        protected = protected.route_layer(login_required.clone());
    }
    if !public_reads && files.has_routes() {
        files = files.route_layer(login_required.clone());
    }
    let mut media = Router::new().nest_service("/media", ServeDir::new(&state.media_dir));
    if !public_reads {
        media = media.layer(login_required.clone());
    }

    Router::new()
        .merge(public)
        .merge(protected)
        .route("/login", get(auth::page).post(auth::login))
        .route("/logout", post(auth::logout))
        // One INFO line per page or API call: method, path, status and time.
        // A layer only wraps the routes added before it, so the files below
        // (card images, CSS, media) are not logged.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .merge(files)
        .merge(private_files.layer(login_required))
        // CSS and the deck builder bundle: `no-cache` makes browsers check for
        // a newer file on each page load (a 304 when unchanged) instead of
        // reusing a stale copy after an update.
        .nest_service(
            "/static",
            SetResponseHeader::overriding(
                ServeDir::new("static"),
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            ),
        )
        .merge(media)
        .layer(map_response(web::headers::security))
        .with_state(state)
}
