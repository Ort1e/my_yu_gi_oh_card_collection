# Recipes

Step-by-step guides for the usual changes. Read [tour.md](tour.md) first for
how the pieces fit. Everything runs inside the Nix shell (direnv loads it),
from the repo root.

- [Daily workflow](#daily-workflow)
- [Save your data to git](#save-your-data-to-git)
- [Use the app from a phone](#use-the-app-from-a-phone)
- [Add books](#add-books)
- [Add a read-only page](#add-a-read-only-page)
- [Add a form that changes data](#add-a-form-that-changes-data)
- [Change the database schema](#change-the-database-schema)
- [Add pure business logic](#add-pure-business-logic)
- [Add a deck builder feature](#add-a-deck-builder-feature)
- [Write a handler test](#write-a-handler-test)
- [Rust patterns you'll meet](#rust-patterns-youll-meet)

## Daily workflow

```bash
cargo run -p backend              # http://127.0.0.1:3000, Ctrl-C to stop
cargo test --workspace            # all tests (temporary DBs, never data/app.db)
cargo clippy --workspace --all-targets
```

- Rust code or **templates** changed → stop and `cargo run` again (templates
  are compiled into the binary). `cargo install cargo-watch` +
  `cargo watch -x 'run -p backend'` restarts it on each save, if you want
  that.
- Deck builder changed → `cd frontend && npm run build`, or `npm run dev`
  for hot reload at `http://localhost:5173/?id=<deck version id>` (backend
  running on :3000).
- The terminal shows `INFO` logs by default: one line per page or API call,
  plus one per change to the collection ("lot created lot_id=153 cards=4").
  `RUST_LOG=debug cargo run -p backend` adds the SQL and more detail;
  `RUST_LOG=info,sqlx=warn`-style lists tune it per module (see `EnvFilter`).
- To look at the data: `sqlite3 data/app.db` (read-only queries are
  harmless while the server runs).

## Save your data to git

`data/app.db` is the real collection, tracked in the private `data`
submodule. While the server runs, recent writes sit in `data/app.db-wal`
(gitignored). Stopping the server with Ctrl-C (or SIGTERM) writes them
into `app.db` and removes the WAL. So:

```bash
# 1. stop the server (Ctrl-C)
# 2. commit the data, in the submodule
git -C data add -A
git -C data commit -m "add lot from <seller>"
git -C data push
# 3. record the new data version in the app repo
git add data
git commit -m "update data"
git push
```

If `data/app.db-wal` still exists after the server stopped (killed with
`kill -9`, crash…), open and close the DB once to fold it in:
`sqlite3 data/app.db "PRAGMA wal_checkpoint(TRUNCATE);"`.

Never delete or regenerate `data/app.db`: the importer only works on an
empty DB, and the Django dump is older than your Rust-era changes.

## Use the app from a phone

The phone talks to the server running on the PC, over the home Wi-Fi. By
default the server only answers the PC itself (`127.0.0.1`).

**Once:**

1. **Open port 3000 in the NixOS firewall.** In
   `/etc/nixos/configuration.nix`, next to `networking.networkmanager.enable`:
   ```nix
   networking.firewall.allowedTCPPorts = [ 3000 ];
   ```
   then apply it: `sudo nixos-rebuild switch --flake /etc/nixos` (or your
   usual rebuild command).
2. **Give the PC a fixed address.** In the internet box's admin page
   (often `http://192.168.1.1`), look for "DHCP" → "static lease" /
   "reservation" and reserve the PC's current address for it. To see
   that address: `hostname -I`, the first `192.168.…` one (the `172.…`
   ones are Docker's). Without this, the address may change some day and
   the phone's bookmark stops working.
3. **Listen on the network, with a password.** Add to `.env` (gitignored,
   read at startup):
   ```bash
   BIND_ADDR=0.0.0.0:3000
   APP_PASSWORD=<a password>
   ```
   The server refuses a network address without `APP_PASSWORD`: without
   login, anyone on the Wi-Fi could edit the collection and read the
   shipment PDFs.

**Each time:**

1. `cargo run -p backend` on the PC; the log says
   `listening on 0.0.0.0:3000, login enabled`.
2. On the phone, **on the home Wi-Fi** (not mobile data):
   `http://<PC address>:3000`, e.g. `http://192.168.1.51:3000`. Bookmark
   it, or "Add to home screen" for an app-like icon.
3. Reading pages works directly; editing asks for the password once (the
   session lasts, restarts included).

**If the page doesn't load:** the server running? Phone on the same
Wi-Fi (some boxes have a separate guest network that can't see the PC)?
Port open (`sudo iptables -L nixos-fw -n | grep 3000`)? From the PC itself,
`http://192.168.1.51:3000` must work too.

**Limits:** it's plain HTTP. Fine at home, but the password crosses the
Wi-Fi unencrypted, and browsers keep the live camera to HTTPS sites (the
book barcode scan takes a photo instead). Don't forward port 3000 on the
box to reach it from outside. For outside access and HTTPS, the clean way
is Tailscale (`services.tailscale.enable = true;` on the PC, the app on the
phone, then `tailscale serve`); not set up yet.

To go back to PC-only: remove `BIND_ADDR` from `.env`.

## Add books

In the app: **Books** in the menu (or `/books`) → **Add**.

1. The ISBN: type it (13 digits under the barcode; hyphens are fine, an old
   10-digit one too), or **📷 Scan** on a phone (take the photo close,
   flat and well lit), or a USB barcode scanner on the PC.
2. "Look up" fills title, authors, publisher, year, pages, language, often
   the series (check it: the catalogues write it in many ways). "Already in
   the list" means this edition is there; another edition or format has
   its own ISBN.
3. Fix what needs it, **Add the book**: the empty form comes back for the
   next one.

A book without an ISBN: fill the fields by hand. A series page lists the
missing volumes, each with a link to add it. A book given away: its edit
page → **Delete this book** (refused while it is behind an advent calendar
door: empty that day first).

**Book advent calendar:** Books → **Advent** → **Add calendar**, then for
each day its book (its ISBN: a book not in the list yet is looked up and
added; or `#id`) and its photo. Photos in
`data/media/book_advent_calendar_photos/<year>/`: commit them with the
data, like the card calendars.

## Add Pokémon cards

In the app: **Pokémon** in the header (or `/pokemon`) → **Add**.

- **A booster opening** (Add → A booster opening): the set code printed at
  the bottom of the cards, next to the number (**FNE** 067/064 on a French
  card, SFA on an English one, the set id like SV6a on a Japanese one), the
  language, the date, then the numbers: `12 34 45r 67x2` (`r` reverse holo,
  `h` holo, `x2` two copies; a card numbered by a letter, like the Mew
  cards of 30th Celebration: `B`). 30th Celebration (**30C**) shares its
  code with its Classic Collection: type the set size after the number
  (`1/30`), or pick the card on the next step. A Classic Collection
  reprint shows the old card's number: type it with its size (`33/181`)
  and pick it in the list, or find it by name (`30C Zekrom`). A Trainer
  Gallery card (TG29/TG30) prints its main set's code: `ASR TG29`, or
  `TG29` alone in the finder (every TG29 is listed, with its set). A
  Japanese card: its set id and number, the rarity letters after it are
  fine (`SV3a 071/062 AR`, `sv3a 071/062ar`), with the language Japanese. Cards come
  in near mint unless changed.
- **The openings** (Openings in the header): each booster with its cards
  and value. A card forgotten: the opening's page → **Add cards by
  number** or **Add one card**; a card in the wrong opening: its edit
  page, "From a booster opening". Saved in the wrong language (the form
  starts in the last opening's): **Date, language, note** changes all its
  cards. **Find the cards** shows each card with its
  picture and rarity: check the variant (the guide says how to tell them
  apart), full art, condition; a count of 0 leaves a card out. **Save**.
- **One card** (Add → One card: a single, a gift, a graded card): find it by
  its printed code (`FNE 067`) or its name in the language chosen below
  (Japanese: a Japanese name, or the English/French name of the Pokémon),
  pick it, choose its variant among those printed, then the rest.
- **The wishlist**: Wishlist → Add a wish (a card, the language and variant
  wanted or any, the most you would pay, a priority). Its row turns green
  when Cardmarket's trend is at or under your price. Adding a copy that
  fulfils a wish offers "Mark it got"; "I got it" on a wish opens the add
  form pre-filled.
- A card sold or given away: its page → **Edit or mark sold** (a date, a
  price or none). Cards are never deleted.

Prices come with the weekly refresh (`refresh-prices`), and when a card is
first added. Pictures are a cache in `data/media/pokemon_card_images/`
(not committed, like `card_images/`): downloaded again when missing.

## Add a read-only page

Example: a `/tournaments` list.

1. **Handler + template struct**: new folder `backend/src/routes/tournaments/`
   with a `mod.rs` (copy `sellers/mod.rs` as a starting point; a new area
   gets its own folder, never a loose file in `routes/`):

   ```rust
   use askama::Template;
   use axum::extract::State;

   use crate::web::error::AppResult;
   use crate::AppState;

   #[derive(Debug, sqlx::FromRow)]
   pub struct TournamentRow {
       id: i64,
       name: String,
   }

   #[derive(Template)]
   #[template(path = "tournaments/list.html")]
   pub struct TournamentsList {
       tournaments: Vec<TournamentRow>,
   }

   pub async fn list(State(state): State<AppState>) -> AppResult<TournamentsList> {
       let tournaments = sqlx::query_as("SELECT id, name FROM tournament ORDER BY name")
           .fetch_all(&state.pool)
           .await?;
       Ok(TournamentsList { tournaments })
   }
   ```

   Check the real column names in `migrations/` first: a column the struct
   expects but the SQL doesn't return is an error at request time (the
   compiler doesn't check SQL).

2. **Template**: `backend/templates/tournaments/list.html`, extending
   `base.html` (or `list_base.html` for a paginated list). Every field of
   the struct is a variable; `{% for t in tournaments %}…{% endfor %}`.
   A table of data follows the shared convention (`static/css/style.css`,
   "Data tables"): `<div class="table-scroll"><table class="numbers">`, on
   the page background, amounts as plain numbers with the unit in the
   heading ("Price (€)"), `class="text"` on text columns after the first.
   To sort it by its headings: `class="sortable"` plus `data-sort="text"` /
   `"number"` on the `<th>`s and `static/js/tables.js` in `extra_js` (which
   also gives a search field: `<input type="search" data-filter-table="id">`,
   see `lots/detail.html`).

3. **Route**: in `routes/mod.rs`, `mod tournaments;` and in `router()`:
   `.route("/tournaments", get(tournaments::list))`.

4. **Link** it from `templates/base.html` (the navigation) if needed: in the
   group it belongs to (`Collection`, `Decks`: a `<details>` menu), or at the
   top level. `static/js/nav.js` marks the current page by itself.

5. Add the URL to the `pages_render` test in `tests/pages.rs`.

A row type needed by several pages goes in `store/models.rs`. A value the template
must compute is best a method on the struct (`fn full_name(&self) -> String`)
called as `{{ row.full_name() }}`: askama is awkward with method arguments
(see CLAUDE.md, "Askama quirks").

## Add a form that changes data

Copy `sellers/form.rs` (one entity) or `decks/form.rs` (create + edit +
delete), as `form.rs` in the folder of its area. The shape:

```rust
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(pairs): Form<Pairs>,
) -> AppResult<Response> {
    let mut form = FormData::new(pairs);
    let name = form.required("name", "Name");          // Option<String>, error recorded
    let price = form.required_decimal("price", "Price");

    // invalid → render the page again with what was typed + the errors
    let (Some(name), Some(price), true) = (name, price, form.errors.is_empty()) else {
        let page = EditThingPage { errors: form.errors, name: form.text("name"), … };
        return Ok(page.into_response());
    };

    let mut tx = begin_write(&state.pool).await?;      // always begin_write for writes
    sqlx::query("UPDATE thing SET name = ?, price = ? WHERE id = ?")
        .bind(&name)
        .bind(Money(price))                            // money is TEXT: wrap in Money
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Redirect::to(&format!("/things/{id}")).into_response())
}
```

Then:

- register the route in **`write_router()`** (not `router()`), which puts it
  behind the login;
- render errors in the template with `{% include "partials/form_errors.html" %}`;
- if the form creates cards, use `cards::insert_lot_cards` and then
  `card_data::spawn_link_cards(state, card_ids)` after the commit;
- if it needs ygoprodeck (`card_data::get_or_fetch`), call it **before**
  `begin_write`.

Cards, unites, lots and sellers are never deleted, by design: don't add
delete routes for them.

## Change the database schema

1. Add `migrations/0004_<what>.sql` (next free number). **Never edit
   `0002`/`0003`**: they are already applied to `data/app.db`, and sqlx
   refuses to start if an applied file changed.
2. Write plain SQLite:

   ```sql
   ALTER TABLE lot ADD COLUMN comment TEXT;
   ```

   - money → `TEXT`, dates → `TEXT` (`YYYY-MM-DD`), booleans → `INTEGER`
     `0/1` with `CHECK (x IN (0, 1))`;
   - SQLite's `ALTER TABLE` is limited (no changing a column's type or
     constraints). Those changes need a new table + copy + rename, and then
     **never** `DROP` a table that other tables reference while foreign
     keys are on: `ON DELETE CASCADE` would empty the children. Ask for
     help or test on a copy first: `cp data/app.db /tmp/try.db` and
     `DATABASE_URL=sqlite:/tmp/try.db cargo run -p backend`.
3. Update the row structs and SQL that use the table, and the importer only
   if you still care about importing Django dumps (you don't need to).
4. `cargo test --workspace`: tests run all migrations on a fresh DB.
5. The next `cargo run -p backend` applies it to `data/app.db`. Commit the
   migration in the app repo and the DB in the data repo.

## Add pure business logic

Anything computable without the DB (a price rule, a parser, a statistic)
goes in `domain/`:

1. a function in the right module (or a new `domain/src/<name>.rs`, plus
   `pub mod <name>;` in `lib.rs`) taking plain values;
2. unit tests at the bottom of the file:

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn shipping_is_split_evenly() {
           assert_eq!(my_function(…), expected);
       }
   }
   ```

3. call it from the handler, which loads the rows, calls the function and
   renders the result. `routes/budget/mod.rs` + `domain/src/budget.rs` is the
   model to follow.

`cargo test -p domain` runs these in a second.

## Add a deck builder feature

1. **Endpoint** in `routes/decks/api/` (the file of its group, or a new one
   declared and re-exported in `api/mod.rs`): input as a `#[derive(Deserialize)]`
   struct taken with `Json(body): Json<MyInput>`, output as
   `Json<MyOutput>` with `#[derive(Serialize)]`. Bad input →
   `return bad_request("why")` (the app shows the message). Money
   serialises as a `"12.50"` string.
2. **Route** under `/api/deck-versions/:id/…`: in `write_router()` if it
   changes anything, `router()` if it only reads.
3. **Client** in `frontend/src/api.js`: one line in `deckApi(id)`, e.g.
   `myThing: (x) => request('POST', \`${base}/my-thing\`, { x }),`.
4. **UI** in a component in `src/lib/` (with its CSS in its `<style>`
   block), wired in `App.svelte`. Pure helpers go in `src/deck.js`.
   Style: 4-space indent, blank lines between logical blocks.
5. Test the endpoint in `tests/deck_api.rs` (`t.json(…)`, see
   `categories_and_monte_carlo`), then `npm run build`.

## Add next year's advent calendar

All in the app, behind the login:

1. **Advent** in the menu (the list of calendars) → **Add calendar** → the
   year → **Create the calendar**: its edit page opens.
2. **Create its lot** (top of the edit page): a gift lot (0 €, December 1st,
   seller "Advent calendar") linked to the calendar. Its page opens: add the
   cards there (the quick add takes a set code and numbers). Cards bought
   in other lots (Cardmarket orders) don't need to be in it.
3. Back on the edit page (the lot page links to it), for each day: its card
   (the lot's cards not used yet are suggested in the field; else its code,
   `MAMO-FR023`, or `#id`, the number in the card's page address) and its
   photo. Save. Days can be filled one at a time, through December.
4. A photo still sideways (some phones don't record the orientation): ↺ ↻.

Then commit the data (stop the server first): `app.db` and the photos in
`data/media/advent_calendar_photos/<year>/` (`media/.gitignore` keeps them).

## Write a handler test

In `backend/src/tests/`, in the file of its area (`TestApp` and the shared
fixtures are in `tests/mod.rs`). `TestApp::new()` gives a fresh migrated DB with
two `card_data` rows (dark magician, kuriboh), so no test calls
ygoprodeck:

```rust
#[tokio::test]
async fn edit_seller() {
    let t = TestApp::new().await;
    t.post("/lots/add", LOT).await;                  // fixture: a lot with a new seller
    let id = t.scalar("SELECT id FROM seller WHERE name = 'New Shop'").await;

    let (status, page) = t.post(&format!("/sellers/{id}/edit"), &[("name", "")]).await;
    assert_eq!(status, StatusCode::OK);              // invalid: page rendered again
    assert!(page.contains("is required"), "{page}");

    let (status, _) = t.post(&format!("/sellers/{id}/edit"), &[("name", "Renamed")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);       // valid: redirect
    assert_eq!(t.scalar("SELECT count(*) FROM seller WHERE name = 'Renamed'").await, 1);
}
```

Helpers: `t.get(uri)`, `t.post(uri, form)`, `t.json(method, uri, body)`,
`t.scalar(sql)` (one integer), `t.text(sql)` (one optional string),
`TestApp::with_password(Some("pw"))` for login tests. Run a single test with
`cargo test -p backend edit_seller`.

## Rust patterns you'll meet

| You see | It means | Python equivalent |
|---|---|---|
| `Option<T>`: `Some(x)` / `None` | a value that may be missing | `x` or `None` |
| `Result<T, E>`: `Ok(x)` / `Err(e)` | success or an error | return value or exception |
| `foo()?` | if `foo` failed, return its error now | letting an exception propagate |
| `.await` | wait for an async operation (DB, HTTP, file) | `await` |
| `let Some(x) = … else { return …; };` | unpack or bail out early | `if x is None: return …` |
| `match value { A => …, B => … }` | exhaustive switch: the compiler checks every case is handled | `match` |
| `&str` vs `String` | borrowed text vs owned text; `.to_string()` converts | both are `str` |
| `&x`, `&mut x` | lend `x` (read-only / writable) without giving it away | (implicit) |
| `#[derive(Debug, Serialize, FromRow, Template)]` | generate code for the struct: debug print, JSON, SQL row, HTML page | decorators / metaclasses |
| `impl Thing { fn method(&self) }` | methods of `Thing` | `class Thing: def method(self)` |
| `Vec<T>`, `HashMap<K, V>`, `BTreeMap` (sorted) | list, dict, sorted dict | `list`, `dict` |
| `.iter().map(…).filter(…).collect()` | transform a collection | comprehension |
| `pub`, `pub(crate)` | visible outside the module / in the crate only | no leading `_` |
| `mod x;` / `use crate::x::y;` | declare a module / import a name | `import` |

When the compiler complains, read the whole message: it usually says what
to change. `cargo clippy` suggests the more idiomatic version.
