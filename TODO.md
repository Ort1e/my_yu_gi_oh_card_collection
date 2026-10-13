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
- [ ] Manage the rarity of the cards (Common, Super, Ultra, Secret, Quarter
      Century…). Today it only ends up in the card name (`Raigeki (V.4 - Ultra
      Rare)`, from Cardmarket), so it can't be filtered or set when adding
      cards by hand. To design: a rarity field on the card? Taken from the set
      code via ygoprodeck (a code can exist in several rarities)?
  - [ ] Help to tell the rarity of a card in hand when its code exists in
        several rarities (e.g. Rarity Collection sets). When a code is typed:
    - list the rarities that code exists in (ygoprodeck `card_sets`: one
      entry per code and rarity, with an indicative price);
    - next to each, how to recognise it on the physical card (name colour
      and foil, foil art or not, foil pattern, stamp…), from a small rarity
      guide kept in the app;
    - pick the rarity from that list instead of typing it.
