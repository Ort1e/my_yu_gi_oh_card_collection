# Deck versions as lists, copies only for built decks

Status: **spec for review** (2026-11-07), nothing implemented. When it ships,
move what is still useful into the code's doc comments and `docs/tour.md`
("Built decks", "The deck builder") and delete this file.

## Why

Today every deck version points at `card` rows (`deck_version_{main,extra,
side}_deck`): copies you own, or **proxies** (`card.is_proxy`, a row for a
card you don't have). So:

- a plain version holds *real copies* it doesn't really own: two versions can
  hold the same copy, and the app juggles it (the reserve, `replace_proxies`,
  `delete_orphan_proxies`, the parts of `clean-cards` that keep proxies right,
  proxies made when a version is copied or built);
- "what is missing" is a property of `card` rows (`is_proxy`), counted in 4
  places ([browsing.md](browsing.md) #6);
- nothing can say "I plan to play this deck, here is what I'd buy" without
  creating proxy rows.

The idea: a version that isn't built is a plain **list** (card data and
counts, like the snapshots already are). "Owned / not owned" is *worked out*
from the collection. Only a **built** version holds real copies (and, for the
cards it lacks, a "missing" mark).

## Schema (migration `0026_…sql`)

Zones by card data and count, for every version:

```sql
CREATE TABLE deck_version_card (
    deck_version_id INTEGER NOT NULL REFERENCES deck_version(id) ON DELETE CASCADE,
    zone            TEXT    NOT NULL CHECK (zone IN ('main','extra','side')),
    card_data_id    INTEGER REFERENCES card_data(id),   -- NULL: a name only (no card data yet)
    name            TEXT    NOT NULL,                   -- as shown when card_data_id is NULL
    count           INTEGER NOT NULL CHECK (count > 0),
    position        INTEGER NOT NULL,                   -- the order in the zone
    PRIMARY KEY (deck_version_id, zone, position)
);
```

This is `deck_snapshot_card` (0025) with a version in place of a snapshot:
the snapshot code and `yugioh_domain::deck_history` already speak this shape.

A built version also assigns copies:

```sql
CREATE TABLE deck_version_copy (
    deck_version_id INTEGER NOT NULL REFERENCES deck_version(id) ON DELETE CASCADE,
    zone            TEXT    NOT NULL,
    position        INTEGER NOT NULL,            -- the list line it fills
    slot            INTEGER NOT NULL,            -- 0 .. count-1 within the line
    card_id         INTEGER REFERENCES card(id), -- the copy; NULL = missing (to buy)
    PRIMARY KEY (deck_version_id, zone, position, slot)
);
-- a copy is locked by one built version only
CREATE UNIQUE INDEX idx_deck_version_copy_card ON deck_version_copy(card_id) WHERE card_id IS NOT NULL;
```

(The unique index on `card_id` replaces `locked_elsewhere`: the database says
which copies are taken.) A line of count 3 has 3 rows (`slot` 0-2); "missing" is
`card_id IS NULL`, so **no proxy `card` row exists any more**.

**Categories** (`card_category_assignment.card_id`) move to
`card_data_id` within a version: you categorise "Ash Blossom" in a version,
not one physical copy. `card_category_assignment` gets `card_data_id`, the
data are converted by the migration (a copy's card data; a proxy's too), and
`card_id` is dropped by rebuilding that table only (a leaf table: no FK
children, so no cascade risk).

**Converting the existing versions**: for each version and zone, group the
`card` rows by `card_data_id` (and by name when NULL), count them, keep the
zone order. For **built** versions also fill `deck_version_copy`: real cards
keep their `card_id`, proxies become `card_id = NULL`. Then delete the proxy
`card` rows no list uses (all of them; they are not in the collection) and
drop the three `deck_version_*_deck` tables. Run on a copy of
`data/app.db` first and compare counts per version before and after (a test
for the migration's SQL on a fixture, like 0025's).

## What goes away or changes

| Today | After |
|---|---|
| proxies of plain versions, `new_proxy`, `delete_orphan_proxies` | gone: "+ Missing card" adds a line by name (card data or free text) |
| `replace_proxies` (a new card replaces a proxy in the decks) | gone: ownership is computed, so a bought card is "owned" at once, in every list |
| `locked_elsewhere`, `proxy_copy`, `swap_card` | replaced by the unique index and the building plan below |
| the reserve (a search of copies you own, drag into a zone) | "add by card": the search finds card data; each result shows *owned × n, free × m* |
| `clean-cards`: replace/delete proxies | gone (the other parts stay) |
| `card.is_proxy`, `/cards?proxy=` | the column stays for one release (always 0), then a migration drops it; the card list's filter goes |
| "Use my copy" | a built version's missing line gets a free copy: `deck_version_copy.card_id` set by hand (see below) |

## Building

Building a version `V`, for each of its list lines (card data, count n):
take up to n **free** copies of that card data (owned, not sold, not locked
by another built version, in the best condition first, the ones in the
fewest other lists first); the lines still short get `card_id = NULL`
("missing"). Unbuilding frees the copies. Because the lists don't hold
copies, **other versions are never touched** (today, building proxies the
cards in the other versions: that whole report, `build_report`, disappears).

When a card is bought later (a lot is saved) nothing happens to a built
version until the owner uses **Use my copy** on its missing line (or a
"Take free copies" button that does it for all the missing lines of the
version): built decks stay under the owner's hand, as today.

## Every place affected

- **Deck builder API** (`routes/decks/api/`): `version.rs` (load/save zones as
  lines + counts; `ApiCard` becomes a *line*: card data, count, and for the
  UI `owned`, `free`, `locked_here`, `missing`), `reserve.rs`/`proxies.rs`
  (search by card data, "+ Missing card"), `categories.rs`, `simulation.rs`
  (the Monte Carlo draws from the expanded list: counts become copies),
  `ydke_import.rs` (a YDKE is already passcodes with counts: lines directly),
  `history.rs` (versions and snapshots now the same shape: `snapshots::record`
  copies lines; `deck_history::diff` is unchanged), `mod.rs`. The API's
  TypeScript types follow (`cargo test` rewrites `api-types/`).
- **Frontend**: the zones show a line per card with `×n`; the owned/missing
  badge comes from the API; the To buy tab is built from the missing counts
  (no proxy rows); drag and drop moves counts.
- **Valuation "by deck"** (`collection/sections.rs::deck_values`, now on
  `/decks/built`): a list's value = the market price of the owned copies
  (best-matching free or locked), "to buy" = the price of the missing count.
- **Built decks and errata** (`routes/decks/built.rs`, `store/errata.rs`):
  missing = lines with no copy; "wrong text" copies = assigned copies whose
  printing is outside the window.
- **Card page** (`/cards/:id`, `cards/mod.rs`): the decks using the copy =
  `deck_version_copy` of that card (only built versions hold copies); plain
  versions that *list* the card are shown as "in the list of …" by card data.
- **Lots** (`lots/form/add.rs`, `store/cards.rs::insert_lot_cards`): no more
  proxy replacement after saving.
- **Version page and decks pages** (`routes/decks/version.rs`, `mod.rs`,
  `snapshots.rs`, `decks/api/history.rs`): read lines instead of joining
  `card`.
- **Formats** (`store/formats.rs`): a version's problems already work on card
  data and counts.
- **Cards edit/list/forms** (`cards/*.rs`, `web/card_rows.rs`, templates):
  the "in a deck" filter (`IN_ANY_DECK`, `IN_VERSION`) becomes "has a
  `deck_version_copy`".
- **Advent calendars, collection stats, Cardmarket** touch `is_proxy` only to
  exclude proxies: those conditions are deleted.
- **Backup/export**: the public copy shows a public deck's lines (with its
  missing cards, as it does now with its proxies).
- **Tests**: `testing.rs::deck_fixture`, `tests/{deck_api,built_decks,
  decks,edit_forms,lots,snapshots,collection,cardmarket,advent}.rs` and
  `backend/src/tests/{export,security}.rs` build decks from `card` rows and
  proxies; they are rewritten around lines. About 16 Rust files and as many
  test files: do it in steps (below), each green.

## The owner's functions (`crates/yugioh-domain/`)

Claude writes the stubs (signature, doc comment, `todo!()`) and the unit
tests they must pass; the owner implements:

1. **Ownership** (`ownership.rs`): how many copies of a card a list has.
   ```rust
   /// For one card data: the copies owned (not sold) with their condition,
   /// the ones locked by built versions, and what a list asks.
   pub fn availability(owned: &[Copy], locked: &HashSet<CopyId>) -> Availability;
   pub struct Availability { pub owned: usize, pub free: usize, pub locked: usize }
   ```
   Rules to decide with tests: sold copies don't count; a copy locked by the
   list being looked at counts as its own; condition ordering for "best
   first".
2. **The building plan**, rewritten on lists (`built_deck.rs`, replacing
   `plan_build`):
   ```rust
   pub fn plan_build(lines: &[Line], free: &HashMap<CardDataId, Vec<Copy>>) -> BuildPlan;
   pub struct BuildPlan { pub assigned: Vec<(LineRef, CopyId)>, pub missing: Vec<(LineRef, usize)> }
   ```
   Deterministic (same input → same plan), best-condition copies first, a
   line may be partly filled.
3. **What is missing**, for one list and for several lists sharing the same
   copies:
   ```rust
   pub fn missing(lines: &[Line], free: &HashMap<CardDataId, usize>) -> Vec<MissingLine>;
   /// Lists in priority order (built first, then planned, by the owner's order):
   /// each takes from what the previous left.
   pub fn missing_for_all(lists: &[(ListId, Vec<Line>)], owned: &HashMap<CardDataId, usize>)
       -> HashMap<ListId, Vec<MissingLine>>;
   ```
   The second is the shopping list's rule: two planned decks that both play
   the same 3 *Ash Blossom* you own once need 5 more, not 4.

Everything else (migration, SQL, routes, API, frontend, tests, docs) is
Claude's.

## Planned versions and the shopping list

- **Settings → Physical deck → "Planned"** (`deck_version.is_planned`,
  migration 0026): a plain version you intend to build (the next
  tournament). Built and planned versions are the ones whose missing cards
  count.
- **`/decks/shopping`** (Decks menu, "Shopping list"): the cards missing from
  the built and planned versions (`missing_for_all`), with
  - the cost per deck (market prices) and the total,
  - **one Cardmarket wants list** for all of it (taking over the global one
    from `/decks/built`; each version's own list stays in its To buy tab),
  - the errata warnings that `/decks/built` shows today,
  - the budget left this month (`budget::left_this_month`) with a link to
    `/budget`.

The shopping page is built on the new "missing" rule, so it **ships after
this redesign**. If you'd rather have it sooner, it can be built first on
today's proxies (built versions only, from `built::ShoppingLine`s, which
already exist), then re-pointed at `missing_for_all`.

## Order of work

1. Migration 0026 + `deck_version_card` read/write in `store/` (lists), the
   API loading lines; the builder shows lines (plain versions only; built
   versions still through `card` rows until step 3).
2. The owner's `availability` and `missing`; ownership shown in the builder
   (replaces the reserve).
3. Building on lists (`deck_version_copy`, the owner's `plan_build`); remove
   proxies, `replace_proxies`, `clean-cards`' proxy steps.
4. Valuation by deck, built decks/errata, the card page, tests moved.
5. `is_planned` and `/decks/shopping`.

Each step: migration (if any) tried on a copy of `data/app.db`, `cargo test`,
`npm run check`, docs (`tour.md`, `guide.md`, CLAUDE.md) in the same commit.

## Questions for the owner

- Is a version's "missing" mark enough for a built deck, or do you want the
  physical proxy (a printed copy) tracked? (This spec assumes the mark.)
- Should two plain lists be allowed to "want" the same copy at once? (Yes
  here; only built versions lock.)
- Keep `card.is_proxy` data for old lots? (Spec: drop after one release.)
