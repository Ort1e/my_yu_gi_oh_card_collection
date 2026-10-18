# Recipes

Step-by-step guides for the usual changes. Read [tour.md](tour.md) first for
how the pieces fit. Everything runs inside the Nix shell (direnv loads it),
from the repo root.

- [Daily workflow](#daily-workflow)
- [Save your data to git](#save-your-data-to-git)
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

## Add a read-only page

Example: a `/tournaments` list.

1. **Handler + template struct**: new file `backend/src/routes/tournaments.rs`
   (copy `sellers.rs` as a starting point):

   ```rust
   use askama::Template;
   use axum::extract::State;

   use crate::error::AppResult;
   use crate::AppState;

   #[derive(Debug, sqlx::FromRow)]
   pub struct TournamentRow {
       id: i64,
       name: String,
   }

   #[derive(Template)]
   #[template(path = "tournaments_list.html")]
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

2. **Template**: `backend/templates/tournaments_list.html`, extending
   `base.html` (or `list_base.html` for a paginated list). Every field of
   the struct is a variable; `{% for t in tournaments %}…{% endfor %}`.

3. **Route**: in `routes/mod.rs`, `mod tournaments;` and in `router()`:
   `.route("/tournaments", get(tournaments::list))`.

4. **Link** it from `templates/base.html` (the navigation) if needed.

5. Add the URL to the `pages_render` test in `tests.rs`.

A row type needed by several pages goes in `models.rs`. A value the template
must compute is best a method on the struct (`fn full_name(&self) -> String`)
called as `{{ row.full_name() }}`: askama is awkward with method arguments
(see CLAUDE.md, "Askama quirks").

## Add a form that changes data

Copy `seller_form.rs` (one entity) or `deck_form.rs` (create + edit +
delete). The shape:

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
- render errors in the template with `{% include "_form_errors.html" %}`;
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
   renders the result. `routes/budget.rs` + `domain/src/budget.rs` is the
   model to follow.

`cargo test -p domain` runs these in a second.

## Add a deck builder feature

1. **Endpoint** in `routes/deck_api/` (the file of its group, or a new one
   declared and re-exported in `deck_api/mod.rs`): input as a `#[derive(Deserialize)]`
   struct taken with `Json(body): Json<MyInput>`, output as
   `Json<MyOutput>` with `#[derive(Serialize)]`. Bad input →
   `return bad_request("why")` (the app shows the message). Money
   serialises as a `"12.50"` string.
2. **Route** under `/api/deck-versions/:id/…`: in `write_router()` if it
   changes anything, `router()` if it only reads.
3. **Client** in `frontend/src/api.js`: one line in `deckApi(id)`, e.g.
   `myThing: (x) => request('POST', \`${base}/my-thing\`, { x }),`.
4. **UI** in a component in `src/lib/` (with its CSS in its `<style>`
   block), wired in `App.svelte`. Pure helpers go in `src/deck.js`. Style: 4-space indent, blank lines between logical blocks.
5. Test the endpoint in `tests.rs` (`t.json(…)`, see
   `categories_and_monte_carlo`), then `npm run build`.

## Write a handler test

In `backend/src/tests.rs`. `TestApp::new()` gives a fresh migrated DB with
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
