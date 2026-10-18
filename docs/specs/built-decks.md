# Spec: built decks (a shared collection without overlap)

TODO.md, "Link decks together". Today every deck version uses the whole
collection: the same owned card can be in several versions at once. Goal:
build several decks at the same time from one collection, each physical card
in only one of them, and see what is still missing.

**Status: designed (2026-10-18), all decisions settled.** Backend for the owner (Claude reviews
and suggests), frontend for Claude once the API exists.

## The idea

A flag on the **deck version**: *built*. Building a version **locks** its
real (non-proxy) cards: they belong to it alone.

- **Building takes the cards.** When a version is built, each of its real
  cards that is also in other versions is **replaced by a proxy in those
  other versions**. The proxy is a copy of the card (same name, card data),
  so those decks still show it, now as a card to get.
- **A built deck keeps its cards.** If one of the cards of the version being
  built is already locked by another built version, it is the version being
  built that gets a proxy instead (first built, first served).
- **Locked cards are out of the collection** for every other version: the
  reserve search, the YDKE import and `clean-cards` never pick them.

So the rule always holds: a real card in a built version is in no other
version. Nothing new is needed for "missing": in this app a proxy already
means a card you don't have for that deck. Building a deck only creates
proxies where its cards were used, so the other decks show them as missing
like any other proxy.

Proxies are never locked (each one belongs to one version already). Sold
cards are already out of the collection; nothing changes for them.

Words used below:
- **built version**: `deck_version.is_built = 1`;
- **locked card**: a non-proxy card in a zone of a built version;
- **locked elsewhere** (seen from version V): locked by a built version
  other than V.

## Decisions

Settled:
- building a version turns its shared cards into proxies in the other
  versions (above); nothing is refused;
- a version can be built and unbuilt at any time;
- the flag is per **version**: several versions of the same deck can be
  built at once (they lock different cards like any two versions);
- a built version can be edited. **Saving zones with a card locked elsewhere
  is refused** (400, for built and not built versions alike) with a message
  that says which cards, where they are and what to do (section 4). It can
  only come from a stale page: the reserve never offers locked cards;
- each version can note its **deck box and sleeves** (section 1b);
- **unbuilding** gives the cards back to the collection; the proxies made
  in other versions stay proxies. `clean-cards` already replaces proxies by
  free owned copies, so running it puts real cards back in those decks
  (nothing to remember about the swaps).

## 1. The column (migration `0005`)

```sql
ALTER TABLE deck_version ADD COLUMN is_built INTEGER NOT NULL DEFAULT 0
    CHECK (is_built IN (0, 1));
```

Every existing version starts not built, so nothing changes until you flag
one.

## 1b. Deck box and sleeves (same migration)

Where the physical deck lives, to find it on the shelf. On the version (the
version is the physical deck), free text, any version (built or not):

```sql
ALTER TABLE deck_version ADD COLUMN deck_box TEXT;  -- "red Boulder 100+"
ALTER TABLE deck_version ADD COLUMN sleeves TEXT;   -- "Dragon Shield matte black"
```

Free text rather than a colour list: brand, model and colour all help, and a
list would never be complete. NULL when not noted (blank input → NULL, with
`non_blank`).

## 2. Shared SQL (`store/cards.rs`, next to `IN_VERSION`)

```rust
/// SQL subquery: ids of the cards locked by a built version other than the
/// one bound as `?1` (bind 0 for "locked by any built version").
pub const LOCKED_ELSEWHERE: &str = "...";

/// A proxy copy of `card_id` (name, en_name, card_data_id, code), like the
/// copies `clone_version` makes. Returns its id.
pub async fn proxy_copy(conn: &mut SqliteConnection, card_id: i64) -> sqlx::Result<i64>

/// In `version_id`, put `new_card` in place of `old_card` (same zone) and
/// move its category assignments. `clean-cards` does the same in the other
/// direction (proxy → owned card): share the code.
pub async fn swap_card(conn, version_id: i64, old_card: i64, new_card: i64) -> sqlx::Result<()>
```

## 3. Building a version (`decks/api/version.rs`)

`PATCH /api/deck-versions/:id` with `{"is_built": true}`, in one write
transaction:

1. for each real card of the version **locked elsewhere**: `proxy_copy`, then
   `swap_card` in this version (the built deck keeps the original);
2. for each remaining real card that is **in other versions**: in each of
   them, `proxy_copy` + `swap_card`;
3. `UPDATE deck_version SET is_built = 1`.

The answer is the usual `DeckVersion`, plus what happened, for the deck
builder to show:

```json
"build_report": {
  "proxied_here": ["Raigeki (in Deck X - v2)"],
  "proxied_elsewhere": ["Ash Blossom & Joyous Spring → Deck Y - v1, Deck Z - test"]
}
```

(`Option`, absent on other answers, `#[serde(skip_serializing_if = "Option::is_none")]`.)
Log counts only: `tracing::info!(version_id, proxied_here, proxied_elsewhere, "deck version built")`.

`{"is_built": false}` only clears the flag (see "unbuilding" above).

The same `PATCH` takes `deck_box` and `sleeves` (each optional; present and
`null` or blank clears it, like `ban_list_id` with `present`), and
`DeckVersion` returns them.

`clone`: the copy is **not** built. It is made of the original's real
cards, so building it later would turn them all into proxies in the copy.

## 4. Where the collection is read

Every place that picks "an owned card" must skip the cards locked elsewhere:

| Where | Today | Change |
|---|---|---|
| reserve search (`decks/api/reserve.rs`) | owned, unsold, not in this version | `AND card.id NOT IN (LOCKED_ELSEWHERE)` bound to the version |
| YDKE import (`decks/api/ydke_import.rs`) | first owned unsold copy not used by the import | same condition (the new version is not built: bind 0) |
| `clean-cards` proxy replacement (`commands/clean_cards.rs`) | owned unsold copy not in the version | same condition, bound to the version of the proxy |
| saving zones (`update_version`) | cards must exist | a card locked elsewhere → 400, message below |

The saving error names each card and the built version holding it, and says
what to do:

```
Raigeki and Ash Blossom & Joyous Spring are already in a built deck
(Raigeki: Deck X - v2; Ash Blossom & Joyous Spring: Deck Y - v1), so they are
no longer in your collection. Reload the page: they will be gone from the
reserve; add proxies instead if this deck needs them.
```

(`bad_request(...)`, the deck builder shows the message as is.)

## 5. The view: `/decks/built`

A page in `routes/decks/` (a `built.rs` next to `mod.rs`/`form.rs`),
template `templates/decks/built.html`, linked from `/decks`. Read-only, in
`router()`. For each built version:

- its full name (link to the deck builder), deck box and sleeves, number
  of cards, number of proxies;
- **missing cards**: its proxies (one line per name, with the count), the
  cards you still have to get to play it for real.

Then, at the bottom, the missing cards of all built versions together (name,
count, which decks): the shopping list.

Row struct and query in `built.rs`; the totals per name can be done in Rust
(group by `en_name`) rather than in SQL.

## 6. Frontend (Claude)

- Deck builder, version panel: a "Built (locks its cards)" switch. Turning
  it on asks for confirmation ("cards shared with other decks become proxies
  there"), saves immediately (a `PATCH` with only `is_built`), then shows
  the `build_report` and reloads the zones (some cards may have become
  proxies here).
- Deck builder, version panel: "Deck box" and "Sleeves" text fields, saved
  when they lose focus (`PATCH` with only that field).
- `/decks`, deck page: a "Built" badge on built versions, with the deck box
  and sleeves when noted (`partials/deck_version_item.html`,
  `DeckVersionRow` gains `is_built`, `deck_box`, `sleeves`).
- `/decks/built`: the page above.

## 7. Tests (a new `backend/src/tests/built_decks.rs`)

- building a version: its cards shared with another version become proxies
  there (same name, card data, categories moved), the built version keeps
  the originals;
- building a second version that shares a card with a built one: the card
  becomes a proxy in the second version, the first keeps it;
- the reserve of another version no longer returns locked cards; unbuilding
  gives them back;
- saving zones with a card locked elsewhere → 400, the message names the
  card and the built version;
- two versions of the same deck can both be built;
- deck box and sleeves: saved, blank clears them, returned by the API;
- YDKE import picks a free copy, else a proxy, when the only owned copy is
  locked;
- `clean-cards` doesn't replace a proxy by a locked card, and after
  unbuilding it puts the owned card back;
- `/decks/built` lists the proxies of the built versions.
