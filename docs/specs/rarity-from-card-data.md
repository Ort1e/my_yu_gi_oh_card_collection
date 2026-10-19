# Rarity from the card data (`clean-cards` step 7)

For: the owner (backend). Status: not started.

## Why

On 2026-10-19, 883 cards had no rarity. 175 are proxies (no rarity, on purpose). For the
708 others, what ygoprodeck says about their code (the `card_sets` of their card data):

| Rarities of the card's code at ygoprodeck | Cards |
|---|---|
| only Common | 211 |
| only one other rarity (Super 167, Rare 81, Ultra 57, Platinum 46, Secret 35, Prismatic 23…) | ~430 |
| several (Collector's / Rare, Starlight / Super…) | ~25 |
| code not listed at ygoprodeck | 40 |
| odd values given as the rarity (`"2"`, `"3"`) | 14 |

So "no rarity = Common" would be wrong for most cards. Instead: when a code was printed in
**one rarity only**, give the card that rarity (~650 cards, the 211 Commons included). The
others (~65) stay empty, to set by hand. Proxies are left alone.

No network: everything is in the database already (`card.code`, and `card_data.json_data`
holding `card_sets: [{set_code, set_rarity}, …]`), so it runs in the existing transaction.

## 1. Pure part: `domain/src/card_set.rs`

```rust
/// A code without its language, to compare a card with ygoprodeck's
/// `set_code`s: "MAMO-FR023" → ("MAMO", "023"), "LOB-F001" → ("LOB", "001"),
/// "SDY-046" → ("SDY", "046").
pub fn code_without_language(code: &str) -> Option<(String, String)>

/// The rarity of a card coded `code` from its ygoprodeck JSON, when that
/// code was printed in one rarity only. Several rarities, an unknown code
/// or an unknown rarity name → None.
pub fn single_rarity(json: &Value, code: &str) -> Option<Rarity>
```

- The cards are coded `MAMO-FR023` but ygoprodeck lists the English codes (`MAMO-EN023`):
  compare both codes **without the language**. `card_number` only strips `EN`; use
  `Language::of_code` (`domain/src/card.rs`) and cut its `code()` / `old_code()`.
- Collect the distinct `set_rarity` of the matching entries; exactly one → `Rarity::from_label`
  (that also skips the odd `"2"`/`"3"` values: no matching rarity).
- Unit tests with synthetic JSON: `-FR` code vs `-EN` set, one rarity, several, code not
  listed, `"2"`, old code `LOB-F001`.

## 2. Step 7: `backend/src/commands/clean_cards.rs`

Copy `rarities_from_names`:

```rust
/// Set the rarity of the cards whose code was printed in one rarity only,
/// from their card data. Returns how many cards changed.
async fn rarities_from_card_data(conn: &mut SqliteConnection) -> anyhow::Result<usize> {
    // SELECT c.id, c.code, d.json_data FROM card c JOIN card_data d ON d.id = c.card_data_id
    // WHERE c.rarity IS NULL AND c.is_proxy = 0 AND c.code IS NOT NULL
    // each row: serde_json::from_str, single_rarity(...), Some → UPDATE card SET rarity = ? WHERE id = ?
}
```

- In `run()`, call it **right after `rarities_from_names`**: a `(V.x - …)` name is more
  reliable than ygoprodeck, and by then step 2 has relinked every card to its card data.
- `Report`: new field (e.g. `rarities_from_data`), a `println!` in `print()`
  ("rarities taken from card data: N") and a field in `log()`.
- Module doc comment: point 7, and "new" in the differences from Django.
- Test: like `moves_the_rarity_out_of_cardmarket_names`. A `card_data` with synthetic
  `card_sets`, cards with one rarity / several / a proxy / a rarity already set: only the
  first changes.

## Check

`cargo run -p backend -- clean-cards --dry-run` should report about 650 rarities on the live
data. Then delete this spec (move what's useful into the doc comments).
