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
    `partials/quick_add.html`, `static/js/card_rows.js`).
    - [ ] backend endpoint `GET /api/sets/:code`, spec in
          `docs/specs/set-lookup.md` (owner): until then the panel says the
          set can't be loaded.
- [x] Manage the rarity of the cards (Common, Super, Ultra, Secret, Quarter
      Century…): column `card.rarity` (migration 0004), list in
      `domain/src/rarity.rs`, read from the shipments (Cardmarket codes
      checked on the real PDFs), filled from the `(V.x - …)` names by
      `clean-cards`, field in the forms, badges, `/cards` filter. Spec and
      notes: `docs/specs/rarity.md`.
  - [ ] once `GET /api/sets/:code` exists: in the quick-add panel, offer
        only the code's rarities (today a code with a single rarity fills it
        in, several are listed under the panel).
- [ ] Link decks together. Today each deck version uses the whole
      collection, so the same card can be in several versions at once. Goal:
      build several decks at the same time from one shared collection,
      without overlap (each card owned is used by only one of the linked
      decks), and track the cards still missing across all of them.
- [x] Refactor the code base to make it easier to read: plan and what was
      done in `docs/specs/refactor.md`.
