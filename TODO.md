# TODO

- [x] Clean up the interface artifacts.
- [x] Add logs.
- [x] Run cleanups automatically instead of by hand (`clean-cards`), in the
      background or as part of the action that needs them (the command
      stays for a manual run):
  - [x] remove unused proxies automatically: when saving zones, and after
        new cards replace deck proxies;
  - [x] the whole `clean-cards` in the background, 5 minutes after the
        server starts then once a day (mainly: names ygoprodeck translated
        since);
  - [x] update the cards imported when a lot is added: card data links,
        then names in the language of their code (background task after
        saving).
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
    - [x] backend endpoint `GET /api/sets/:code` (spec
          `docs/specs/set-lookup.md`), cached in memory.
- [x] Manage the rarity of the cards (Common, Super, Ultra, Secret, Quarter
      Century…): column `card.rarity` (migration 0004), list in
      `domain/src/rarity.rs`, read from the shipments (Cardmarket codes
      checked on the real PDFs), filled from the `(V.x - …)` names by
      `clean-cards`, field in the forms, badges, `/cards` filter. Spec and
      notes: `docs/specs/rarity.md`.
  - [x] quick-add panel: each row offers only its code's rarities (one
        rarity: selected; several: one row per copy); set codes and full
        codes must carry the language.
- [ ] Link decks together: a *built* flag on deck versions; the cards of a
      built version are locked (gone from the collection for every other
      deck; where other decks used them, they become proxies there), and
      `/decks/built` lists the built versions and their missing cards.
      Design: `docs/specs/built-decks.md` (deck box and sleeves too);
      backend (owner), then frontend.
- [x] Refactor the code base to make it easier to read: plan and what was
      done in `docs/specs/refactor.md`.
