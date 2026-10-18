# CLAUDE.md

Personal Yu-Gi-Oh! card collection manager: cards owned, purchase/sale lots, sellers,
monthly budget, decks (versions, categories, YDKE import/export, draw simulation), ban lists.

## History — read first

- `main` is the **Rust app** (the one in use). Its live data is `data/app.db` (data submodule).
- It replaced a **Django** app (cutover 2026-10-13). The Django code is only in history, at tag
  `django-final`: `git show django-final:my_ygo_cards/models/deck.py`,
  `git ls-tree -r --name-only django-final`, or `git archive django-final my_ygo_cards | tar -x -C <scratch dir>`.
  Its `deck_builder_frontend/` is an abandoned React stub.
- Human-oriented docs: `docs/tour.md` (architecture), `docs/recipes.md` (how to change things). Keep them in sync.

## Division of work — read first

The port is finished; the project now serves as Rust practice for the owner.
- **Frontend: Claude implements.** Templates (`backend/templates/`), CSS (`static/css/`), the deck
  builder (`frontend/`), page scripts (`static/js/`).
- **Backend: the owner implements the substantial work, Claude suggests.** Rust in `backend/src/`,
  `domain/`, `migrations/`, `importer/`: for new features, data model, API design or real logic, explain,
  review and propose (route, JSON in/out, where the code goes, similar code to copy from), then build the
  frontend against the API once it exists. Small, mechanical backend edits (e.g. log lines across
  handlers) Claude may make directly; when unsure whether a change is small, ask.

## Stack

| Layer | Tech |
|---|---|
| HTTP | axum 0.7, tower-http (ServeDir, trace) |
| DB | SQLite (WAL) via sqlx 0.7 (runtime queries, `sqlx::migrate!`) |
| Pages | askama 0.12 + askama_axum templates, scripts in `static/js/` (no inline JS), Chart.js from a CDN on the dashboard |
| Deck builder | Svelte 4 + Vite 5 app in `frontend/` (Chart.js), built to `static/deck-builder/` (gitignored), talks only to `/api/*` |
| Env | Nix flake (`flake.nix`, `.envrc` → `use flake`), Rust 1.88.0 pinned in `rust-toolchain.toml` |

Cargo workspace (`Cargo.toml`), shared deps under `[workspace.dependencies]`:
- `backend/` — web server **and** maintenance commands (one binary):
  - `main.rs` (`AppState`: pool, reqwest client, media/uploads dirs, Cardmarket account, `Auth`; dispatches `commands::Command`; `app()` wires the routers and the login layer), `auth.rs` (single-password login: `require_login` middleware, `/login`, `/logout`, HMAC-signed session cookie, key in table `app_secret`), `db.rs` (pool, `begin_write`, `exists`/`row_exists`, `Money`), `error.rs` (`AppError`/`AppResult`, `.or_404()`, `bad_request`, API input helpers `required`/`non_blank`), `filters.rs` (askama filters), `pagination.rs`, `models.rs` (row types shared by pages), `forms.rs` (`FormData`: raw form pairs → validated values + errors; card rows `cards-<i>-<field>`), `card_data.rs` (ygoprodeck lookups with a shared rate limiter, background card→card_data linking), `cards.rs` (card-row writes and shared SQL: `Zone` (+ `card_ids`/`insert`), `SOLD_EXISTS`, `IN_ANY_DECK`, `IN_VERSION`, `insert_lot_cards`, `new_proxy`, `delete_orphan_proxies`), `shipment_files.rs` (pending upload → lot shipment file moves), `tests.rs` (`TestApp`: handler tests on a temp migrated DB, no network).
  - `routes/<domain>.rs`: one module per page group (row structs, queries, `#[derive(Template)]` structs, handlers). Read-only routes in `routes::router()`, **every write route in `routes::write_router()`** (behind the login layer). Forms: `lot_form.rs` (create + edit), `deck_form.rs` (create, edit, delete, new version), `card_form.rs`, `seller_form.rs` (edit), `budget_form.rs`; deck builder JSON API: `deck_api/` (one file per route group, shared `ApiCard` in `mod.rs`).
  - `commands/`: `scrap-ban-list`, `clean-cards` (Django's management commands).
  - `templates/`: `base.html`, `list_base.html` (blocks `items`/`filter`/`add_button`), partials prefixed `_`. CSS from Django in `static/css/`.
- `domain/` — pure, unit-tested business logic (no DB/HTTP): `card` (name normalisation, statuses), `ydke`, `lot`, `budget`, `monte_carlo`, `cardmarket` (shipment PDF parsing via lopdf), `ban_list` (Konami JSON). Each module's doc comment lists its deliberate differences from Django.
- `importer/` — one-off loader from a Django `dumpdata` JSON into the sqlx schema (used once for the cutover; refuses a non-empty DB).
- `migrations/` — sqlx migrations, embedded by both binaries (`sqlx::migrate!("../migrations")`).
- `frontend/` — the deck builder Svelte app (`App.svelte`, `lib/*.svelte` with their own `<style>`, `api.js` = HTTP client, `deck.js` = pure helpers).
- `data/` — git **submodule** (`Ort1e/my_yu_gi_oh_card_data`, private): **`app.db` (the live database, tracked)**, `media/card_images/`, `media/shipment_files/`, and the Django leftovers `data.json` (dump `app.db` was imported from) and `db.sqlite3` (Django DB). Its `.gitignore` excludes `*.db-wal`/`-shm`/`-journal`.

## Commands

All inside the Nix shell (`nix develop`, or automatic with direnv). Run from the **repo root** (relative paths: `static/`, `data/app.db`, `data/media`).

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo test -p domain --test real_shipments -- --ignored --nocapture   # parser vs the real PDFs (prints counts/lot ids only)

cargo run -p backend                          # web server, http://127.0.0.1:3000; runs migrations on startup
cargo run -p backend -- scrap-ban-list        # import new/current Konami ban lists (--all, --dry-run)
cargo run -p backend -- clean-cards           # normalise names, relink card data, replace/delete proxies (--dry-run)

DATABASE_URL=sqlite:/tmp/test.db cargo run -p importer -- data/data.json   # import the Django dump into a scratch DB (never over data/app.db)

cd frontend && npm install && npm run build   # → static/deck-builder/, needed by /deck-builder/:id
cd frontend && npm run dev                    # hot reload at http://localhost:5173/?id=<deck version id>, proxies to :3000
```

`.env` sets `DATABASE_URL=sqlite:data/app.db` (the default too). Both binaries create the DB file if missing.
`data/app.db` is **real data tracked in git**: don't delete/rebuild it, and don't run tests or experiments against it (use a scratch DB). The server closes the pool on Ctrl-C/SIGTERM (graceful shutdown), which checkpoints the WAL into `app.db`; commit it in the submodule only with the server stopped, then bump the submodule pointer here.
Backend env vars (defaults): `BIND_ADDR` (`127.0.0.1:3000`), `APP_PASSWORD` (unset = login disabled, then only a loopback `BIND_ADDR` is accepted), `MEDIA_DIR` (`data/media`), `UPLOADS_DIR` (`uploads`, gitignored), `CARDMARKET_ACCOUNT` (`0rtie`).

## Routes

- Pages: `/` dashboard, `/budget` (+ `/budget/manage`), `/cards` (filters), `/cards/:id`, `/sellers`, `/sellers/:id`, `/lots` (filters), `/lots/:id`, `/lots/add`, `/decks`, `/decks/:id`, `/decks/add`, `/deck-builder/:id`.
- Edition: `/{cards,sellers,lots,decks}/:id/edit` (GET form, POST save); `POST /decks/:id/delete` (versions cascade, orphan proxies removed), `POST /decks/:id/versions` (new empty version → deck builder).
- **Cards, unites, lots and sellers are never deleted** (they are the collection's history): no delete routes for them. Deck versions are edited/deleted only in the deck builder.
- `/card-image/:card_data_id` serves `media/card_images/<name>.jpg`, downloading it once from the URL in `card_data.json_data` if missing. `/static`, `/media` are plain file servers, except `/media/shipment_files` (login required).
- Login (`auth.rs`): with `APP_PASSWORD` set, `write_router()` and the shipment PDFs need the session cookie: pages redirect to `/login?next=…`, `/api/*` answers 401 `{"error": …}`. Reads stay public. New write routes must go in `write_router()`.
- Lot creation: `POST /lots/add/shipment` parses a Cardmarket PDF and re-renders the form pre-filled; the PDF waits as `uploads/pending-<token>.pdf` (deleted after 24 h) and moves to `media/shipment_files/shipment_<lot id>.pdf` on save. New cards are linked to card data by a background task.
- Deck builder API (`routes/deck_api/`): `/api/deck-versions/:id` `GET`/`PATCH`/`DELETE`, `…/clone`, `…/proxies[/:card_id]`, `…/reserve`, `…/categories[/:category_id]`, `…/cards/:card_id/categories[/:category_id]`, `…/monte-carlo`, `…/tournament`, and `/api/decks/:id/import-ydke`. Zone edits stay local in the app until "Save"; proxies, categories and the tournament save immediately; YDKE copy is computed client-side.

## Database conventions (`migrations/0002_ygo_schema.sql`)

- Primary keys are the Django pks, imported verbatim.
- Money (`price`, `no_card_price`, `amount`) is **TEXT**, never REAL (Django `DecimalField`). Read/write it as `db::Money` / `rust_decimal::Decimal`; never `SUM()`/`MIN()` it in SQL.
- Dates are ISO `YYYY-MM-DD` TEXT. Booleans are INTEGER 0/1.
- Django M2M → join tables `deck_version_{main,extra,side}_deck` (see `cards::Zone`), `card_category_condition`.
- Django MTI `CardConditionalCategory(CardCategory)` → **flattened** into `card_category` with nullable `condition_description`, `categorie_true_id`, `categorie_false_id`. "Is conditional" = `condition_description IS NOT NULL`.
- Domain names are kept from Django (some French): `unite` (one card in a lot), `apport` (one-off budget contribution), `categorie_true/false`.
- `0002` mirrors Django's constraints (UNIQUE, CHECK on choice fields, nullability, ON DELETE; PROTECT → RESTRICT). Empty Django FileFields (`""`) are imported as NULL.
- `sqlx::migrate!` checksums applied migrations. Cutover is done: **never edit an existing migration**, only add new numbered files (`0004_…sql`). They run on `data/app.db` at the next startup.
- sqlx 0.7 + SQLite concurrency (see `db.rs`): write transactions start with `db::begin_write(&pool)`, never `pool.begin()`; never `INSERT … RETURNING` + `fetch_one` (not committed until the statement is reset) — use `execute()?.last_insert_rowid()`. Do network calls (ygoprodeck) **before** opening a write transaction: they write card data through the pool.
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
- New behaviour goes where it belongs: pure logic in `domain/`, card-row writes in `backend/src/cards.rs`, ygoprodeck in `card_data.rs`, pages/handlers in `routes/`.
- Tests: pure logic as unit tests in `domain/`; handlers and commands through `crate::tests::TestApp` (pre-insert any card data so nothing hits the network).
- Frontend (`frontend/`): **4-space indentation and airy code** (blank lines between logical blocks, after guards/early returns, around try/catch); `.editorconfig` sets it.
- Askama quirks: template method arguments are passed inconsistently by value/reference — prefer argument-less helper methods (`page.next_href()`); `{% let x = field %}` moves, so expose `&` getters for non-Copy fields.
- Format on save is on (rustfmt via rust-analyzer); keep clippy clean.
- Idiomatic Rust is preferred over strict Django parity (enums, `Result` instead of crashes, fixing Django bugs); note each deliberate difference in the module's doc comment. Clean up code freely while working.
