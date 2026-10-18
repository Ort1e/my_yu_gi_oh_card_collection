# Spec: readability refactor

TODO.md, "Refactor the code base to make it easier to read". The code is
already consistent; most of the cost comes from a few long files and from
SQL or checks that are copied where they could be shared. Nothing here
changes behaviour: `cargo test --workspace` must pass unchanged after each
step (except for moved tests).

**Status: done (2026-10-18)**, all steps below, one commit each (the backend
too, exceptionally by Claude). Kept as the record of what moved where.
The paths below are the ones before the folder reorganisation that came
right after (`web/`, `store/`, `routes/<area>/`, `templates/<area>/`, see
CLAUDE.md).

## Backend (owner)

### 1. Split `routes/deck_api.rs` (1134 lines) into a module

One file per section, the sections already exist (`// ---------- … ----------`):

```
routes/deck_api/
  mod.rs          ApiCard, CardRecord, load_cards, push_ids, zone_card_ids, Created
  version.rs      DeckVersion, load_version, get/update/delete/clone_version
  proxies.rs      add_proxy, delete_proxy
  reserve.rs      reserve
  categories.rs   ApiCategory, ApiCondition, load_categories, add/delete/assign…
  simulation.rs   monte_carlo
  tournament.rs   save_tournament
  ydke_import.rs  import_ydke (+ the `ydke()` helper of version.rs if you prefer)
```

`routes/mod.rs` keeps the same paths (`deck_api::version::get_version`, or
`pub use` them in `deck_api/mod.rs` to leave `routes/mod.rs` alone). The
linked decks TODO will add code here, so it is worth doing before it.

### 2. Zone SQL in one place (`cards.rs`)

- `INSERT INTO {zone table} (deck_version_id, card_id)` is written 4 times
  in `deck_api.rs` (update, clone, add_proxy, YDKE import). → a method
  `Zone::insert(self, conn, version_id, card_id)`.
- `zone_card_ids` (deck_api.rs) → `Zone::card_ids(self, conn, version_id)`.
- "card ids in this version" (UNION of the 3 zone tables with
  `deck_version_id = ?1`) is written 3 times: `deck_api.rs` update_version,
  reserve, and `commands/clean_cards.rs`. → a const next to `IN_ANY_DECK`:
  `IN_VERSION` (`… WHERE deck_version_id = ?1 UNION …`). `reserve` builds its
  query with `QueryBuilder` and binds the id 3 times; with the const it can
  push `" AND card.id NOT IN ("` + the const, then bind once if you switch
  that query to numbered placeholders, or keep the builder and only reuse
  the const elsewhere.

### 3. One way to check that a row exists

Today there are three:
- `deck_api.rs`: `exists(conn, table, id)` / `row_exists(...)`;
- `lot_form.rs` (×3), `deck_form.rs`: `query_scalar("SELECT id FROM x WHERE id = ?")…ok_or(AppError::NotFound)`;
- `delete_category`: `rows_affected() == 0`.

→ move `exists`/`row_exists` to `db.rs` and use them in the forms. The
`rows_affected()` ones are fine (the check is the write itself).

### 4. Shorten `lot_form.rs` `create` (125 lines)

- `find_or_create_seller(tx, seller_id, name, is_person, source)` →
  `AppResult<i64>`: the `match (seller_id, seller_name)` block.
- `attach_shipment(tx, media_dir, lot_id, pending_file)` →
  `AppResult<Option<(PathBuf, PathBuf)>>`: the "move the PDF" block.
- The pending-file helpers (`TOKEN`, `PENDING_MAX_AGE`, `pending_path`,
  `remove_stale_uploads`, `shipment_destination`, and these two) can go to
  their own file `routes/shipment_files.rs`, leaving `lot_form.rs` with the
  forms only.
- The seller list query is written twice (`AddLotPage::new` and `sellers()`);
  `AddLotPage::new` can call `sellers()`.

### 5. Small input helpers

The "trim, empty → error / None" pattern is in `update_version`,
`clone_version`, `add_proxy`, `add_category`, `save_tournament`
(`blank_to_none`), `import_ydke`. Two helpers in `error.rs` or a small
`input.rs` would make the handlers shorter:

```rust
/// Trimmed text, `None` when absent or blank.
pub fn non_blank(s: Option<String>) -> Option<String>;
/// Trimmed text, or `bad_request(message)` when blank.
pub fn required<'a>(s: &'a str, message: &str) -> AppResult<&'a str>;
```

### 6. Split `tests.rs` (1224 lines)

`tests/mod.rs` with `TestApp` and its helpers, then one file per area,
the way the tests are already grouped: `pages.rs`, `lots.rs`, `decks.rs`,
`deck_api.rs` (with its `json` / `deck_fixture` helpers), `forms.rs` (edit
card/seller/lot), `auth.rs`.

## Frontend (Claude)

1. Move the inline scripts of the templates to `static/js/`, like
   `card_rows.js`: `dashboard.html` (charts), `rarities.html`.
2. Split `App.svelte` (395 lines): the reserve search, the YDKE toolbar and
   the version/ban list panel become components in `lib/`; `App.svelte` keeps
   the zones and the moves.
3. Move the CSS of each Svelte component from `deck_builder.css` into the
   component's `<style>` block; keep the shared rules (`.area`, `.hint`,
   layout grid) in `deck_builder.css`.
