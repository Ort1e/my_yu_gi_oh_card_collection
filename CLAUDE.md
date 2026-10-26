# CLAUDE.md

Personal Yu-Gi-Oh! card collection manager: cards owned, purchase/sale lots, sellers,
monthly budget, decks (versions, categories, YDKE import/export, draw simulation), ban lists.
Plus a separate **book collection** (the owner's wife's novels: ISBN lookup and barcode scan,
authors, series, book advent calendars) under `/books`, and a **Pokémon card collection** (mostly
booster pulls, French/English/Japanese, wishlist, Cardmarket prices from TCGdex) under `/pokemon`.

## History — read first

- `main` is the **Rust app** (the one in use). Its live data is `data/app.db` (data submodule).
- It replaced a **Django** app (cutover 2026-10-13). The Django code is only in history, at tag
  `django-final`: `git show django-final:my_ygo_cards/models/deck.py`,
  `git ls-tree -r --name-only django-final`, or `git archive django-final my_ygo_cards | tar -x -C <scratch dir>`.
  Its `deck_builder_frontend/` is an abandoned React stub.
- Human-oriented docs: `docs/tour.md` (architecture), `docs/recipes.md` (how to change things). Keep them in sync.
- `docs/specs/`: specs of work **not done yet** (handed to the owner or to Claude). Delete a spec once it's implemented, after moving what's still useful into the code's doc comments or the docs (git history keeps it).

## Division of work — read first

The port is finished; the project now serves as Rust practice for the owner, who wants to write the
**interesting functions and logic**, not the plumbing (owner's decision, 2026-10-19).
- **The owner implements the interesting logic**: algorithms and business rules, wherever they live
  (mostly `crates/yugioh-domain/`, also the core of a `store/` function or a command step), e.g.
  `single_rarity`, the valuation and its ownership-by-date, the built-deck locking rules, the
  Monte Carlo, a parser. Claude explains, reviews and proposes (signature, where it goes, similar
  code), and leaves a stub: signature, doc comment, `todo!()`, plus the unit tests it should pass.
- **Claude implements everything else**, backend included: migrations, SQL queries and `store/`
  writes, routes and handlers, forms, JSON APIs, commands' wiring and reports, background tasks,
  handler tests (`TestApp`), templates, CSS, the deck builder (`frontend/`), page scripts
  (`static/js/`), docs.
- When a feature is planned (spec), mark which functions are the owner's. When unsure whether a
  piece is "interesting logic" or plumbing, ask.
- **The books part** (`crates/books/`, `crates/books-domain/`, the book tables): Claude implements
  all of it, logic included (owner's decision, 2026-10-18).
- **The Pokémon part** (`crates/pokemon/`, `crates/pokemon-domain/`, the `pokemon_*` tables): Claude
  implements all of it, logic included, like the books (owner's decision, 2026-10-23).
- **The shared crates** (`common`, `common-domain`, `advent`) are plumbing (Claude), except a
  Yu-Gi-Oh! rule moved there (`common-domain`'s condition factors stay the owner's).

## Stack

| Layer | Tech |
|---|---|
| HTTP | axum 0.7, tower-http (ServeDir, trace) |
| DB | SQLite (WAL) via sqlx 0.7 (runtime queries, `sqlx::migrate!`) |
| Pages | askama 0.12 + askama_axum templates, scripts in `static/js/` (no inline JS), Chart.js from a CDN on the dashboard |
| Deck builder | Svelte 4 + Vite 5 app in `frontend/` (Chart.js), built to `static/deck-builder/` (gitignored), talks only to `/api/*` |
| Env | Nix flake (`flake.nix`, `.envrc` → `use flake`), Rust 1.88.0 pinned in `rust-toolchain.toml` |

Cargo workspace (`Cargo.toml`: `members = ["crates/*", "importer"]`), shared deps under `[workspace.dependencies]`. One crate per collection, a pure domain crate each, shared crates below them; dependencies only go down (domain ← collection ← `backend`). The refactor into crates (2026-10-26) is described in `docs/tour.md`, "The big picture" and "What the collections share".
- `crates/backend/` (package `backend`) — the binary: `main.rs` (env, `AppState` from `common`, `app()` = `common::app(state, vec![yugioh::routes(), books::routes(), pokemon::routes()])`, graceful shutdown), `commands/` (`Command` parsing and dispatch; `prices.rs`: `refresh-prices` = Yu-Gi-Oh! then Pokémon, and the weekly task; `pictures.rs`: `clean-pictures` and its daily task), `tests/` (`auth.rs`, `pages.rs`: the whole app).
- `crates/common/` — what every collection uses: `lib.rs` (`AppState`: pool, reqwest client, media/uploads dirs, Cardmarket account, `Auth`; `Routes { public, write, private, files, private_media }` + `merge`; `app()`: the login layer on `write` + `private`, the trace layer, `/static`, `/media`, the private media folders behind the login), `web/` (`auth.rs` single-password login: `require_login`, `/login`, `/logout`, HMAC-signed cookie, key in `app_secret`; `error.rs` `AppError`/`AppResult`, `.or_404()`, `bad_request`, `required`/`non_blank`; `forms.rs` `FormData`; `pagination.rs` (`Page::new`/`sized`, `querystring`); `filters.rs` askama filters; `tiles.rs` the card grid's `CardTile` (same size and lines for every tile; `CardShape` = each collection's proportions) and `View` grid/table), `db.rs` (pool + migrations, `begin_write`, `exists`/`row_exists`, `Money`, `today`, `is_fresh`), `photos.rs` (EXIF orientation, 1600 px, JPEG without metadata, quarter turns; `image` crate), `http.rs` (`Throttle`), `media.rs` (`fetch_once`, `picture`, `remove_unused`), `schedule.rs` (`every`, `when_due`, `is_due`), `sql.rs` (`like_pattern`), `testing.rs` (feature `test-support`: `TestApp` built from a collection's router, `read`), `templates/` (`base.html`, `list_base.html` (blocks `items`/`filter`/`add_button`/`summary`/`more_css`/`more_js`), `login.html`, `partials/`: `{yugioh,books,pokemon}_nav.html`, `card_grid.html`, `form_errors.html`, `pagination.html`).
- `crates/advent/` — the advent calendars, cards and books: `Calendar` trait + `Spec` (tables, texts, what is behind a door, how a day field finds it), `routes::<C>()`, `pages.rs` (list, calendar), `form.rs` (add, day save, rotate, remove), `day_form.rs`, `calendar_photos.rs` (day photo files: write, originals, rotate, delete), `templates/advent/`.
- `crates/yugioh/` — the Yu-Gi-Oh! cards. `lib.rs` (`routes()`, `app()` for its tests), `web/` (re-exports of `common::web` + `card_rows.rs`: card rows `cards-<i>-<field>`, `CardRows` trait, `NewCards`), `store/` (`db` re-export, `models.rs` (row types shared by pages; `CardRow::tile`), `cards.rs` (card-row writes and shared SQL: `Zone` (+ `card_ids`/`insert`), `SOLD_EXISTS`, `IN_ANY_DECK`, `IN_VERSION`, `insert_lot_cards`, `new_proxy`, `delete_orphan_proxies`, built decks: `locked_elsewhere`, `proxy_copy`, `swap_card`), `card_data/` (ygoprodeck: `mod.rs` the client with its `YGOPRODECK` throttle and card data lookups, `translations.rs` names in other languages remembered in `card_translation` + `plan_translations`, `sets.rs` set lookup + `SET_CACHE`, `linking.rs` the background linking of new cards), `shipment_files.rs` (pending upload → lot shipment file moves), `prices/` (market prices, weekly: `ygoprodeck.rs` into `card_price`, `cardmarket.rs` Cardmarket's price guide downloaded in memory with only the needed rows kept; `history.rs` both read back by date), `valuation.rs` (owned copies, lots and list estimates for `yugioh_domain::valuation`), `tournaments.rs`), `routes/` (one folder per area: `budget/`, `cards/` (+ `images.rs`), `sellers/`, `lots/`, `decks/` (+ `api/`, the deck builder JSON API: one file per route group, shared `ApiCard` in `api/mod.rs`), `reference/` (rarity guide, set lookup), `advent/` (the card calendars' `Cards: advent::Calendar` + the calendar's lot), `collection/` (`/collection/value`), `tournaments/`, `dashboard.rs`; read-only routes in `routes::router()`, **every write route in `routes::write_router()`**), `commands/` (`ban_list`, `clean_cards` (+ the daily task), `prices` (the Yu-Gi-Oh! part of `refresh-prices`, `due`), `tournament_notes`), `testing.rs` (fixtures for other crates' tests: `insert_card_data`, `LOT`, `deck_fixture`), `tests/` (one file per area: `pages`, `lots`, `decks`, `budget`, `deck_api`, `edit_forms`, `advent`, `collection`, `cardmarket`, `built_decks`, `tournaments`, `rarity`…), `templates/` (same folders as `routes/`, `partials/`).
- `crates/books/` — the book collection: `lib.rs` (`routes()`), `routes/` (`mod.rs` list/detail, `form.rs`, `authors.rs`, `series.rs`, `api.rs` ISBN lookup JSON, `advent/` the book calendars' `Books: advent::Calendar`), `store/` (`books.rs` book-row writes: authors and series created on the way, orphans deleted; `book_data.rs` ISBN lookup: Open Library then Google Books, cached in `book_data`), `templates/books/`, `tests/` (`books`, `book_advent`).
- `crates/pokemon/` — the Pokémon collection: `lib.rs` (`routes()`), `routes/` (`mod.rs` list (the shared grid or a table) and copy page, `form.rs` add one/edit/sell, `booster.rs` a booster opening, `openings.rs`, `wishlist.rs`, `api.rs` the card finder's JSON, `images.rs`, `rarities.rs`), `store/` (`tcgdex.rs` the throttled client, `cards.rs` cards cached in `pokemon_card` (`ensure`) + copy and wish writes + orphan cards, `sets.rs` sets by printed code in `pokemon_set`, `openings.rs`, `prices.rs` latest/history/weekly refresh, `ptcgio.rs` the Pokémon TCG API for missing pictures), `templates/`, `tests/pokemon.rs`.
- `crates/yugioh-domain/` — pure, unit-tested business logic (no DB/HTTP): `card` (name normalisation, statuses), `card_set` (codes vs ygoprodeck's `card_sets`: quick-add set cards, single rarity), `rarity` (rarity list, Cardmarket codes, `(V.x - …)` name suffix), `ydke`, `lot`, `budget`, `built_deck` (building a deck version: which cards become proxies where), `valuation` (collection value: factors, ownership by date, gain from lot prices), `cardmarket_catalog` (Cardmarket price guide: set code → expansion, copy → product), `tournament` (round/game results, labels, stats), `monte_carlo`, `cardmarket` (shipment PDF parsing via lopdf), `ban_list` (Konami EU JSON, US HTML pages); `tests/real_shipments.rs`. Each module's doc comment lists its deliberate differences from Django.
- `crates/books-domain/` (`book`, `isbn`), `crates/pokemon-domain/` (TCGdex JSON, `card_ref` printed codes, `pulls` booster numbers, `price` variant prices, `wish`), `crates/common-domain/` (`CardStatus`, the condition factors; re-exported by `yugioh-domain`).
- Each collection crate (and `advent`) has an `askama.toml`: `dirs = ["templates", "../common/templates"]`.
- **Don't pile files in one folder**: new code and templates go in the folder of their area (or a new one) in the crate of their collection; what two collections share goes in `common` (or a shared crate), never copied.
- `importer/` — one-off loader from a Django `dumpdata` JSON into the sqlx schema (used once for the cutover; refuses a non-empty DB).
- `migrations/` — sqlx migrations, one folder for every collection, embedded by both binaries (`sqlx::migrate!("../../migrations")` in `common::db`).
- `frontend/` — the deck builder Svelte app (`App.svelte`, `lib/*.svelte` with their own `<style>`, `api.js` = HTTP client, `deck.js` = pure helpers).
- `data/` — git **submodule** (`Ort1e/my_yu_gi_oh_card_data`, private): **`app.db` (the live database, tracked)**, `media/card_images/`, `media/shipment_files/`, and the Django leftovers `data.json` (dump `app.db` was imported from) and `db.sqlite3` (Django DB). Its `.gitignore` excludes `*.db-wal`/`-shm`/`-journal`.

## Commands

All inside the Nix shell (`nix develop`, or automatic with direnv). Run from the **repo root** (relative paths: `static/`, `data/app.db`, `data/media`).

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo cov                                     # test coverage → target/llvm-cov/html/index.html (alias in .cargo/config.toml; cargo llvm-cov --workspace --summary-only for a table)
cargo test -p yugioh-domain --test real_shipments -- --ignored --nocapture   # parser vs the real PDFs (prints counts/lot ids only)
cargo test -p yugioh                          # one crate's tests (also: books, pokemon, advent, common, backend, *-domain)

cargo run -p backend                          # web server, http://127.0.0.1:3000; runs migrations on startup
cargo run -p backend -- scrap-ban-list        # import new/current Konami ban lists, EU JSON then US pages for missing dates (--all, --dry-run, --date YYYY-MM-DD for a US list not linked yet)
cargo run -p backend -- clean-cards           # normalise names, relink card data, replace/delete proxies, rarity from (V.x - …) names, names in their code's language, rarity of single-rarity codes from card data (--dry-run); the server also runs it in the background (5 min after start, then daily)
cargo run -p backend -- import-tournament-notes <file.json>   # one-off: the tournaments' old notes, transcribed, as rounds and games (--dry-run)
cargo run -p backend -- refresh-prices        # today's market prices: ygoprodeck into card_price, then Cardmarket's price guide (links, prices), then the Pokémon cards from TCGdex (--dry-run); the server does it weekly by itself
cargo run -p backend -- export-public         # the public copy (static pages: Yu-Gi-Oh! and Pokémon cards, Pokémon wishlist, books, market values; nothing else) into public-export/ (--out DIR), for the OVH hosting: common::export + each crate's public section
cargo run -p backend -- clean-pictures        # delete the cached card pictures (media/card_images, media/pokemon_card_images) no card uses any more (--dry-run); the server does it daily

DATABASE_URL=sqlite:/tmp/test.db cargo run -p importer -- data/data.json   # import the Django dump into a scratch DB (never over data/app.db)

cd frontend && npm install && npm run build   # → static/deck-builder/, needed by /deck-builder/:id
cd frontend && npm run dev                    # hot reload at http://localhost:5173/?id=<deck version id>, proxies to :3000
```

`.env` sets `DATABASE_URL=sqlite:data/app.db` (the default too). Both binaries create the DB file if missing.
`data/app.db` is **real data tracked in git**: don't delete/rebuild it, and don't run tests or experiments against it (use a scratch DB). The server closes the pool on Ctrl-C/SIGTERM (graceful shutdown), which checkpoints the WAL into `app.db`; commit it in the submodule only with the server stopped, then bump the submodule pointer here.
Backend env vars (defaults): `BIND_ADDR` (`127.0.0.1:3000`), `APP_PASSWORD` (user `owner`) and `APP_PASSWORD_<NAME>` (more users; none = login disabled, then only a loopback `BIND_ADDR` is accepted), `PUBLIC_READS` (`1`; `0` = every page behind the login), `SECURE_COOKIES` (`0`; `1` behind HTTPS), `MEDIA_DIR` (`data/media`), `UPLOADS_DIR` (`uploads`, gitignored), `CARDMARKET_ACCOUNT` (`0rtie`). The work goes on `dev` (CI: `.github/workflows/ci.yml`), `main` = what runs.

## Routes

- Pages: `/` dashboard (+ collection value tile), `/tournaments` (list + stats, `?since=`/`?deck=`), `/tournaments/:id`, `/tournaments/add[?version=]`, `/tournaments/:id/edit`, `POST /tournaments/:id/delete`, `/tournaments/labels` (rename, merge), `/collection/value` (market value, gain from lot prices, moves, by deck), `/budget` (+ `/budget/manage`), `/cards` (filters, pictures or `?view=table`), `/cards/:id`, `/sellers`, `/sellers/:id`, `/lots` (filters), `/lots/:id`, `/lots/add`, `/decks`, `/decks/:id`, `/decks/add`, `/decks/built` (built versions, missing cards, shopping list), `/deck-builder/:id`, `/rarities` (rarity guide), `/advent-calendar` (list, login required), `/advent-calendar/add`, `/advent-calendar/:year`, `…/:year/edit` (fill the days: card by code or `#id`, photo upload, ↺ ↻).
- Books (every route starts with `/books`; templates override `{% block nav %}` with `partials/books_nav.html` and set `{% block body_class %}books`): `/books` (filters title/author/series, sort), `/books/:id`, `/books/add` (ISBN lookup + barcode scan, `static/js/book_form.js`), `/books/:id/edit`, `POST /books/:id/delete` (refused while behind an advent door), `/books/authors[/:id]`, `/books/series[/:id]` (missing volumes), `GET /books/api/isbn/:isbn` (lookup JSON, login), `/books/advent-calendar[/add|/:year|/:year/edit]` (private), days by ISBN (looked up and added) or `#id`. Book reads are public like the cards.
- Pokémon (every route starts with `/pokemon`; templates override `{% block nav %}` with `partials/pokemon_nav.html` and set `{% block body_class %}pokemon`): `/pokemon` (pictures or `?view=table`, filters, value), `/pokemon/:id`, `/pokemon/add` (one card: the card finder `partials/pokemon_finder.html` + `static/js/pokemon_finder.js`), `/pokemon/add/booster[?opening=]` (numbers → `step=preview` → `step=save`: a new opening, or more cards to one), `/pokemon/openings[/:id[/edit]]`, `/pokemon/:id/edit` (also "sold"; **no delete**), `/pokemon/wishlist[/add|/:id/edit]`, `POST /pokemon/wishlist/:id/{delete,got}`, `/pokemon/rarities` (guide: `static/js/pokemon_rarities.js`), `GET /pokemon/api/{search,card}` (login, 401 JSON), `/pokemon/card-image/:card_id/:lang`. Reads public.
- Card lists: one picture grid for both card collections (`common::web::tiles::CardTile`, `partials/card_grid.html`, `static/css/tiles.css`), a table with `?view=table` (`View`). Rarity guides: data per collection (`static/js/rarities.js`, `pokemon_rarities.js`), one renderer (`static/js/rarity_guide.js`, `data-guide`).
- Tables of data: one convention for all (`static/css/style.css`, "Data tables": `table.numbers` in `.table-scroll`, unit in the heading, `.text` columns, `sortable` + `static/js/tables.js` for sorting and search). The lot page groups the same copies on one row (`routes/lots/mod.rs`, `CopyGroup`).
- Phones: every page is responsive (`@media (max-width: 760px)` at the end of each stylesheet; header not fixed there). The deck builder is desktop-only. Phone access: docs/recipes.md, "Use the app from a phone".
- Edition: `/{cards,sellers,lots,decks}/:id/edit` (GET form, POST save); `POST /decks/:id/delete` (versions cascade, orphan proxies removed), `POST /decks/:id/versions` (new empty version → deck builder).
- **Cards, unites, lots and sellers are never deleted** (they are the collection's history): no delete routes for them. Deck versions are edited/deleted only in the deck builder.
- `/card-image/:card_data_id` serves `media/card_images/<name>.jpg`, downloading it once from the URL in `card_data.json_data` if missing. `/static`, `/media` are plain file servers, except `/media/shipment_files`, `/media/advent_calendar_photos` and `/media/book_advent_calendar_photos` (login required: each crate's `Routes::private_media`, served by `common::app`).
- Login (`common::web::auth`, docs/tour.md "Login"): users from `APP_PASSWORD[_<NAME>]`, 5 wrong passwords → 15 min refusal, security headers on every answer (`web::headers`); with a password set, every collection's `write` and `private` routes and the private media folders (everything with `PUBLIC_READS=0`) need the session cookie: pages redirect to `/login?next=…`, `/api/*`, `/books/api/*` and `/pokemon/api/*` answer 401 `{"error": …}`. Reads stay public. New write routes must go in `write_router()` (Yu-Gi-Oh!) or `write` of the crate's `routes()`.
- Lot creation: `POST /lots/add/shipment` parses a Cardmarket PDF and re-renders the form pre-filled; the PDF waits as `uploads/pending-<token>.pdf` (deleted after 24 h) and moves to `media/shipment_files/shipment_<lot id>.pdf` on save. New cards are linked to card data by a background task, which then puts English names of cards coded in another language (`-FR`) in that language (ygoprodeck; recent sets may have no translation yet), and replaces deck proxies by the new owned copies (unused proxies deleted). Saving deck zones also deletes the proxies left unused.
- `GET /api/sets/:code` (`routes/reference/sets.rs`): the cards of a set for the quick-add panel (ygoprodeck, cached; see docs/tour.md, "Talking to ygoprodeck").
- Deck builder API (`routes/decks/api/`): `/api/deck-versions/:id` `GET`/`PATCH`/`DELETE`, `…/clone`, `…/proxies[/:card_id][/replace]`, `…/reserve`, `…/categories[/:category_id]`, `…/cards/:card_id/categories[/:category_id]`, `…/monte-carlo`, and `/api/decks/:id/import-ydke`. The `PATCH` also builds/unbuilds (`is_built`, answer with `build_report`; saving a built version's zones takes its new cards from the other versions the same way) and notes `deck_box`/`sleeves`. Proxies of a built version are never swapped automatically (`replace_proxies` skips them): `…/proxies/:card_id/replace` ("Use my copy") does it by hand. Zone edits stay local in the app until "Save"; proxies and categories save immediately; the tournament panel only links to the tournament pages; YDKE copy is computed client-side.

## Database conventions (`migrations/0002_ygo_schema.sql`)

- Primary keys are the Django pks, imported verbatim.
- Money (`price`, `no_card_price`, `amount`) is **TEXT**, never REAL (Django `DecimalField`). Read/write it as `db::Money` / `rust_decimal::Decimal`; never `SUM()`/`MIN()` it in SQL.
- Dates are ISO `YYYY-MM-DD` TEXT. Booleans are INTEGER 0/1.
- Django M2M → join tables `deck_version_{main,extra,side}_deck` (see `cards::Zone`), `card_category_condition`.
- Django MTI `CardConditionalCategory(CardCategory)` → **flattened** into `card_category` with nullable `condition_description`, `categorie_true_id`, `categorie_false_id`. "Is conditional" = `condition_description IS NOT NULL`.
- `card.rarity` (0004): the label of `yugioh_domain::rarity::Rarity` (`Ultra Rare`), NULL when unknown; no CHECK, the forms validate against `Rarity::ALL`.
- `advent_calendar.lot_id` (0006): the calendar's own lot (created from its edit page, `POST /advent-calendar/:year/lot`), NULL when none.
- Books (0007): `book` (`isbn` = ISBN-13 digits, UNIQUE: one row per edition/format; nullable), `book_author` (`sort_name` surname first), `book_book_author` (ordered by `position`), `book_series`, `book_data` (ISBN lookup cache: JSON of `books_domain::book::BookInfo`, NULL = nothing found, asked again after 7 days), `book_advent_calendar[_entry]` (own tables, not shared with the card calendars). **Books can be deleted** (unlike cards); authors/series without books are deleted with them.
- `card_price` (0008): ygoprodeck's market price per `(card_data_id, date)`, weekly, never pruned; `cardmarket_price` EUR (used), `tcgplayer_price` USD (kept for later), NULL = none (`"0"` too). Totals are never stored: computed from these rows and the lot dates (`yugioh_domain::valuation`).
- Cardmarket (0009): `cardmarket_expansion` (set code → idExpansion, NULL = not found), `cardmarket_product` (products named like our card data, latest prices), `cardmarket_price` (history of the linked products), `card.cardmarket_product_id` + `card.cardmarket_product_manual` (1 = chosen on the edit page, kept by the refreshes). Never store the downloaded files.
- Built decks (0010): `deck_version.is_built` (locks the version's real cards: out of the collection for every other version; read through `cards::locked_elsewhere`), `deck_box`, `sleeves` (free text, NULL when not noted).
- Tournaments (0011): `tournament` + `placement`/`players`/`conclusion`/`is_team` (`notes` = the old free text, kept until cleared), `tournament_round` (result `win`/`loss`/`double_loss`/`bye`/`forfeit_win`, no drawn rounds), `tournament_game` (`won`/`lost`/`draw`/`unfinished`, any number), `deck_label` (UNIQUE NOCASE) with `deck_deck_label` and `tournament_round_label`. The version played stays `deck_version.tournament_id` (one at most).
- Pokémon (0012): `pokemon_card` (TCGdex's cards owned or wished, UNIQUE `(catalogue, tcgdex_id)`, `catalogue` `intl`/`ja`: a copy's language `ja` ⇔ the Japanese catalogue; asked again after 30 days), `pokemon_copy` (never deleted: `sold_on`/`sold_price`; `source` booster/single/gift/trade/other; graded: `grader`, `grade`, `cert_number`, `estimate` typed by hand), `pokemon_wish` (`got_copy_id`), `pokemon_price` (`prices` JSON per variant per date, never pruned), `pokemon_set` (set cache by printed code, 7 days; several sets can share a code: 30C), `pokemon_opening` (0013: booster openings, `pokemon_copy.opening_id`; its language and date are its cards'), `pokemon_card.image_from` (0014/0015: where a picture TCGdex lacks comes from, `pokemontcg:<id>` (the Pokémon TCG API, `pokemon/src/store/ptcgio.rs`), `''` = none found, NULL = not looked up). A card nobody owns or wishes is deleted (`cards::delete_orphans`). Never call TCGdex inside a write transaction; tests insert these rows.
- `card_translation` (0005): ygoprodeck card names in other languages by `(ygopro_id, language)`, a cache: a name found is kept, `name` NULL = no translation on `checked_at`, asked again after 7 days (`card_data::translated_name`). Never ask ygoprodeck for a translation without it.
- Domain names are kept from Django (some French): `unite` (one card in a lot), `apport` (one-off budget contribution), `categorie_true/false`.
- `0002` mirrors Django's constraints (UNIQUE, CHECK on choice fields, nullability, ON DELETE; PROTECT → RESTRICT). Empty Django FileFields (`""`) are imported as NULL.
- `sqlx::migrate!` checksums applied migrations. Cutover is done: **never edit an existing migration**, only add new numbered files (next: `0016_…sql`). They run on `data/app.db` at the next startup.
- sqlx 0.7 + SQLite concurrency (see `common/src/db.rs`): write transactions start with `db::begin_write(&pool)`, never `pool.begin()`; never `INSERT … RETURNING` + `fetch_one` (not committed until the statement is reset) — use `execute()?.last_insert_rowid()`. Do network calls (ygoprodeck, Open Library) **before** opening a write transaction: they write card data through the pool.
- SQLite + FKs: never drop/rebuild a parent table in a migration while `foreign_keys` is on — `DROP TABLE` fires ON DELETE CASCADE in children.

## Privacy rule: shipment PDFs

Cardmarket shipment PDFs (uploads, `data/media/shipment_files/`, and the test PDFs in `my_ygo_cards/tests/doc/` at tag `django-final`) contain **personal data** (names, addresses).
- Never store their extracted text anywhere: not in the DB, logs/tracing, error messages, test fixtures, snapshots, docs, commits, or Claude memory. Only the parsed fields the app needs (dates, total, seller name, card lines) may be persisted.
- Parse the text in memory and drop it. Tests must read the PDF at runtime and assert on parsed fields; never paste PDF text into source files. Unit tests use synthetic text.
- Use `lopdf` for extraction: `pdf-extract` `println!`s text fragments to stdout. Check any new PDF crate for prints/logs of content.
- To debug the layout, print a masked shape (letters → `a`, digits → `9`), never the text.
- Don't print or quote PDF contents in chat/terminal output, and don't copy the PDFs to new locations without asking.

## Working conventions

- Handlers return `AppResult<T>`; use `?` and `.or_404()`, never `.unwrap()` on DB results. API input errors: `bad_request(...)` (JSON `{"error": …}`).
- New behaviour goes where it belongs: pure logic in the collection's `*-domain` crate, card-row writes in `crates/yugioh/src/store/cards.rs`, ygoprodeck in `yugioh/src/store/card_data/`, pages/handlers in `routes/<area>/` of the collection's crate; what two collections need, once, in `common` (or `common-domain`, `advent`).
- Tests: pure logic as unit tests in the domain crates; handlers and commands through each crate's `crate::tests::TestApp` (a wrapper of `common::testing::TestApp`; pre-insert any card data so nothing hits the network).
- Frontend (`frontend/`): **4-space indentation and airy code** (blank lines between logical blocks, after guards/early returns, around try/catch); `.editorconfig` sets it.
- Askama quirks: template method arguments are passed inconsistently by value/reference — prefer argument-less helper methods (`page.next_href()`); `{% let x = field %}` moves, so expose `&` getters for non-Copy fields.
- Format on save is on (rustfmt via rust-analyzer); keep clippy clean.
- Idiomatic Rust is preferred over strict Django parity (enums, `Result` instead of crashes, fixing Django bugs); note each deliberate difference in the module's doc comment. Clean up code freely while working.
