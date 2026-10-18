# Spec: set lookup endpoint (`GET /api/sets/:code`)

Backend half of "quick add from a set" (TODO.md, faster card entry). The
frontend panel (Claude) types a set code once, then card numbers, and fills
the lot's card rows from this endpoint. **Done (2026-10-18)**: `domain::card_set`
by the owner, `fetch_set` (with the cache) and the handler by Claude.

## What it returns

`GET /api/sets/RA05` (the set's prefix, without the language part
`-FR`/`-EN`, case-insensitive):

```json
{
  "code": "RA05",
  "name": "Rarity Collection 5",
  "cards": [
    {
      "number": "001",
      "ygopro_id": 39111158,
      "name_en": "Tri-Horned Dragon",
      "name_fr": "Dragon à Trois Cornes",
      "rarities": ["Starlight Rare"]
    }
  ]
}
```

- `cards` sorted by `number`, one entry per number.
- `number`: the part after `-`, without the language: `RA05-EN037` → `"037"`.
  ygoprodeck codes are always English: drop a leading `EN`, or a lone `E`
  followed by a digit (old sets: `LOB-E001` → `"001"`); keep anything else, it
  is part of the number (`ABPF-ENSE1` → `"SE1"`, `SDY-046` → `"046"`). Keep it a
  string: the leading zeros matter.
- `name_fr`: `null` when ygoprodeck has no French name (49 of the 150 RA05
  cards, e.g. `037` "Albion the Sanctifire Dragon").
- `rarities`: every rarity ygoprodeck lists for that code, without duplicates.
  Not used by the first version of the panel, but needed by the rarity TODO;
  cheap to include now.
- Unknown set code → **400** `{"error": "Unknown set code RA99."}` via
  `bad_request(...)`, so the panel can show the message.
- Read-only: the route goes in `routes::router()`, not `write_router()`.

## Where the data comes from (ygoprodeck)

Checked on 2026-10-13:

1. `GET https://db.ygoprodeck.com/api/v7/cardsets.php`: every set,
   `[{"set_name": "Rarity Collection 5", "set_code": "RA05", "num_of_cards": 150, …}, …]`.
   Find the entries whose `set_code` equals the prefix → their `set_name`s.
   **Several sets can share a prefix**: `ABPF` is "Absolute Powerforce", its
   "Special Edition" and a "Sneak Peek Participation Card". Use the one with the
   most cards (`num_of_cards`) for the response's `name`, and fetch them all
   (steps 2–3 per set) so the special-edition cards (`ABPF-ENSE1`) are there too.
2. `GET …/cardinfo.php?cardset=<set_name>`: every card of the set (150 for
   RA05), `{"data": [card, …]}`. In each card, `card_sets` lists all its
   printings: keep the entries whose `set_code` starts with `"RA05-"`; each
   gives `set_code` and `set_rarity` (one entry per rarity, so the same code
   appears several times). `name` is the English name, `id` the ygopro id.
3. `GET …/cardinfo.php?cardset=<set_name>&language=fr`: same cards, French
   `name` (codes stay English). Match French to English **by `id`**. Cards
   without a French name are simply missing from it. If it answers an error
   (ygoprodeck uses 400 + `{"error": …}` for "no result"), treat it as "no French
   names", not as a failure.

Use `query(&[("cardset", name)])` (reqwest encodes the spaces) and call
`throttle()` before each request, like `fetch()` does.

## Where the code goes

The skeleton is in place, wired and compiling: the bodies are `todo!()`, each
with an `#[allow(...)] // TODO: remove once …` to delete when it's done.

| Piece | Signature (already there) | Copy from |
|---|---|---|
| Number rule | `domain/src/card_set.rs`: `pub fn card_number(code: &str) -> Option<String>` | its doc comment lists the cases |
| Reshaping the JSON | `domain/src/card_set.rs`: `pub fn set_cards(prefix: &str, en: &[Value], fr: &[Value]) -> Vec<SetCard>` (`SetCard` is defined) | `domain/src/ban_list.rs` (also parses JSON) |
| The HTTP calls | `backend/src/store/card_data.rs`: `pub async fn fetch_set(http: &reqwest::Client, prefix: &str) -> AppResult<Option<FetchedSet>>` (`FetchedSet { name, en, fr }` is defined) | `fetch()` in the same file (throttle, error mapping, 400 handling) |
| The handler | `backend/src/routes/reference/sets.rs`: `pub async fn set_cards(State(state), Path(code)) -> AppResult<Json<SetResponse>>`, route `/api/sets/:code` already in `routes::router()` | `decks::api::categories` for a small JSON handler |

Until it's implemented, calling the route panics on the `todo!()` (the
request fails, the server keeps running) and the panel says it can't load
the set.

Why the split: the HTTP part can't be tested without the network (`TestApp`
never calls ygoprodeck), but the reshaping can, with **unit tests on small
synthetic JSON** in `domain/`: two cards, one with two rarities, one without a
French name, a code from another set to be ignored, a `…-ENSE1` and an old
`…-E001` code for the number rule. That's where the bugs would be.

## Cache (required)

ygoprodeck allows about 20 requests per second and blocks the IP for an
hour above that. `throttle()` (60 ms between requests, shared by every
ygoprodeck call of the server) already keeps us under it, but each panel
load still costs 3 requests per set (more when several sets share the
prefix), and ygoprodeck asks API users to keep what they fetch. So
`fetch_set` caches its answers in memory:

```rust
/// Answers of `fetch_set` by prefix (uppercase). A released set doesn't
/// change; a restart empties it.
static SET_CACHE: Mutex<BTreeMap<String, FetchedSet>> = Mutex::const_new(BTreeMap::new());
```

- next to `LAST_REQUEST` in `store/card_data.rs` (the same `tokio::sync::Mutex`);
  `FetchedSet` gets `#[derive(Clone)]`;
- look the prefix up first and return a clone when it's there; otherwise
  fetch, store, return. Don't hold the lock during the requests (look up,
  release, fetch, then lock again to insert): two loads of the same new set
  at once just fetch it twice, which is fine;
- **only successful answers** are stored: an unknown prefix (`Ok(None)`) or
  an error is fetched again next time, so a set released today works today;
- log each real fetch, not the cache hits:
  `tracing::info!(prefix, cards = n, "set fetched from ygoprodeck")`;
- no expiry: a set never changes once released. `cardsets.php` (the list of
  every set, ~1000 entries, a new set now and then) may be cached the same
  way for a day (`Instant` stored with it) — optional.

The handler doesn't change. Test: nothing automatic (no network in tests);
by hand, the second `curl /api/sets/RA05` answers at once, and the log shows
a single `set fetched from ygoprodeck` line.

## Nice to have (later)

- The frontend could offer the `cardsets.php` list as suggestions.

## What the panel does with it (frontend, for reference)

The user types `RA05-FR` → the panel calls `/api/sets/RA05` once and keeps the
answer. Typing `37` + Enter adds a card row: name = `name_fr` (or `name_en`),
English name = `name_en`, code = `RA05-FR037` (the prefix as typed + number),
status = the panel's default; the same number again bumps that row's quantity.
Nothing else changes: saving the lot works as today, and the background task
links the new cards to their card data through `name_en`.
