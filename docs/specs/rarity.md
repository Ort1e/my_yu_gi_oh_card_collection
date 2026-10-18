# Spec: card rarity (backend)

**Status: done (2026-10-18)**, by Claude this time. What differs from the
plan below:
- the Cardmarket codes are **not** ygoprodeck's for six rarities:
  `SUR` Super Rare, `SER` Secret Rare, `QSCR` Quarter Century Secret Rare,
  `PSCR` Platinum Secret Rare, `SLR` Starlight Rare, `PGR` Premium Gold
  Rare (`Rarity::from_cardmarket_code`). Found by pairing each code with the
  `(V.x - …)` suffix of the same lines, and with the set rarity stored in
  `card_data` for the codes that never had one; 625 of the 626 cards of the
  real shipments get a rarity (the last one has a stray `T`). `PIR` really
  is Platinum Rare (ygoprodeck writes `(PIR)`);
- the Duel Terminal parallel rares are four variants;
- a shipment card keeps its raw `rarity_code` too, for the warning;
- `split_cardmarket_name` returns `(&str, Option<Rarity>)`; 176 cards had
  the suffix on 2026-10-18.

Each physical card gets a rarity ("Ultra Rare"…). Decided on 2026-10-13:
- stored as **text** in a new nullable column, **no CHECK constraint**; the known
  rarities are a list in `domain/` that the forms validate against;
- the 172 existing names like `Raigeki (V.4 - Ultra Rare)` get their rarity
  filled **and the suffix stripped** (`Raigeki`);
- the frontend (rarity field in the forms, badges, filter, recognition guide)
  is Claude's, built against the names below.

Steps, each one shippable on its own:

## 1. The column (migration `0004`)

`migrations/0004_card_rarity.sql`:

```sql
ALTER TABLE card ADD COLUMN rarity TEXT;
```

Nullable: most cards have no known rarity. It runs on `data/app.db` at the
next start (commit the data afterwards).

## 2. The list of rarities (`domain/src/card.rs` or a new `domain/src/rarity.rs`)

Like `CardStatus`: an enum with the stored label, and conversions from the
two outside spellings.

The full list, from the rarity names ygoprodeck uses across all its cards
(checked 2026-10-13; count of printings in brackets). The frontend guide
(`static/js/rarities.js`) describes the same names.

| Stored (`label()`) | ygoprodeck `set_rarity` (code) |
|---|---|
| Common | Common (C) [18117] |
| Short Print | Short Print (SP) |
| Super Short Print | Super Short Print (SSP) |
| Rare | Rare (R) |
| Super Rare | Super Rare (SR) |
| Ultra Rare | Ultra Rare (UR) |
| Ultra Rare (Pharaoh's Rare) | Ultra Rare (Pharaoh's Rare) |
| Ultimate Rare | Ultimate Rare (UtR) |
| Secret Rare | Secret Rare (ScR) |
| Ultra Secret Rare | Ultra Secret Rare (UScR) |
| Extra Secret Rare | Extra Secret Rare, Extra Secret |
| 10000 Secret Rare | 10000 Secret Rare (10000ScR) |
| Ghost Rare | Ghost Rare (GR) |
| Gold Rare | Gold Rare (GUR) |
| Gold Secret Rare | Gold Secret Rare (GScR) |
| Ghost/Gold Rare | Ghost/Gold Rare (GGR) |
| Premium Gold Rare | Premium Gold Rare (PG) |
| Platinum Rare | Platinum Rare (PIR) |
| Platinum Secret Rare | Platinum Secret Rare (PS), **PLatinum** Secret Rare (sic) |
| Prismatic Secret Rare | Prismatic Secret Rare (PScR) |
| Collector's Rare | Collector's Rare (CR) |
| Starlight Rare | Starlight Rare (StR) |
| Quarter Century Secret Rare | Quarter Century Secret Rare |
| Grand Master Rare | Grand Master Rare (new in 2026, Magnificent Monsters) |
| Starfoil Rare | Starfoil Rare (SFR), Starfoil |
| Shatterfoil Rare | Shatterfoil Rare (SHR) |
| Mosaic Rare | Mosaic Rare (MSR) |
| Normal Parallel Rare | Normal Parallel Rare |
| Super Parallel Rare | Super Parallel Rare (SPR) |
| Ultra Parallel Rare | Ultra Parallel Rare (UPR) |
| Duel Terminal … Parallel Rare | the four "Duel Terminal … Parallel Rare" (DNPR, DRPR, DSPR, DUPR): one variant each, or a single "Duel Terminal Parallel Rare" if you prefer |

Not rarities (ygoprodeck uses them as notes): `New`, `2`, `3`, `Reprint`,
`New artwork`, `European debut`, `Oceanian debut`, `European & Oceanian debut`,
`Cr`, `force-SMW`. Treat them as unknown (`None`).

The **Cardmarket codes** on the shipment lines are probably close to the codes
in brackets, but that's a guess: check them against your PDFs.

- `from_label(&str)`: case-insensitive, also accepting `Collectors Rare`
  (Cardmarket's spelling in your names) and ygoprodeck's variants above (the
  frontend's `findRarity` ignores case and punctuation: the same trick works).
- `from_code(&str)` for the shipment codes. **The Cardmarket codes are a
  guess** (ygoprodeck's codes, without parentheses): check them against your
  PDFs, e.g. print the set of codes found in `real_shipments` (codes aren't
  personal data, never print the rest of the line).
- An unknown rarity is not an error: keep `None` and `tracing::warn!` it.
- `ALL` (for the select in the forms), like `CardStatus::ALL`.

## 3. Reading and writing it

Names the frontend will use, so they must match:

| Where | Name |
|---|---|
| card row of the lot forms | field `cards-<i>-rarity` → `CardRowValues.rarity`, `NewCards.rarity: Option<Rarity>` (validated like `status`), written by `cards::insert_lot_cards` |
| card edit form | field `rarity` (`cards/form.rs`) |
| page row structs | `CardRow.rarity: Option<String>` (+ `CardRow::COLUMNS`), lot detail's `UniteRow` |
| deck builder API | `ApiCard.rarity: Option<String>` (`decks/api/mod.rs`) |
| cards filter | `CardFilter.rarity` (`/cards?rarity=Ultra%20Rare`), exact match |
| templates | a `fn rarities(&self)` on the page structs that show the select, like `statuses()` |

## 4. Shipment imports (`domain/src/cardmarket.rs`)

After the set code, a card line has the rarity code then the edition
(`… LOB UR First Edition 1,00 EUR`): `CARD` skips them with `.*?`. Capture
the first token (`(?P<rarity>\S+)`) into a new `ShipmentCard.rarity`, and
strip the `(V.x - …)` suffix from `name` (step 5's function). The tests in
that file use synthetic text: add the rarity to it.

## 5. Filling the 172 existing cards (`clean-cards`)

A pure function in `domain/`, unit-tested on the names you have:

```rust
/// "Raigeki (V.4 - Ultra Rare)" → ("Raigeki", Some(Rarity::UltraRare))
pub fn split_cardmarket_name(name: &str) -> (&str, Option<Rarity>)
```

- the suffix is ` (V.<n> - <rarity>)` at the **end** of the name; the rarity
  part is matched against the list;
- careful: some card names contain ` - ` themselves, so anchor on `(V.` and the
  final `)`, not on the first ` - `;
- a `V.` suffix with an unknown rarity: strip nothing, warn.

Then a step in `clean-cards` (`commands/clean_cards.rs`, with `--dry-run`
showing what would change): for each card whose name has the suffix and whose
`rarity` is NULL, set `name` and `rarity`. `en_name` doesn't change (it never
had the suffix).

## Later

- The set lookup (`docs/specs/set-lookup.md`) already returns each code's
  `rarities`: the quick-add panel will offer only those.
