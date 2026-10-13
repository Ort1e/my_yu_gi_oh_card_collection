# TODO

- [x] Clean up the interface artifacts.
- [x] Add logs.
- [ ] Run cleanups automatically instead of by hand (`clean-cards`), in the
      background or as part of the action that needs them:
  - [ ] remove unused proxies automatically;
  - [ ] update the cards imported when a lot is added (names, card data
        links).
- [ ] Faster way to add many cards to a lot, e.g. the pulls from a box of
      boosters of one set. A new lot is fine, but adding the cards one row at
      a time is long. Ideas:
  - type only the set code once (`RA05-FR`), then just the card numbers;
  - fill in the name from the code (ygoprodeck lists each card's sets with
    their English codes, `RA05-EN001`; the number is the same in `RA05-FR001`);
  - paste a list (one card per line: code, or name, with an optional
    quantity and status).
  - Status: the "Quick add from a set" panel is done on "Add a lot" and on a
    lot's page, which now takes several cards at once (frontend:
    `_quick_add.html`, `static/js/card_rows.js`).
    - [ ] backend endpoint `GET /api/sets/:code`, spec in
          `docs/specs/set-lookup.md` (owner): until then the panel says the
          set can't be loaded.
- [ ] Manage the rarity of the cards (Common, Super, Ultra, Secret, Quarter
      Century…). Today it only ends up in the card name (`Raigeki (V.4 - Ultra
      Rare)`, from Cardmarket). Decided: a nullable text column + a list in
      `domain/`, and the suffix stripped from those names.
  - [ ] backend (owner): column, rarity list, forms, shipment import, filling
        the 172 existing cards; spec in `docs/specs/rarity.md`;
  - [x] recognition guide: `/rarities` page, and the rarities of a code with
        several, listed in the quick-add panel (`static/js/rarities.js`);
  - [ ] frontend once the column exists: rarity field in the card rows (quick
        add: offer only the code's rarities), badges, `/cards` filter.
