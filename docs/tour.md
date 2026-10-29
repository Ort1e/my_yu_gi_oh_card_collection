# A tour of the code

This is the reading guide for the Rust version of the app. It assumes you
know the Django version (and the collection itself) well, and shows where
everything went. For step-by-step changes, see [recipes.md](recipes.md).

Suggested reading order, about an hour with the code open next to it:

1. [The big picture](#the-big-picture) (this file)
2. [Following a page request](#following-a-page-request): `crates/yugioh/src/routes/sellers/mod.rs`
3. [Following a form](#following-a-form): `crates/yugioh/src/routes/sellers/form.rs`, then `lots/form/add.rs`
4. [The database](#the-database): `migrations/0002_ygo_schema.sql`, `crates/common/src/db.rs`
5. [The domain crates](#the-domain-crates): `crates/yugioh-domain/src/lib.rs` and one module, e.g. `ydke.rs`
6. [The deck builder](#the-deck-builder): `frontend/src/App.svelte` and `crates/yugioh/src/routes/decks/api/`
7. [Talking to ygoprodeck](#talking-to-ygoprodeck): `crates/yugioh/src/store/card_data/`
8. [Tests](#tests): `crates/*/src/tests/`
9. [The books](#the-books): `crates/books/`, a second, smaller collection
10. [The Pokémon cards](#the-pokémon-cards): `crates/pokemon/`, a third one, with prices
11. [What the collections share](#what-the-collections-share): `crates/common/`, `crates/advent/`
12. [Where it runs](#where-it-runs): the NAS, the public copy, `deploy/`

## The big picture

```
                    browser
          ┌───────────┴─────────────┐
     HTML pages                 deck builder (Svelte app, static/deck-builder/)
          │                         │  JSON over /api/*
          ▼                         ▼
 ┌────────────── backend (the binary: main.rs, commands) ──────────────┐
 │  common::app(state, [yugioh::routes(), books::routes(),             │
 │                      pokemon::routes()]): routers + login layer     │
 └────┬──────────────────────┬───────────────────────┬─────────────────┘
      ▼                      ▼                       ▼
   yugioh                  books                  pokemon        one crate per collection:
   routes/ store/          routes/ store/         routes/ store/ pages (SQL + handler +
   templates/ commands/    templates/             templates/     template struct), writes,
      │      └──────┬──────────┘                     │           askama templates
      │          advent (the calendars' engine)      │
      └──────────────┴──────────┬────────────────────┘
                                ▼
   common: AppState, web/ (login, errors, forms, pagination, filters, card tiles),
           db (pool, Money), photos, http (throttle), media, schedule, base templates
      │ pure functions                        │ sqlx
      ▼                                       ▼
   yugioh-domain, books-domain,          app.db (SQLite: live on the NAS, copies in the data submodule)
   pokemon-domain, common-domain              ▲
   (no DB, no HTTP)                           │ migrations/ applied at startup
```

The Cargo workspace (`crates/*` and `importer`):

| Crate | What it is | Django equivalent |
|---|---|---|
| `crates/backend/` | The binary: the web server **and** the maintenance commands; wires the collections | the project, `manage.py` |
| `crates/yugioh/` | The Yu-Gi-Oh! cards: pages, writes, ygoprodeck and Cardmarket, commands, background tasks | `views/`, `forms`, `rest_api/`, `management/commands/` |
| `crates/books/` | The book collection | (none) |
| `crates/pokemon/` | The Pokémon cards | (none) |
| `crates/advent/` | The advent calendars, cards and books: one engine | (models only) |
| `crates/common/` | What every collection uses: state, web plumbing, DB, photos, base templates, the test harness | settings, base templates |
| `crates/yugioh-domain/` | Pure Yu-Gi-Oh! business logic, unit-tested | model methods, `monte_carlos/`, `shipment_reader/` |
| `crates/books-domain/`, `crates/pokemon-domain/` | The other collections' pure logic | (none) |
| `crates/common-domain/` | Pure logic shared by both card collections: conditions and their value factors | |
| `importer/` | One-off Django `dumpdata` loader, used for the cutover | `loaddata` |

Dependencies only go down: a collection uses its domain crate, `common`
(and `advent`); `backend` uses them all; nothing uses `backend`. Plus
`frontend/` (the Svelte deck builder, built by npm, not Cargo) and
`migrations/` (SQL files run by both binaries).

Each collection crate has its own `templates/` and an `askama.toml` that
adds `../common/templates` (`base.html`, `list_base.html`, the navigation
bars, the shared partials), so its templates extend those by name.

### What replaced what

| Django | Rust |
|---|---|
| `models/*.py` (schema) | `migrations/0002_ygo_schema.sql`, plain SQL |
| `models/*.py` (methods) | `crates/*-domain/` for pure logic, SQL in `routes/` or `store/` |
| ORM queries | raw SQL strings run by `sqlx` (`query_as`, `query_scalar`) |
| `urls.py` | each collection's `Routes` (`public`, `write`, `private`, `files`): `yugioh/src/routes/mod.rs`, `books/src/lib.rs`, `pokemon/src/lib.rs` |
| class-based views | `async fn` handlers in `routes/<area>/` (`mod.rs` pages, `form.rs` forms) |
| Django templates | askama templates in `crates/<collection>/templates/<area>/`, nearly identical syntax |
| `forms.py` | `common::web::forms` (`FormData`) + validation in each `routes/<area>/form.rs` |
| DRF API + serializers | `routes/decks/api/` + `#[derive(Serialize)]` structs |
| `signals.py` | explicit code in the handler (e.g. `delete_orphan_proxies`) |
| `manage.py <command>` | `cargo run -p backend -- <command>` (`commands/`) |
| admin | edit pages (`/cards/:id/edit`, …) |
| Django auth | `common::web::auth`: one password per user (`APP_PASSWORD`, `APP_PASSWORD_<NAME>`), signed cookie |
| `runserver` | `cargo run -p backend` |

### Startup (`crates/backend/src/main.rs`)

`main()`:

1. reads `.env` (`dotenvy`), sets up the logs (`tracing`, level from
   `RUST_LOG`, `info` by default), parses the command line (`commands::Command`);
2. opens the SQLite pool and runs pending migrations (`common::db::init_pool`);
3. builds `AppState`, the struct every handler receives: the DB pool, an
   HTTP client (for ygoprodeck), the media/uploads directories, your
   Cardmarket account name, and the login settings;
4. either runs a maintenance command, or starts the background tasks and
   serves `app(state)` until Ctrl-C;
5. closes the pool, which folds the SQLite WAL back into the database file.

`app()` gives the three collections' `Routes` to `common::app`, which
builds the whole URL map: the public routes, the write and private routes
behind the login middleware, `/login`, and the static/media file servers
(each collection names its private media folders). Its
`TraceLayer` logs one line per page or API call; it sits before the file
routes (card images, `/static`, `/media`), which are therefore not logged,
because a `.layer()` only wraps the routes added before it.

Changes to the collection are logged by the handlers themselves, after the
commit, with named fields: `tracing::info!(lot_id, cards = n, "lot created")`.

## Following a page request

`GET /sellers?page=2`, in `crates/yugioh/src/routes/sellers/mod.rs`:

```rust
pub async fn list(
    State(state): State<AppState>,      // the shared AppState
    Query(param): Query<PageParam>,     // ?page=2, parsed by serde
) -> AppResult<SellersList> {           // a template struct, or an error
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM seller")
        .fetch_one(&state.pool)
        .await?;                        // `?`: a DB error becomes a 500
    …
    let sellers = sqlx::query_as(…).bind(PAGE_SIZE).bind(page.offset())
        .fetch_all(pool).await?;        // rows → Vec<SellerRow> via #[derive(FromRow)]
    Ok(SellersList { page_title: "All Sellers", page, total, sellers })
}
```

Things to notice:

- **Extractors**: a handler's arguments say what it needs from the
  request: `State`, `Path(id)` for `/sellers/:id`, `Query`, `Form`, `Json`,
  `Multipart`. axum fills them in, and rejects the request with a 4xx if
  one can't be parsed (e.g. `/sellers/abc`).
- **The response type is the template**. `SellersList` has
  `#[derive(Template)] #[template(path = "sellers/list.html")]`; returning
  it renders the HTML. The template's variables are the struct's fields, and
  it can call the struct's methods.
- **Templates are compiled.** askama turns each template into Rust code at
  build time, so a typo in a variable name is a compile error, not a blank
  on the page. The price: `cargo build` after each template edit (there is
  no hot reload of templates).
- **Row structs** (`SellerRow`) are filled by name from the SQL columns
  (`#[derive(sqlx::FromRow)]`). Rows used by several pages live in
  `store/models.rs` (`CardRow`, `LotRow`, `DeckVersionRow`), with their `SELECT`
  as a constant next to them.
- **Errors** (`common::web::error`): handlers return `AppResult<T>`. `?` turns a
  `sqlx::Error` into a 500 (logged), `.or_404()` turns a missing row
  (`Option::None`) into a 404, `bad_request("…")` gives a 400 with a JSON
  `{"error": …}` for the API.
- **Pagination** (`common::web::pagination`): `Page::new(requested, count, querystring)`
  and `page.offset()`; `partials/pagination.html` renders the links. Filter pages
  (`/cards`, `/lots`) keep their filters in the page links with
  `querystring(…)`.
- **Template inheritance** works like Django's: `base.html` → `list_base.html`
  (blocks `items`, `filter`, `add_button`, and `summary`, `more_css`,
  `more_js`) → `sellers/list.html`. Partials live in `partials/` and are
  pulled in with `{% include %}`. Filters such as `|money` or `|yes_no` are
  plain functions in `common::web::filters`. The shared templates are in
  `crates/common/templates/`.
- **Inside the yugioh crate**, `crate::web::…` and `crate::store::db` are
  re-exports of `common`'s modules (`yugioh/src/web/mod.rs`): the code reads
  as before the split.

## Following a form

`POST /sellers/:id/edit`, in `crates/yugioh/src/routes/sellers/form.rs`:

1. The body arrives as raw `(name, value)` pairs (`Form<Pairs>`) and is
   wrapped in `FormData`, which collects validation errors as it reads:
   `form.required("name", "Name")`, `form.id(…)`, `form.decimal(…)`,
   `form.date(…)` each return `Option<value>` and push a message to
   `form.errors` when invalid.
2. Extra checks (e.g. "name already taken") push to `form.errors` too.
3. If anything is wrong, the same page struct is rebuilt with the values
   **as typed** (`form.text(…)`) and the errors, and rendered again. That's
   Django's "form invalid → re-render" pattern.
4. Otherwise: `begin_write(&pool)` opens a write transaction, the
   `UPDATE`/`INSERT`s run on it, `tx.commit()`, then `Redirect::to(…)`.

`lots/form/add.rs::create` is the same pattern at full size:

- a lot plus repeated card rows (`cards-0-name`, `cards-0-code`, …, read by
  `form.card_rows()` / `form.new_cards()`, the `CardRows` trait of
  `yugioh/src/web/card_rows.rs`);
- an existing seller **or** a new one created on the fly (`SellerChoice`);
- `cards::insert_lot_cards` creates each card `quantity` times with its
  `unite`;
- an optional shipment PDF (see below) moved into
  `media/shipment_files/` right before the commit, and moved back if the
  commit fails;
- after the commit, `spawn_link_cards` starts a **background task** that
  links the new cards to their ygoprodeck `card_data` (fetching unknown
  names from the API), puts their names in the language of their code, then
  swaps deck proxies for the new owned copies. The redirect doesn't wait
  for it (see [Talking to ygoprodeck](#talking-to-ygoprodeck)).

### The shipment PDF flow

`POST /lots/add/shipment` (`lots/form/add.rs::upload`): the PDF is parsed **in
memory** (`yugioh_domain::cardmarket::parse_shipment_pdf`), and the form is
rendered pre-filled with the dates, total, seller and card lines. The file
waits in `uploads/pending-<token>.pdf` (deleted after 24 h), and the form
carries the token in a hidden `pending` field. On save, `create` moves it to
`media/shipment_files/shipment_<lot id>.pdf`. The file moves are in
`crates/yugioh/src/store/shipment_files.rs`.

The PDFs contain names and addresses: their text is never stored, logged or
printed, only the parsed fields. Keep it that way (CLAUDE.md, "Privacy
rule").

## The database

**Schema.** `migrations/0002_ygo_schema.sql` is the Django schema written
by hand in SQL, with Django's constraints kept (UNIQUE, CHECK on choice
fields, ON DELETE). The ids are the Django ids. Tables you'll meet most:

```
card_data ◄── card ──► unite ──► lot ──► seller ──► seller_source
 (ygoprodeck)  │                (purchase/sale)
               │
               ├── deck_version_{main,extra,side}_deck ──► deck_version ──► deck
               └── card_category_assignment ──► card_category ──► deck_version
monthly_budget ◄── apport           advanced_ban_list ◄── ban_list_entry ──► card_data
```

Differences from the Django models to keep in mind:

- **Money is TEXT** (`"12.50"`), read and written as `db::Money` (a wrapper
  around `rust_decimal::Decimal`). Never sum money in SQL (SQLite would
  convert it to floats): load the rows and add in Rust.
- Dates are `YYYY-MM-DD` text, booleans are `0/1`.
- `CardConditionalCategory` is folded into `card_category`: a category is
  conditional when `condition_description IS NOT NULL`.
- Django's many-to-many fields are explicit join tables (`cards::Zone::table()`
  gives the one for a deck zone).

**Migrations.** `sqlx::migrate!` embeds `migrations/*.sql` into the binary
and applies the new ones at startup, recording a checksum of each. So an
applied migration must **never be edited** (startup would fail on the
checksum): changes are new files, numbered after the last one (`0017_….sql` next).

**Writing safely** (`common/src/db.rs`): SQLite allows one writer at a time. Every
transaction that writes starts with `db::begin_write(&pool)` (not
`pool.begin()`), which takes the write lock up front so concurrent writers
wait instead of failing with "database is locked". Get new ids with
`.execute(…)?.last_insert_rowid()`. Do ygoprodeck calls *before* opening a
write transaction, because they write `card_data` through the pool themselves.

**Where the data lives.** The live database is on the NAS
(`/Data1To/collection/app.db`). The private `data` submodule keeps copies
(`data/app.db`, the media), brought from the NAS's nightly backups; on the
computer the server runs on such a copy. Ctrl-C (or SIGTERM) stops the
server cleanly, and after that `app.db` alone holds everything. See
[recipes.md](recipes.md#back-the-data-up-to-git).

## The domain crates

`crates/yugioh-domain/` holds the logic worth testing on its own, with no
database and no HTTP. Each function gets plain values and returns plain
values (`books-domain`: `book`, `isbn`; `pokemon-domain`: see [the Pokémon
cards](#the-pokémon-cards); `common-domain`: `CardStatus` and the condition
factors, which `yugioh-domain` re-exports as `card::CardStatus` and
`valuation::condition_factor`):

| Module | What |
|---|---|
| `card` | name normalisation (`normalize_card_name`, `clean_card_name`), `CardStatus` (from `common-domain`) |
| `lot` | `LotType`, shipping cost |
| `budget` | monthly recap (`recap`, `BudgetTotals`, `available_debt`) for `/budget` and the dashboard |
| `ydke` | `YdkeDeck::parse` / `to_url` |
| `monte_carlo` | opening-hand simulation, conditional categories included |
| `cardmarket` | shipment text → `Shipment` (dates, total, seller, card lines) |
| `ban_list` | Konami's ban-list JSON and US pages → entries |
| `cardmarket_catalog` | Cardmarket's price guide files, set code → expansion, copy → product |
| `valuation` | market value of the copies (condition/language factors), ownership by date, gain from lot prices, price moves |

Each module's doc comment (`//!` at the top) says what it ports and lists
the **deliberate differences from Django** (bugs fixed, crashes turned into
errors). That is the place to look when the Rust version behaves
differently from what you remember. Unit tests sit at the bottom of each
file (`#[cfg(test)] mod tests`).

## The deck builder

A Svelte 4 app in `frontend/`, built by Vite into `static/deck-builder/`
(`bundle.js`, `bundle.css`). The page `/deck-builder/:id`
(`templates/decks/builder.html`) is just a `<div id="app">` that loads the
bundle. The app then only talks JSON to `/api/*`:

| Frontend | Backend |
|---|---|
| `src/main.js`: mounts `App` with the version id | |
| `src/App.svelte`: state of the zones, drag & drop, save | `decks/api/version.rs`: load, save, delete, copy |
| `src/api.js`: one function per endpoint | `crates/yugioh/src/routes/decks/api/`: one handler per endpoint, one file per group (`ApiCard` in `mod.rs`) |
| `src/deck.js`: pure helpers (sorting, prices, YDKE URL) | |
| `src/lib/*.svelte`: zone, card tile, card overview, reserve search, YDKE toolbar, version panel, simulation, tournament; each with its CSS | `reserve.rs`, `proxies.rs`, `categories.rs`, `simulation.rs`, `tournament.rs`, `ydke_import.rs` |

Moving cards between zones only changes the app's local state until
**Save** (`PATCH /api/deck-versions/:id` with the three zones). Proxies,
categories and the tournament are saved immediately. Errors come back as
`{"error": "…"}` with a 4xx status, and `api.js` turns them into exceptions
the app shows to you.

During frontend work, `npm run dev` serves the app with hot reload on
`:5173` and forwards `/api` to the backend on `:3000`.

## Built decks

A deck version can be **built** (`deck_version.is_built`, migration 0010):
the physical deck exists, so its real (non-proxy) cards are **locked**, out
of the collection for every other version. In this app a proxy already means
"a card I don't have for this deck", so nothing else is needed to see what is
missing.

- **Building** (`PATCH /api/deck-versions/:id` with `{"is_built": true}`,
  `decks/api/version.rs`): `yugioh_domain::built_deck::plan_build` says which cards
  change; each of the version's cards shared with other versions becomes a
  proxy there (`store::cards::proxy_copy` + `swap_card`, categories moved),
  except the cards another built version already holds: first built, first
  served, those become proxies here. The answer carries a `build_report`
  the deck builder shows.
- **Saving a built version's zones** runs the same building step again: a
  card added from the reserve that other versions use becomes a proxy in
  them, and the save answer carries a `build_report` too. So a real card of
  a built version is never in another version.
- **Unbuilding** only clears the flag. The proxies made in other versions
  stay; `clean-cards` puts the owned cards back (`replace_proxies`), in
  those versions and in the unbuilt one.
- **Locked cards** (`store::cards::locked_elsewhere(version_id)`, a subquery)
  are skipped by the reserve, the YDKE import and `clean-cards`; saving zones
  with one is refused with a 400 naming the card and the built deck (only a
  stale page can send one). A copy of a built version gets proxies for them.
- **A built version's proxies are swapped by hand only.** The automatic
  swap (`replace_proxies`, after a new lot and in `clean-cards`) skips built
  versions: the physical deck changes only when you decide. In the deck
  builder, a selected proxy has a "Use my copy" button (`POST
  /api/deck-versions/:id/proxies/:card_id/replace`, `decks/api/proxies.rs`):
  it takes a `store::cards::free_copy` (a copy in no deck first), and if that
  copy was in other versions, they get a proxy of it, as building does.
- Each version can note its **deck box** and **sleeves** (free text).
- `/decks/built` (`routes/decks/built.rs`): each built version, what it still
  misses (its proxies), and the shopping list of all of them.

## Tournaments

`/tournaments` (`routes/tournaments/`, `templates/tournaments/`,
`static/css/tournaments.css`): every tournament with its record and
placement, then the stats, both following the filter (`?since=`, `?deck=`).
A tournament has numbered **rounds** (`tournament_round`: opponent's deck as
written, its labels, die, result, team result, comment), each with any number
of **games** (`tournament_game`: won, lost, draw (replayed, so more than 3
games) or unfinished (cut by time), first/second, comment). The version
played is still `deck_version.tournament_id`, one version at most; a copy of
a version (`clone`) doesn't take it.

- **Labels** (`deck_label`): archetype/engine names, one table for your decks
  (`deck_deck_label`, on the deck edit page) and the opponents' rounds
  (`tournament_round_label`). Typed comma-separated with suggestions
  (`yugioh_domain::tournament::split_labels`), matched case-insensitively, created on
  the way, deleted when unused; `/tournaments/labels` renames and merges.
- **Writes** (`store/tournaments.rs`): `insert`/`update` replace the rounds
  (`set_results`), shared by the form and `import-tournament-notes`.
- **The form** (`routes/tournaments/form.rs`, `static/js/tournament_form.js`):
  rows `rounds-<i>-…` and `rounds-<i>-games-<j>-…` (`FormData::indices`),
  blank rows skipped, numbered in order. The edit page shows the old free
  `notes` beside the form until "Clear the old notes".
- **Stats** (`yugioh_domain::tournament::stats`): rounds (forfeit = win,
  double loss = loss, byes apart), games (draws and unfinished out of the
  rate), by version, by your deck's labels, matchups, die, first/second.
  `routes/tournaments/data.rs` turns the rows into `RoundRecord`s.
- **Deck builder**: the tournament panel is a summary with links to these
  pages (`decks/api/tournament.rs`).

## The advent calendar

`/advent-calendar/:year` (`static/js/advent_calendar.js`,
`static/css/advent.css`): 24 doors that open on the photo and the card of
the day. The tables `advent_calendar` (one per year) and
`advent_calendar_entry` (day, card, photo) come from Django, which had no
page for them. The photos are in `media/advent_calendar_photos/<year>/`
(upright, 1600 px, metadata stripped; the full-size originals in
`originals/`). They show handwritten notes, so the pages and the photos are
behind the login.

The card and book calendars are one engine, the `advent` crate:

| Piece | Where |
|---|---|
| what a collection's calendars are: tables, texts, what is behind a door, how a day field finds it | the `advent::Calendar` trait (`crates/advent/src/lib.rs`) and its `Spec` |
| the list and a calendar's page | `advent/src/pages.rs`, `advent/templates/advent/{list,calendar}.html` |
| create a calendar, fill a day (multipart), turn a photo, empty a day | `advent/src/form.rs` (+ `day_form.rs`), `templates/advent/{add,edit}.html` |
| the photo files: written, turned, deleted | `advent/src/calendar_photos.rs` (resizing: `common::photos`) |
| the cards: a card by code or `#id`, the door's picture and text, the calendar's lot | `yugioh/src/routes/advent/mod.rs` (`Cards`), `yugioh/templates/advent/{door,lot}.html` |
| the books: a book by ISBN (looked up and added) or `#id` | `books/src/routes/advent/mod.rs` (`Books`), `books/templates/books/advent/door.html` |

`advent::routes::<Cards>()` gives the routes under the calendar's base
(`/advent-calendar`, `/books/advent-calendar`), its pages private and its
photo folder too; each collection merges them into its own `Routes`. A day
is filled on the edit page with an item (`#id`, a code, an ISBN) and a
photo; `common::photos` makes the copy shown on the page (upright from the
EXIF orientation, 1600 px, re-encoded without metadata) and the original is
kept in `originals/`; ↺ ↻ turn a photo that came out sideways. Each save
writes new file names, so browsers never show an old photo, and deletes the
previous files.

A card calendar can have its own lot (`advent_calendar.lot_id`, migration
0006, "Create its lot" on the edit page, `POST /advent-calendar/:year/lot`,
in the yugioh crate): its cards are added on the usual lot page, then
suggested in the day fields; the lot page links back to the calendar.

## The books

A second collection in the same app and database, with nothing in common
with the cards: the novels at home, in `crates/books/`. Every route starts
with `/books` (`src/routes/`, `templates/books/`); the book templates swap the
navigation bar (`{% block nav %}` → `partials/books_nav.html`) and the page
background (`body.books`, `static/css/books.css`). No prices, lots or
pictures.

| Piece | Where |
|---|---|
| tables (`book`, `book_author`, `book_book_author`, `book_series`, `book_data`, `book_advent_calendar[_entry]`) | `migrations/0007_books.sql` |
| ISBN parsing, check digits, ISBN-10 → 13 | `crates/books-domain/src/isbn.rs` |
| lookup answers → `BookInfo`, series text → name + volume, author sort names, missing volumes | `crates/books-domain/src/book.rs` |
| writes: a book with its authors (in order) and series, created on the way; orphans deleted | `src/store/books.rs` |
| the lookup: Open Library (`jscmd=data` + `details`), Google Books when it lacks something; cached in `book_data` | `src/store/book_data.rs` |
| the routes | `src/lib.rs` (`routes()`) |
| list (filters, sort by title / surname / added), detail | `src/routes/mod.rs` |
| add, edit, delete (refused while behind an advent door) | `src/routes/form.rs` |
| `GET /books/api/isbn/:isbn`, the lookup as JSON | `src/routes/api.rs` |
| authors, series (with the missing volumes) | `src/routes/authors.rs`, `series.rs` |
| the book advent calendars (the `advent` crate, own tables and photo folder) | `src/routes/advent/mod.rs` |

The add form (`static/js/book_form.js`): "Look up" (or Enter, which USB
barcode scanners send) calls the API and fills the fields; "Scan" opens the
phone's camera through a file input and reads the barcode **in the
browser**: `BarcodeDetector` where the browser has it (HTTPS only), else the
ZXing library loaded from jsDelivr, trying the photo as taken and turned a
quarter turn. Plain HTTP on the home network can't use the live camera
(browsers keep it to HTTPS), hence the photo.

Like ygoprodeck: the lookup happens **before** any write transaction, and
tests never call it (they insert `book_data` rows).

## The Pokémon cards

A third collection with its own tables (migration 0012), nothing shared
with the Yu-Gi-Oh! ones: the owner's Pokémon cards, mostly from boosters
opened at home, in French, English and Japanese, collected and not played
(no decks), in `crates/pokemon/`. Every route starts with `/pokemon`; the templates swap the
navigation bar (`partials/pokemon_nav.html`) and the colours (`body.pokemon`,
`static/css/pokemon.css`: pale yellow, red header). Reads are public, writes
and the TCGdex lookups behind the login. Copies are **never deleted** (sold
instead, like the Yu-Gi-Oh! cards); wishes can be.

| Piece | Where |
|---|---|
| tables: `pokemon_card` (TCGdex's cards, owned or wished only), `pokemon_copy`, `pokemon_wish`, `pokemon_price`, `pokemon_set` (set cache) | `migrations/0012_pokemon.sql` |
| TCGdex JSON → `CardInfo`, `Variant`, `Source`, `Catalogue`, full art from the rarity | `crates/pokemon-domain/src/lib.rs` |
| the printed code (`FNE 067/064`), card numbers that match (`67` = `067`) | `crates/pokemon-domain/src/card_ref.rs` |
| a booster's numbers (`67 45r 12x2`) | `crates/pokemon-domain/src/pulls.rs` |
| a variant's price from Cardmarket's numbers, a copy's value | `crates/pokemon-domain/src/price.rs` |
| a wish's price against its maximum, a copy that fulfils it | `crates/pokemon-domain/src/wish.rs` |
| the TCGdex client (throttled, every call goes through it) | `src/store/tcgdex.rs` |
| cards cached in `pokemon_card` (`cards::ensure`), copy and wish writes, orphan cards | `src/store/cards.rs` |
| sets by printed code, with their card lists | `src/store/sets.rs` |
| prices: latest, history, the weekly refresh | `src/store/prices.rs` |
| the routes | `src/lib.rs` (`routes()`) |
| the collection (the shared picture grid or a table, filters, value), a copy | `src/routes/mod.rs` |
| add one copy, edit (and sell) | `src/routes/form.rs` |
| a booster opening: numbers → preview → save | `src/routes/booster.rs` |
| the wishlist | `src/routes/wishlist.rs` |
| the card finder's JSON (`/pokemon/api/search`, `/pokemon/api/card`) | `src/routes/api.rs` |
| pictures, downloaded once into `media/pokemon_card_images/` (`common::media`) | `src/routes/images.rs` |
| the rarity and variant guide | `src/routes/rarities.rs`, `static/js/pokemon_rarities.js` (drawn by `rarity_guide.js`) |

**TCGdex** ([tcgdex.dev](https://tcgdex.dev), free, no key) has two
catalogues: the international cards (one id, `sv06.5-067`, for the French,
English, German… prints) and the Japanese ones (their own sets and ids,
`SV6a-010`, Japanese names only). A copy's language picks the catalogue
(`ja` → Japanese). An international card is asked in English (its category,
types and rarity are the stored labels; French answers translate them) and
in French (name, set name, picture); a Japanese card in Japanese, then its
species' French and English names by its Pokédex number. The Pokémon TCG API
(pokemontcg.io) is only asked for the pictures TCGdex lacks (below).

| What | Call | Remembered in |
|---|---|---|
| a card | `GET /v2/{en,fr,ja}/cards/{id}` | `pokemon_card`, asked again after 30 days |
| a Japanese card's species names | `GET /v2/{fr,en}/cards?dexId=eq:{n}` | `pokemon_card` |
| a set by printed code (`FNE`, `SFA`) and its card list | `GET /v2/{lang}/sets?abbreviation.localized=` (then `.official`, then the id), `GET /v2/{lang}/sets/{id}` | `pokemon_set`, 7 days |
| a name search | `GET /v2/{lang}/cards?name=` (Pocket cards left out) | not stored |
| a picture | `{image}/high.webp` | `media/pokemon_card_images/` |
| prices, weekly | the card again | `pokemon_price` |

Its filters match "contains, ignoring the case" (`eq:` is exact but
case-sensitive), so the code keeps the exact matches; a card is only found
through its set's list (`sv06.5-67` is a 404, `067` is the number). Several
sets can share a printed code: 30th Celebration (`30th`) and its Classic
Collection (`30th-c`) are both `30C`, with numbers 001 to 030 in both; the
set size typed after the number (`001/030`, `1/30` in the booster form)
picks one, else both are offered (`sets::cards`). A Classic Collection
reprint is printed with the number of the card it reprints (Pikachu & Zekrom
GX: 33/181), which TCGdex doesn't keep: a set size that fits none of the
sets, or a number in none, offers the reprints to pick from
(`sets::subsets`), and `30C Zekrom` (a code then a name) searches the sets'
names (`&`, `et`, `and` alike: `pokemon_domain::fold_name`). A code also
covers its family: TCGdex keeps a Trainer Gallery apart (`swsh10tg`,
code `ASR:TG`) while its cards print `ASR TG29/TG30` (`sets::in_family`);
a lettered number alone (`TG29`) lists every card with it
(`tcgdex::by_number`). A Sword & Shield card prints no code but a
regulation mark (`F 034/189`): the sets of that size
(`sets::by_size`, TCGdex `cardCount.official=eq:189`), the card with that
number and mark. TCGdex's French "localized" code of some sets is their
name (`151` for 151, printed MEW): a code without a letter is ignored. A number missing from sets remembered on an earlier
day asks TCGdex again (`sets::are_old`, `sets::forget`). TCGdex has no
pictures of the Trainer Galleries either: they come from the Pokémon TCG
API (below).

**Booster openings** (migration 0013, `src/store/openings.rs`,
`src/routes/openings.rs`): the booster form saves a `pokemon_opening`
with its copies (`pokemon_copy.opening_id`); `/pokemon/openings` lists them
with their value and best card, an opening's page shows its cards and adds
more (the booster form with `?opening=<id>`, or the one-card form, whose
"From a booster opening" field also moves a copy). The migration grouped
the booster copies saved before by day and language. Its edit page
changes its date and language, and its cards' with them (French ⇔
English; not Japanese, another catalogue): the set code and names are
looked up again by its cards' set ids. A new opening starts in the last
one's language.

**Pictures TCGdex lacks** (migrations 0014, 0015): TCGdex has no picture
of some prints (the Trainer Galleries, some promos, the Classic Collection
reprints). The picture route then asks the [Pokémon TCG
API](https://pokemontcg.io) for the same print, once (`src/store/ptcgio.rs`,
`cards::find_image`): its set by printed code and size (`30C`, 30 cards:
`me55c`), name or id written its way (`sv06.5` → `sv6pt5`); the card by
number, else by name and artist (it numbers a reprint like the card it
reprints: 33). Saved in `image` (a full URL) with `image_from =
'pokemontcg:<id>'`; `''` = it has none (asked again at the monthly card
refresh). That service is being replaced by a paid one and its API answers 500 or
502 to about half the requests, while its image server is reliable: the
picture is first taken straight from the image server when the set id and
number are TCGdex's (`swsh10tg/TG29_hires.png`); the API is asked only
otherwise (a reprint, another set id), each request tried up to 6 times,
its set list kept a day. A failure is a warning ("no picture for now"), not
saved: the card is tried again an hour later. Never another print's picture (an
earlier version borrowed one by name: often another artwork; 0015 cleared
them).

**Prices** (`pokemon_domain::price`): TCGdex gives Cardmarket's numbers per
variant (`variants_detailed[].pricing.cardmarket`), else once per card.
Cardmarket sells a card's normal and reverse holo prints as one product:
the plain fields are the normal's, the `-holo` ones the **reverse holo**'s;
a holo-only card has its price in the plain fields. A price is the trend,
else the 30-day average. A copy's value: a graded copy's estimate (typed by
hand), else its variant's price × the condition factor of the Yu-Gi-Oh!
cards; no language factor. Boosters cost nothing in the app: there is no
gain/loss, only the market value (and what a single cost, when noted).

**Choosing** (the owner can't always tell a rarity at a glance): the card
finder (`partials/pokemon_finder.html`, `static/js/pokemon_finder.js`)
shows the picked card's rarity with its symbol and the guide's link, a
"chase card" hint when its number is above the set size, and one button per
printed variant with its price and how to recognise it; the booster preview
has the same per row (`static/js/pokemon_booster.js`). The guide
(`/pokemon/rarities`) is data in `static/js/pokemon_rarities.js`, like the
Yu-Gi-Oh! one, drawn by the same `static/js/rarity_guide.js`.

## Login

`common::web::auth`: with a password set, every collection's `write` and
`private` routes and the private media folders (shipment PDFs, calendar
photos) require a session cookie. Pages redirect to `/login?next=…`, the
APIs (`/api/*`, `/books/api/*`, `/pokemon/api/*`) answer 401. Without any
password there is no login, and the server refuses to listen anywhere but
a loopback address (`127.0.0.1`).

- **Users**: `APP_PASSWORD` is user `owner`, `APP_PASSWORD_<NAME>` adds
  user `<name>` (a friend). The form asks only for the password, which says
  who logs in (so two users can't share one); the logs name the user of
  each request.
- **What each may change** (`auth::Changes`): the owner everything; another
  user the books only, unless `APP_ACCESS_<NAME>` says `all` or lists the
  collections (`books,pokemon`). Everyone logged in reads everything. Each
  collection's `Routes` names itself (`collection: "books"`), and
  `common::app` puts `auth::require_access` on its `write` routes, inside
  the login: a refusal is a 403 page, or JSON for the APIs. `/login` says
  who is logged in and what they may change.
- **The cookie** is `<user>.<expiry>.<signature>`, signed with the user's
  current password and a key kept in the `app_secret` table: a restart
  doesn't log anyone out, changing a password logs that user out.
  `HttpOnly`, `SameSite=Strict` (no form posted from another site), and
  `Secure` with `SECURE_COOKIES=1` (behind HTTPS).
- **Wrong passwords**: after 5 from one client (its address, or the one a
  proxy on the same machine forwards), 15 minutes of refusal, the right
  password included.
- **Private mode**: `PUBLIC_READS=0` puts every page and picture behind the
  login, reads included (the deployed app); only `/login` and `/static`
  stay open.
- **Headers** (`common::web::headers`): a Content-Security-Policy (our
  files, jsDelivr's scripts, TCGdex's thumbnails), `nosniff`,
  `X-Frame-Options: DENY`, `Referrer-Policy` on every answer.
- Uploaded photos are read with limits (`common::photos`: 10,000 px a
  side, 300 MB), the shipment PDFs with `lopdf` ≥ 0.42 (deep nesting
  refused), and JSON put in a page goes through `filters::json_for_script`.
  The tests: `backend/src/tests/security.rs`, `yugioh/src/tests/security.rs`.

## Maintenance commands

`crates/backend/src/commands/` parses and dispatches them; the Yu-Gi-Oh!
ones live in `crates/yugioh/src/commands/`: `scrap-ban-list` (Konami ban lists → `advanced_ban_list`:
the EU site's JSON, then the US site's pages for the dates still missing, a
fallback for when the EU JSON lags behind; the US index only links the current
list, so `--date 2026-10-19` fetches one not linked yet, and a US page whose table
is still the previous list's is skipped)
and `clean-cards` (normalise names, relink card data, swap proxies for owned
copies, delete unused proxies, rarity from `(V.x - …)` names, names in the
language of their code, rarity from the card data when the code was printed in
one rarity only). Both accept `--dry-run`: they run in a transaction
that is rolled back.

`import-tournament-notes <file.json>` (`yugioh/src/commands/tournament_notes.rs`,
one-off) loads the tournaments' old notes, transcribed into a JSON file of
the private data submodule, as rounds and games; a tournament that has rounds
is skipped, so it is safe to run twice.

`refresh-prices` fetches today's market price of every card data
(`yugioh/src/store/prices/ygoprodeck.rs`, batches of 100 ids) into `card_price`, then
Cardmarket's price guide (`store/prices/cardmarket.rs`, links and prices), then the
Pokémon cards owned or wished from TCGdex (`pokemon/src/store/prices.rs`); the server
does it weekly by itself (`backend/src/commands/prices.rs::spawn_weekly`,
`common::schedule::when_due`: checked every 6 hours, runs when the last
Yu-Gi-Oh! refresh is 7 days old; the Pokémon refresh follows it, since it
keeps no trace of a run that found no price).

`clean-pictures` (`backend/src/commands/pictures.rs`) deletes the cached
card pictures no card uses any more: in `media/card_images/` those of card
data no card links to (and old Django duplicates), in
`media/pokemon_card_images/` those of Pokémon cards no longer owned or
wished. Each collection lists its files in use (`yugioh::routes::used_card_pictures`,
`pokemon::routes::images::used_files`); a picture deleted by mistake is just
downloaded again. The server runs it daily, 15 minutes after it starts.

`export-public` (`backend/src/commands/export.rs`, `common::export`) writes
the **public copy**: static pages to put on the OVH hosting, where nothing
runs. They are **the app's own pages**, saved as files:

1. the exporter builds the app in-process (no login, no network) and asks
   it for its pages like a browser, inside `common::export::public_copy()`
   (a tokio task-local, true only there);
2. it starts from `START` (`/cards`, `/collection/stats`, `/decks`,
   `/rarities`, `/pokemon`, the wishlist and guide, `/books`, authors,
   series) and follows the links, **only** to the addresses `public()`
   accepts: those, a card's, a deck's, a deck version's, a copy's, a
   book's, an author's or a series' page (by id), and the picture routes.
   `/lots`, `/sellers`, `/tournaments`, the deck builder… are never asked
   for, whatever links to them. A page can also refuse: a deck not on the
   public site answers 404 in that mode (only the versions played in a
   tournament or shown by hand, the button on a version's page, and their
   decks), so it isn't in the copy;
3. each page is saved under a flat name (`/cards?page=2` →
   `cards-page-2.html`, `/cards/12` → `cards-12.html`; only the `page` and
   `view` parameters are kept: filters aren't in the copy), its links
   rewritten to these files (`#` for anything else), the pictures as
   WebP at most 480 px wide (`img/card-image-12.webp`: about a third
   smaller than JPEG for the same look; the OVH plan has 100 MB), the
   `/static` files it uses copied. The first start page is also
   `index.html`.

In that mode the templates leave out what is private:
`{% if !common::export::public_copy() %}` around the private menus
(`partials/*_nav.html`), the add buttons and filters (`list_base.html`), a
card's lot, price paid and decks, the Pokémon copies' price paid, notes,
certificate and opening, the wishlist's maximum prices, verdicts and
notes, a book's ISBN and notes, a deck's buttons, tournaments and deck
builder. The card list shows the cards owned only
(no proxy, not sold). A page added to the copy is a path in `START` or
`public()`, and a look at its template for anything private.

The folder is emptied at each export, and refused if it isn't an earlier
export. `backend/src/tests/export.rs` runs it on a database full of
private things and checks that every page is there, every link stays in
the copy, and no seller, price paid, note, ISBN, deck, tournament or budget
gets in. The NAS writes it and uploads it every morning ([Where it
runs](#where-it-runs)).

`backup --to DIR [--keep DAYS]` (`backend/src/commands/backup.rs`) copies
the data while the server runs: `DIR/app-YYYY-MM-DD.db` by SQLite's
`VACUUM INTO` (consistent even with writes going on), the media folder
mirrored, the copies older than 14 days deleted. `fetch-release --to DIR`
(`release.rs`, on the NAS only) downloads the CI's latest build of `main`
when it is new.

You rarely need `clean-cards` by hand: the server runs it in the background
5 minutes after it starts, then once a day (`clean_cards::spawn_daily`, `common::schedule::every`, one
summary line in the log), and the actions that make a cleanup necessary do
their part themselves (saving deck zones deletes unused proxies, saving new
cards links, translates and swaps proxies).

## Talking to ygoprodeck

All the calls to the [ygoprodeck API](https://ygoprodeck.com/api-guide/)
are in `crates/yugioh/src/store/card_data/` (the client in `mod.rs`, then
`translations.rs`, `sets.rs`, `linking.rs`) and `store/prices/`. It allows about 20 requests per
second and blocks the IP for an hour above that: `YGOPRODECK.wait()` (a
`common::http::Throttle`) keeps every call of the server 60 ms apart, and
what can be remembered is.

| What | Call | Remembered in |
|---|---|---|
| a card's data, by English name or ygopro id | `cardinfo.php?name=` / `?id=` | `card_data` (with the whole JSON in `json_data`) |
| its picture | the URL in `json_data` | `media/card_images/` (`/card-image/:id`, `common::media::fetch_once`) |
| a set, for the quick add (`GET /api/sets/:code`) | `cardsets.php`, then `cardinfo.php?cardset=` in English and French for each set with that code | memory (`SET_CACHE`, until a restart); its French names also go to `card_translation` |
| a name in another language | `cardinfo.php?id=&language=fr` | `card_translation` (migration 0005): a name found is kept, "no translation yet" is asked again after 7 days |
| a set code's card names, for its Cardmarket expansion (`store/prices/cardmarket.rs`) | `cardsets.php`, then `cardinfo.php?cardset=` per set with that code | `cardmarket_expansion` (once; not found: again after 30 days) |
| market prices, weekly (`store/prices/ygoprodeck.rs`) | `cardinfo.php?id=1,2,…` (100 ids a request; unknown ids are left out, all unknown = 400) | `card_price` (migration 0008), one row per card and refresh date, never pruned |

Two rules: call ygoprodeck **before** opening a write transaction (the calls
write through the pool), and only through these functions. Tests never call
it: `TestApp` pre-inserts the card data they need (and translations, in
`card_translation`). The shapes ygoprodeck answers are reshaped in pure
functions in `yugioh-domain` (`card_set`, `rarity::Rarity::from_label` for its
rarity names), which are unit-tested on small synthetic JSON.

## The collection value

`/collection/value` (`routes/collection/mod.rs`), a tile on the dashboard,
and a "Market" line on card pages. Two price sources, both refreshed weekly
by `refresh-prices`:

- **Cardmarket's price guide** (`store/prices/cardmarket.rs`,
  `yugioh_domain::cardmarket_catalog`): two public JSON files (products, prices; ~30
  MB) downloaded into memory, a price **per printing**. Only what the
  collection needs is kept (migration 0009): the products named like our
  card data, each set code's expansion, the price history of the linked
  products. The files are never saved. They have no set code and no rarity,
  so each copy is linked to its product by a guess: the set code's
  expansion is the one holding most of the set's card names (ygoprodeck),
  twins (OCG version, reprint) told apart by the date they were added; then
  the product with the card's name there, by rarity rank against price rank
  when there are several. About 80% of the copies get one; the rest, or a
  wrong guess, are set on the card's edit page ("Cardmarket product",
  `card.cardmarket_product_manual = 1`, kept by the refreshes). Its price
  (the trend, else the 30-day average) covers every language and edition.
- **ygoprodeck** (`card_price`): one price per card name, the cheapest
  printing's, an estimate usually too low for old or high rarity copies.
  Used for the copies without a product. The `card_prices` copy inside
  `card_data.json_data` is never used: it dates from when the card was
  linked.

Cardmarket's API takes no new users and scraping its site is against its
terms; the price guide files are its open download.

`yugioh_domain::valuation` does the sums:
- a copy's price: its product's (`Prices::of`), else its card name's;
- a copy's value: price × condition (MT/NM/unknown 1, EX 0.8, G 0.6, P 0.4)
  × language (non-English 0.9), totals rounded to the cent;
- a copy counts on date D when its purchase lot (not cancelled) is dated on
  or before D and it has no sale lot by D; proxies never count;
- the **gain/loss uses the lot prices** (`lot.price`, shipping included,
  less its `refund`: what the seller gave back, e.g. Dateacard's shipping,
  migration 0016; the budget counts the same cost in the lot's month):
  value of the copies owned + sale lots − purchase lots, up to D. Unite
  prices can't do it: many lots were bought as a whole, without a price per
  card;
- beside it, the copies that do have a unite price are compared card by
  card (value against that price, no shipping share);
- each purchase lot against its **full price** (`lot_gain`): the value of
  its cards still owned, plus what its sold cards brought (their price in
  the sale lot), minus `lot.price` (non-card items included); the lots
  without cards (sleeves, boxes) are left out;
- price moves: since the previous refresh and over 3 months, cards owned
  now, moves under 0.50 € left out (one line per price, the same source on
  both dates).

The lists show estimates too: each card on `/cards` (its picture tile, and
the table view) and on a lot's page
(`CardRow::estimate`, from the `valuation::MARKET_PRICE` SQL expression:
the card's latest price, its product's else its name's), and each purchase
lot the value of its cards still owned (`fill_lot_estimates`).

The page loads every row (`store::valuation::owned_copies`, `lots`,
`prices_by_date`) and computes in Rust: money is never summed in SQL. The
chart is `static/js/collection_value.js`; the card page's price line is an
SVG drawn by the server (`cards::sparkline`).

## What the collections share

Besides the plumbing (`common::web`, `common::db`), these were written once
for every collection:

| What | Where | Used by |
|---|---|---|
| the picture grid of cards: `CardTile` (the whole picture in its collection's proportions, `CardShape`; the name on two lines, the other name, the print, the details; at the bottom the condition badge, tags and value; faded when sold), every tile of a page the same size with its lines in the same places; and `View`, grid or `?view=table` (24 or 100 a page, the other view's link) | `common::web::tiles`, `common/templates/partials/card_grid.html`, `static/css/tiles.css` | `/cards`, `/pokemon`, a booster opening |
| the list page: title with counts, a `summary` line, items, pagination, filter popup | `common/templates/list_base.html` | every list |
| the advent calendars | the `advent` crate | cards, books |
| the rarity guide's boxes, and the one asked for | `static/js/rarity_guide.js` (`data-guide`) | `/rarities`, `/pokemon/rarities` |
| one request at a time per outside service, spaced | `common::http::Throttle` | ygoprodeck, TCGdex, the Pokémon TCG API, the book catalogues |
| "asked again after N days" | `common::db::is_fresh` | translations, Cardmarket expansions, ISBN lookups, Pokémon cards and sets |
| a picture downloaded once into `media/`, and the ones no card uses deleted (`clean-pictures`, daily) | `common::media::fetch_once`, `picture`, `remove_unused` | `/card-image/:id`, `/pokemon/card-image/…` |
| a card's page: its picture beside its box | `.card-detail` (card.css) | `/cards/:id`, `/pokemon/:id` |
| background tasks | `common::schedule` (`every`, `when_due`, `is_due`) | the daily clean-up, the weekly prices |
| card conditions and their value factors | `common-domain` | Yu-Gi-Oh! and Pokémon values |
| photos made web-ready (upright, 1600 px, no metadata) | `common::photos` | the calendars |

## Where it runs

```
 computers, phones (home Wi-Fi)                  anyone
        │ http://192.168.1.202:3000              │ https://<domain>/collection/
        ▼                                        ▼
 ┌──────────────── NAS ─────────────────┐   OVH free hosting
 │ /apps/collection       the app        │   (static files only:
 │ /Data1To/collection    live data      │ ─► the cards, wishlist and books,
 │ /Data500/…-backups     nightly copies  │    small pictures, ~20 MB)
 │ nightly: backup, export + upload,     │
 │          update of the app            │
 └───────────────────────────────────────┘
        ▲ the computer: pulls the latest copy into the data submodule
          (deploy/pc/pull-backup.sh)
```

- **The app** runs on the NAS (a ReadyNAS on Debian 8) as a systemd
  service, for the home network only, every page behind the login
  (`PUBLIC_READS=0`).
- **Its build** comes from the CI: the job `nas-build` makes one static
  binary (musl, for any x86-64 processor) with the deck builder and the
  stylesheets, on pull requests into `main` and pushes to it;
  `nas-release` publishes it as the release `nas-latest` once every check
  is green. The NAS downloads it every morning (`fetch-release`), backs up,
  replaces and restarts, and goes back to the previous version if the new
  one doesn't answer.
- **The public copy** (`export-public`) is uploaded over SFTP to the OVH
  hosting, changed files only.
- The scripts and the step-by-step guide: [deploy/README.md](../deploy/README.md).

## Tests

- The domain crates: plain unit tests on values.
- Each collection crate's `src/tests/` (one file per area, helpers in
  `mod.rs`): `common::testing::TestApp` (feature `test-support`) creates a
  migrated SQLite DB in a temporary directory and sends real HTTP requests
  to the collection's routes, behind the real login layer. The yugioh one
  adds two fake `card_data` rows (`yugioh::testing`), so nothing calls
  ygoprodeck. Each test reads like a scenario: POST a form, check the
  redirect, check the rows with `app.scalar("SELECT …")`.
- `crates/backend/src/tests/`: the whole app (login, pages of every
  collection).
- `crates/yugioh-domain/tests/real_shipments.rs` (ignored by default):
  checks the PDF parser against your real shipments and their lots in
  `data/app.db` (the copy pulled from the NAS).

`cargo test --workspace` runs everything but the last one. Tests never
touch `data/app.db`, apart from that ignored test, which opens it
read-only.

`cargo cov` (an alias in `.cargo/config.toml`, for `cargo-llvm-cov`) runs
the same tests and writes a coverage report to
`target/llvm-cov/html/index.html`: open it to see, line by line, the code
no test runs.
