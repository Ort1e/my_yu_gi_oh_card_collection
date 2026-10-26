//! The advent calendars: the 24 days of a December, each with what was
//! behind its door (a card, a book) and a photo of it. One engine for every
//! collection: the pages ([`pages`]), the day form and its writes
//! ([`form`], [`day_form`]) and the photo files ([`calendar_photos`]).
//! A collection says what is behind a door by implementing [`Calendar`],
//! then adds [`routes`] to its own.
//!
//! Private: the pages are behind the login like the write routes, and the
//! photo folder is served behind it too (`Routes::private_media`): the
//! photos may show handwritten notes.
//!
//! Each collection has its own tables (`<table>`, `<table>_entry` with the
//! item's column), no shared rows.

pub mod calendar_photos;
pub mod day_form;
pub mod form;
pub mod pages;

use std::future::Future;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, SqliteConnection};

use common::db::Pool;
use common::web::error::{AppResult, OrNotFound};
use common::{AppState, Routes};

/// What a collection's calendars are: their place and their texts.
pub struct Spec {
    /// The list's address: `/advent-calendar`.
    pub base: &'static str,
    /// The calendars' table (`advent_calendar`); their days are in
    /// `<table>_entry`.
    pub table: &'static str,
    /// The days' column naming the item: `card_id`.
    pub item_column: &'static str,
    /// Photos live under `<media>/<photo_folder>/<year>/`.
    pub photo_folder: &'static str,
    /// The section of the site: its `body` class and navigation bar (`""`:
    /// the Yu-Gi-Oh! one, `"books"`).
    pub section: &'static str,
    /// "Advent calendar": the titles, followed by the year.
    pub name: &'static str,
    /// What is behind a door: `card` (also the day form's field).
    pub item: &'static str,
    /// Its plural: `cards`.
    pub items: &'static str,
    /// The day field's label: `Card (code or #id)`.
    pub field_label: &'static str,
    /// The day field's placeholder on an empty day: `e.g. MAMO-FR023`.
    pub example: &'static str,
    /// How a door is filled, after "choose its card": `a code like
    /// MAMO-FR023, or #id`.
    pub choose: &'static str,
    /// The add page's help, after "for each one,": `its card (it must be in
    /// the collection)`.
    pub add_help: &'static str,
    /// The edit page's help on the day fields (HTML).
    pub edit_help: &'static str,
    /// What happens to the item when its day is emptied: `the card stays in
    /// the collection`.
    pub stays: &'static str,
}

/// What a day field names.
pub enum Pick<New> {
    /// An item of the collection, by id.
    Existing(i64),
    /// An item to add on the way (a book looked up by its ISBN).
    New(New),
}

/// What the edit page shows besides the days.
#[derive(Default)]
pub struct EditExtras {
    /// Above the days (HTML): the card calendar's lot.
    pub above: String,
    /// Suggested in the day fields: `(id, label)`, given as `#id`.
    pub suggestions: Vec<(i64, String)>,
}

/// A collection's advent calendars.
pub trait Calendar: Send + Sync + 'static {
    const SPEC: &'static Spec;

    /// The days with their items: `e.day`, `e.photo` and the item's columns
    /// (read by [`Calendar::Item`]), from `<table>_entry e`, for the
    /// calendar `?`, by day.
    const ENTRIES: &'static str;

    /// What is behind a door, rendered by its template when the door is
    /// opened (a link to its page).
    type Item: for<'r> FromRow<'r, SqliteRow> + askama::Template + Send + Unpin;

    /// An item a day field adds on the way.
    type New: Send + Sync;

    /// The item's id, its page and its name, and what follows its `#id` on
    /// the edit page (a card's code, a book's authors).
    fn id(item: &Self::Item) -> i64;
    fn href(item: &Self::Item) -> String;
    fn name(item: &Self::Item) -> &str;
    fn detail(item: &Self::Item) -> String;

    /// The item a day field names (`text`: trimmed, not empty). `Err` is
    /// the message to show. Network calls go here: before the write
    /// transaction.
    fn find(
        state: &AppState,
        text: &str,
    ) -> impl Future<Output = AppResult<Result<Pick<Self::New>, String>>> + Send;

    /// Add a new item ([`Pick::New`]) in the day's transaction: its id.
    fn add(
        conn: &mut SqliteConnection,
        new: &Self::New,
    ) -> impl Future<Output = sqlx::Result<i64>> + Send;

    /// The edit page's extras.
    fn edit_extras(
        pool: &Pool,
        calendar_id: i64,
        year: i64,
    ) -> impl Future<Output = AppResult<EditExtras>> + Send;

    /// More buttons in the calendar page's toolbar (HTML).
    fn toolbar(pool: &Pool, calendar_id: i64) -> impl Future<Output = AppResult<String>> + Send {
        let _ = (pool, calendar_id);
        async { Ok(String::new()) }
    }
}

/// The calendars' routes, under [`Spec::base`]: the pages private, the
/// writes behind the login, the photos private.
pub fn routes<C: Calendar>() -> Routes {
    let base = C::SPEC.base;
    Routes {
        public: Router::new(),
        private: Router::new()
            .route(base, get(pages::list::<C>))
            .route(&format!("{base}/:year"), get(pages::calendar::<C>))
            .route(&format!("{base}/:year/edit"), get(form::edit_page::<C>)),
        write: Router::new()
            .route(
                &format!("{base}/add"),
                get(form::add_page::<C>).post(form::create::<C>),
            )
            .route(
                &format!("{base}/:year/days/:day"),
                post(form::save_day::<C>).layer(DefaultBodyLimit::max(40 * 1024 * 1024)),
            )
            .route(
                &format!("{base}/:year/days/:day/rotate"),
                post(form::rotate::<C>),
            )
            .route(
                &format!("{base}/:year/days/:day/delete"),
                post(form::remove_day::<C>),
            ),
        files: Router::new(),
        private_media: vec![C::SPEC.photo_folder],
    }
}

/// The id of the calendar of `year`, a 404 when there is none.
pub async fn calendar_id<C: Calendar>(pool: &Pool, year: i64) -> AppResult<i64> {
    sqlx::query_scalar(&format!("SELECT id FROM {} WHERE year = ?", C::SPEC.table))
        .bind(year)
        .fetch_optional(pool)
        .await?
        .or_404()
}
